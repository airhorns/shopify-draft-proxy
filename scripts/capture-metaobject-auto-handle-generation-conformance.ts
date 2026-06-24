/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-auto-handle-generation.json');
const specPath = path.join('config', 'parity-specs', 'metaobjects', 'metaobject-auto-handle-generation.json');
const runId = Date.now().toString();
const type = `auto_handle_${runId}`;
const randomHandleBase = type.replaceAll('_', '-').toLowerCase();
const explicitHandle = 'MyHandle';
const explicitConflictHandle = explicitHandle.toLowerCase();
const upsertHandle = 'UpsertHandle';

const requestPaths = {
  definitionCreate: 'config/parity-requests/metaobjects/metaobject-auto-handle-generation-definition-create.graphql',
  create: 'config/parity-requests/metaobjects/metaobject-auto-handle-generation-create.graphql',
  upsert: 'config/parity-requests/metaobjects/metaobject-auto-handle-generation-upsert.graphql',
};

const documents = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const metaobjectDeleteMutation = `#graphql
  mutation MetaobjectAutoHandleDelete($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const metaobjectDefinitionDeleteMutation = `#graphql
  mutation MetaobjectAutoHandleDefinitionDelete($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(part, 10);
      current = Number.isInteger(index) ? current[index] : undefined;
      continue;
    }

    const object = readObject(current);
    if (!object) {
      return undefined;
    }
    current = object[part];
  }
  return current;
}

function readUserErrors(payload: unknown, root: string): unknown[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value : [];
}

function extractString(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }
  return value;
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, root: string, label: string): void {
  const userErrors = readUserErrors(payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

function titleizeHandle(handle: string): string {
  return handle
    .split(/[-_]/u)
    .filter((part) => part.length > 0)
    .map((part) => `${part.slice(0, 1).toUpperCase()}${part.slice(1).toLowerCase()}`)
    .join(' ');
}

function generatedDisplayNameFor(handle: string): string {
  const match = /^(.+)-(\w{8})$/u.exec(handle);
  if (!match) {
    throw new Error(`Generated handle did not include an 8-character suffix: ${handle}`);
  }
  return `${titleizeHandle(match[1] ?? '')} #${(match[2] ?? '').toUpperCase()}`;
}

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
}

function assertGeneratedHandle(capture: Capture, label: string): string {
  const handle = extractString(capture.response, ['data', 'metaobjectCreate', 'metaobject', 'handle'], label);
  const displayName = extractString(capture.response, ['data', 'metaobjectCreate', 'metaobject', 'displayName'], label);
  const pattern = new RegExp(`^${escapeRegex(randomHandleBase)}-[a-z0-9]{8}$`, 'u');
  if (!pattern.test(handle)) {
    throw new Error(`${label} returned non-Core-shaped generated handle ${handle}`);
  }
  const expectedDisplayName = generatedDisplayNameFor(handle);
  if (displayName !== expectedDisplayName) {
    throw new Error(`${label} returned displayName ${displayName}, expected ${expectedDisplayName}`);
  }
  return handle;
}

function assertExplicitHandle(capture: Capture, expectedHandle: string, label: string): void {
  const handle = extractString(capture.response, ['data', 'metaobjectCreate', 'metaobject', 'handle'], label);
  if (handle !== expectedHandle) {
    throw new Error(`${label} returned handle ${handle}, expected ${expectedHandle}`);
  }
}

function createVariables(handle?: string): Record<string, unknown> {
  return {
    metaobject: {
      type,
      ...(handle === undefined ? {} : { handle }),
      fields: [{ key: 'body', value: handle === undefined ? 'Generated display name fallback' : `Explicit ${handle}` }],
    },
  };
}

function upsertVariables(handle: string, fieldValue: string): Record<string, unknown> {
  return {
    handle: { type, handle },
    metaobject: {
      fields: [{ key: 'body', value: fieldValue }],
    },
  };
}

function assertUpsertFallbackDisplayName(capture: Capture, label: string): void {
  const handle = extractString(capture.response, ['data', 'metaobjectUpsert', 'metaobject', 'handle'], label);
  const displayName = extractString(capture.response, ['data', 'metaobjectUpsert', 'metaobject', 'displayName'], label);
  if (handle !== upsertHandle.toLowerCase()) {
    throw new Error(`${label} returned handle ${handle}, expected ${upsertHandle.toLowerCase()}`);
  }
  if (displayName !== 'Upsert Handle') {
    throw new Error(`${label} returned displayName ${displayName}, expected Upsert Handle`);
  }
}

async function cleanup(
  createdMetaobjectIds: string[],
  definitionIds: string[],
  cleanupCaptures: Capture[],
): Promise<void> {
  for (const id of createdMetaobjectIds) {
    cleanupCaptures.push(await captureGraphql('cleanup-metaobject-delete', metaobjectDeleteMutation, { id }));
  }
  for (const id of definitionIds) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-metaobject-definition-delete', metaobjectDefinitionDeleteMutation, { id }),
    );
  }
}

function generatedHandleDifferences() {
  return [
    {
      path: '$.id',
      matcher: 'shopify-gid:Metaobject',
      reason: 'The local proxy stages a synthetic metaobject ID while Shopify returns the captured live-store ID.',
    },
    {
      path: '$.handle',
      matcher: `regex:^${escapeRegex(randomHandleBase)}-[a-z0-9]{8}$`,
      reason:
        'Shopify and the proxy both generate Core-shaped random handle suffixes; the concrete random value is volatile.',
    },
    {
      path: '$.displayName',
      matcher: `regex:^${escapeRegex(titleizeHandle(randomHandleBase))} #[A-Z0-9]{8}$`,
      reason:
        'Display-name fallback includes the generated random code, so parity validates the Core display-name shape instead of the concrete volatile code.',
    },
  ];
}

function buildSpec(): Record<string, unknown> {
  return {
    scenarioId: 'metaobject-auto-handle-generation',
    operationNames: ['metaobjectDefinitionCreate', 'metaobjectCreate', 'metaobjectUpsert'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'runtime-staging'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/metaobject_definitions_test.gleam'],
    proxyRequest: {
      documentPath: requestPaths.definitionCreate,
      variablesCapturePath: '$.definitionCreate.request.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify evidence for metaobjectCreate handle generation when no handle is supplied and the definition has no displayNameKey, explicit mixed-case handle lowercasing, titleized display-name fallback, case-insensitive conflict suffixing, and metaobjectUpsert create/update fallback displayName derivation from the explicit handle.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'definition-create-setup',
          capturePath: '$.definitionCreate.response.data.metaobjectDefinitionCreate',
          proxyPath: '$.data.metaobjectDefinitionCreate',
          selectedPaths: ['$.metaobjectDefinition.type', '$.metaobjectDefinition.displayNameKey', '$.userErrors'],
        },
        {
          name: 'auto-create-one',
          capturePath: '$.autoCreateOne.response.data.metaobjectCreate',
          proxyPath: '$.data.metaobjectCreate',
          selectedPaths: ['$.metaobject', '$.userErrors'],
          proxyRequest: {
            documentPath: requestPaths.create,
            variablesCapturePath: '$.autoCreateOne.request.variables',
            apiVersion,
          },
          expectedDifferences: generatedHandleDifferences(),
        },
        {
          name: 'auto-create-two',
          capturePath: '$.autoCreateTwo.response.data.metaobjectCreate',
          proxyPath: '$.data.metaobjectCreate',
          selectedPaths: ['$.metaobject', '$.userErrors'],
          proxyRequest: {
            documentPath: requestPaths.create,
            variablesCapturePath: '$.autoCreateTwo.request.variables',
            apiVersion,
          },
          expectedDifferences: generatedHandleDifferences(),
        },
        {
          name: 'explicit-mixed-case',
          capturePath: '$.explicitMixedCaseCreate.response.data.metaobjectCreate',
          proxyPath: '$.data.metaobjectCreate',
          selectedPaths: ['$.metaobject.handle', '$.metaobject.type', '$.metaobject.displayName', '$.userErrors'],
          proxyRequest: {
            documentPath: requestPaths.create,
            variablesCapturePath: '$.explicitMixedCaseCreate.request.variables',
            apiVersion,
          },
        },
        {
          name: 'explicit-case-conflict',
          capturePath: '$.explicitCaseConflictCreate.response.data.metaobjectCreate',
          proxyPath: '$.data.metaobjectCreate',
          selectedPaths: ['$.metaobject.handle', '$.metaobject.type', '$.metaobject.displayName', '$.userErrors'],
          proxyRequest: {
            documentPath: requestPaths.create,
            variablesCapturePath: '$.explicitCaseConflictCreate.request.variables',
            apiVersion,
          },
        },
        {
          name: 'upsert-create-explicit-handle-fallback-display-name',
          capturePath: '$.upsertCreate.response.data.metaobjectUpsert',
          proxyPath: '$.data.metaobjectUpsert',
          selectedPaths: ['$.metaobject.handle', '$.metaobject.type', '$.metaobject.displayName', '$.userErrors'],
          proxyRequest: {
            documentPath: requestPaths.upsert,
            variablesCapturePath: '$.upsertCreate.request.variables',
            apiVersion,
          },
        },
        {
          name: 'upsert-update-explicit-handle-fallback-display-name',
          capturePath: '$.upsertUpdate.response.data.metaobjectUpsert',
          proxyPath: '$.data.metaobjectUpsert',
          selectedPaths: ['$.metaobject.handle', '$.metaobject.type', '$.metaobject.displayName', '$.userErrors'],
          proxyRequest: {
            documentPath: requestPaths.upsert,
            variablesCapturePath: '$.upsertUpdate.request.variables',
            apiVersion,
          },
        },
      ],
    },
  };
}

const cleanupCaptures: Capture[] = [];
const createdMetaobjectIds: string[] = [];
const definitionIds: string[] = [];
let definitionCreate: Capture | null = null;
let autoCreateOne: Capture | null = null;
let autoCreateTwo: Capture | null = null;
let explicitMixedCaseCreate: Capture | null = null;
let explicitCaseConflictCreate: Capture | null = null;
let upsertCreate: Capture | null = null;
let upsertUpdate: Capture | null = null;

try {
  definitionCreate = await captureGraphql('definition-create', documents.definitionCreate, {
    definition: {
      type,
      name: `Auto Handle ${runId}`,
      fieldDefinitions: [{ key: 'body', name: 'Body', type: 'single_line_text_field', required: false }],
    },
  });
  assertNoUserErrors(definitionCreate.response, 'metaobjectDefinitionCreate', 'definition-create');
  const definitionId = extractString(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition-create',
  );
  definitionIds.push(definitionId);

  autoCreateOne = await captureGraphql('auto-create-one', documents.create, createVariables());
  assertNoUserErrors(autoCreateOne.response, 'metaobjectCreate', 'auto-create-one');
  const autoOneHandle = assertGeneratedHandle(autoCreateOne, 'auto-create-one');
  createdMetaobjectIds.push(
    extractString(autoCreateOne.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'auto-create-one'),
  );

  autoCreateTwo = await captureGraphql('auto-create-two', documents.create, createVariables());
  assertNoUserErrors(autoCreateTwo.response, 'metaobjectCreate', 'auto-create-two');
  const autoTwoHandle = assertGeneratedHandle(autoCreateTwo, 'auto-create-two');
  if (autoTwoHandle === autoOneHandle) {
    throw new Error(`auto-create-two reused generated handle ${autoTwoHandle}`);
  }
  createdMetaobjectIds.push(
    extractString(autoCreateTwo.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'auto-create-two'),
  );

  explicitMixedCaseCreate = await captureGraphql(
    'explicit-mixed-case',
    documents.create,
    createVariables(explicitHandle),
  );
  assertNoUserErrors(explicitMixedCaseCreate.response, 'metaobjectCreate', 'explicit-mixed-case');
  assertExplicitHandle(explicitMixedCaseCreate, explicitConflictHandle, 'explicit-mixed-case');
  createdMetaobjectIds.push(
    extractString(
      explicitMixedCaseCreate.response,
      ['data', 'metaobjectCreate', 'metaobject', 'id'],
      'explicit-mixed-case',
    ),
  );

  explicitCaseConflictCreate = await captureGraphql(
    'explicit-case-conflict',
    documents.create,
    createVariables(explicitConflictHandle),
  );
  assertNoUserErrors(explicitCaseConflictCreate.response, 'metaobjectCreate', 'explicit-case-conflict');
  assertExplicitHandle(explicitCaseConflictCreate, `${explicitConflictHandle}-1`, 'explicit-case-conflict');
  createdMetaobjectIds.push(
    extractString(
      explicitCaseConflictCreate.response,
      ['data', 'metaobjectCreate', 'metaobject', 'id'],
      'explicit-case-conflict',
    ),
  );

  upsertCreate = await captureGraphql(
    'upsert-create-explicit-handle-fallback-display-name',
    documents.upsert,
    upsertVariables(upsertHandle, 'Upsert display name fallback'),
  );
  assertNoUserErrors(upsertCreate.response, 'metaobjectUpsert', 'upsert-create-explicit-handle-fallback-display-name');
  assertUpsertFallbackDisplayName(upsertCreate, 'upsert-create-explicit-handle-fallback-display-name');
  createdMetaobjectIds.push(
    extractString(
      upsertCreate.response,
      ['data', 'metaobjectUpsert', 'metaobject', 'id'],
      'upsert-create-explicit-handle-fallback-display-name',
    ),
  );

  upsertUpdate = await captureGraphql(
    'upsert-update-explicit-handle-fallback-display-name',
    documents.upsert,
    upsertVariables(upsertHandle, 'Updated upsert display name fallback'),
  );
  assertNoUserErrors(upsertUpdate.response, 'metaobjectUpsert', 'upsert-update-explicit-handle-fallback-display-name');
  assertUpsertFallbackDisplayName(upsertUpdate, 'upsert-update-explicit-handle-fallback-display-name');

  await cleanup(createdMetaobjectIds, definitionIds, cleanupCaptures);
  createdMetaobjectIds.splice(0, createdMetaobjectIds.length);
  definitionIds.splice(0, definitionIds.length);

  await mkdir(outputDir, { recursive: true });
  await mkdir(path.dirname(specPath), { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'metaobjectCreate auto-handle generation with no displayNameKey, second same-type generated create, explicit mixed-case handle lowercasing with titleized display-name fallback, case-insensitive explicit handle conflict suffixing, and metaobjectUpsert create/update fallback displayName derivation from the explicit handle.',
        captureContext: {
          runId,
          type,
          randomHandleBase,
          explicitHandle,
          explicitConflictHandle,
          upsertHandle,
          definitionId,
        },
        definitionCreate,
        autoCreateOne,
        autoCreateTwo,
        explicitMixedCaseCreate,
        explicitCaseConflictCreate,
        upsertCreate,
        upsertUpdate,
        cleanup: cleanupCaptures,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
  console.log(`Wrote ${specPath}`);
} catch (error) {
  try {
    await cleanup(createdMetaobjectIds, definitionIds, cleanupCaptures);
  } catch (cleanupError) {
    cleanupCaptures.push({
      name: 'cleanup-failure',
      request: { query: '', variables: {} },
      status: 0,
      response: cleanupError instanceof Error ? cleanupError.message : String(cleanupError),
    });
  }
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobject-auto-handle-generation-blocker-${runId}.json`);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        captureContext: {
          runId,
          type,
          randomHandleBase,
          explicitHandle,
          explicitConflictHandle,
          upsertHandle,
          definitionIds,
          createdMetaobjectIds,
        },
        blocker: error instanceof Error ? error.message : String(error),
        partialCaptures: {
          definitionCreate,
          autoCreateOne,
          autoCreateTwo,
          explicitMixedCaseCreate,
          explicitCaseConflictCreate,
          upsertCreate,
          upsertUpdate,
        },
        cleanup: cleanupCaptures,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
  throw error;
}
