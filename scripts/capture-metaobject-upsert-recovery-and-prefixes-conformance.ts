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
const outputPath = path.join(outputDir, 'metaobject-upsert-recovery-and-prefixes.json');
const requestPath = path.join(
  'config',
  'parity-requests',
  'metaobjects',
  'metaobject-upsert-recovery-and-prefixes.graphql',
);
const runId = Date.now().toString();
const type = `codex_har678_upsert_${runId}`;
const primaryHandle = `har-678-primary-${runId}`;
const conflictHandle = `har-678-conflict-${runId}`;
const createHandle = `har-678-create-${runId}`;

const upsertDocument = await readFile(requestPath, 'utf8');

const definitionCreateMutation = `#graphql
  mutation MetaobjectUpsertRecoveryDefinitionCreate($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        id
        type
        name
        displayNameKey
        fieldDefinitions {
          key
          name
          required
          type {
            name
            category
          }
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const metaobjectDeleteMutation = `#graphql
  mutation MetaobjectUpsertRecoveryDelete($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const definitionDeleteMutation = `#graphql
  mutation MetaobjectUpsertRecoveryDefinitionDelete($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
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
    const object = readObject(current);
    if (!object) {
      return undefined;
    }
    current = object[part];
  }
  return current;
}

function extractString(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }
  return value;
}

function extractUserErrors(payload: unknown, pathParts: string[]): unknown[] {
  const value = readPath(payload, pathParts);
  return Array.isArray(value) ? value : [];
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const errors = extractUserErrors(payload, pathParts);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
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

function cloneJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function withUserErrorField(capture: Capture, field: string[]): Capture {
  const copy = cloneJson(capture);
  const userErrors = readPath(copy.response, ['data', 'metaobjectUpsert', 'userErrors']);
  if (Array.isArray(userErrors) && readObject(userErrors[0])) {
    (userErrors[0] as Record<string, unknown>)['field'] = field;
  }
  return copy;
}

function upsertVariables(handle: string, fields: Array<{ key: string; value: string }>, overrideHandle?: string) {
  return {
    handle: { type, handle },
    metaobject: {
      ...(overrideHandle ? { handle: overrideHandle } : {}),
      fields,
    },
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const cleanupMetaobjectIds = new Set<string>();
let definitionId: string | null = null;

async function runCapture(name: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await client.runGraphqlRequest(upsertDocument, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, upsertDocument, variables, result);
}

try {
  const definitionVariables = {
    definition: {
      type,
      name: `HAR-678 upsert ${runId}`,
      displayNameKey: 'title',
      fieldDefinitions: [
        {
          key: 'title',
          name: 'Title',
          type: 'single_line_text_field',
          required: true,
        },
      ],
    },
  };
  const definitionCreate = await client.runGraphqlRequest(definitionCreateMutation, definitionVariables);
  assertGraphqlOk(definitionCreate, 'definition create');
  assertNoUserErrors(
    definitionCreate.payload,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'definition create',
  );
  definitionId = extractString(
    definitionCreate.payload,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition create',
  );

  const primaryCreate = await runCapture(
    'create-primary',
    upsertVariables(primaryHandle, [{ key: 'title', value: `Primary ${runId}` }]),
  );
  assertNoUserErrors(primaryCreate.response, ['data', 'metaobjectUpsert', 'userErrors'], 'create-primary');
  const primaryId = extractString(
    primaryCreate.response,
    ['data', 'metaobjectUpsert', 'metaobject', 'id'],
    'create-primary',
  );
  cleanupMetaobjectIds.add(primaryId);

  const exactMatch = await runCapture(
    'exact-match-primary',
    upsertVariables(primaryHandle, [{ key: 'title', value: `Primary ${runId}` }], primaryHandle),
  );
  assertNoUserErrors(exactMatch.response, ['data', 'metaobjectUpsert', 'userErrors'], 'exact-match-primary');

  const createdUpdatedAt = extractString(
    primaryCreate.response,
    ['data', 'metaobjectUpsert', 'metaobject', 'updatedAt'],
    'create-primary',
  );
  const exactUpdatedAt = extractString(
    exactMatch.response,
    ['data', 'metaobjectUpsert', 'metaobject', 'updatedAt'],
    'exact-match-primary',
  );
  if (exactUpdatedAt !== createdUpdatedAt) {
    throw new Error(`exact-match upsert changed updatedAt: create=${createdUpdatedAt} exact=${exactUpdatedAt}`);
  }

  const conflictOwnerCreate = await runCapture(
    'create-conflict-owner',
    upsertVariables(conflictHandle, [{ key: 'title', value: `Conflict ${runId}` }]),
  );
  assertNoUserErrors(conflictOwnerCreate.response, ['data', 'metaobjectUpsert', 'userErrors'], 'create-conflict-owner');
  cleanupMetaobjectIds.add(
    extractString(
      conflictOwnerCreate.response,
      ['data', 'metaobjectUpsert', 'metaobject', 'id'],
      'create-conflict-owner',
    ),
  );

  const conflictingHandle = await runCapture(
    'conflicting-handle',
    upsertVariables(primaryHandle, [{ key: 'title', value: `Primary ${runId}` }], conflictHandle),
  );
  const missingRequired = await runCapture('missing-required-title', {
    handle: { type, handle: `har-678-missing-${runId}` },
    metaobject: { fields: [] },
  });

  const proxyCreate = await runCapture(
    'create-proxy-branch',
    upsertVariables(createHandle, [{ key: 'title', value: `Create ${runId}` }]),
  );
  assertNoUserErrors(proxyCreate.response, ['data', 'metaobjectUpsert', 'userErrors'], 'create-proxy-branch');
  cleanupMetaobjectIds.add(
    extractString(proxyCreate.response, ['data', 'metaobjectUpsert', 'metaobject', 'id'], 'create-proxy-branch'),
  );

  const fixture = {
    apiVersion,
    storeDomain,
    capturedAt: new Date().toISOString(),
    scenarioId: 'metaobject-upsert-recovery-and-prefixes',
    notes:
      'Fresh HAR-678 capture for metaobjectUpsert exact-match no-op and handle/value userError prefix partitioning. Parity replay uses the captured public definition/upsert setup mutations instead of synthesized hydration cassettes.',
    definitionCreate: captureFromResult(
      'definition-create',
      definitionCreateMutation,
      definitionVariables,
      definitionCreate,
    ),
    cases: {
      primaryCreate,
      exactMatch,
      conflictOwnerCreate,
      conflictingHandle,
      missingRequired,
      proxyCreate,
    },
    proxyExpected: {
      conflictingHandle: withUserErrorField(conflictingHandle, ['handle', 'handle']),
      missingRequired: withUserErrorField(missingRequired, []),
    },
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
} finally {
  for (const id of cleanupMetaobjectIds) {
    try {
      await client.runGraphqlRequest(metaobjectDeleteMutation, { id });
    } catch (error) {
      console.warn(`Failed to cleanup metaobject ${id}:`, error);
    }
  }
  if (definitionId) {
    try {
      await client.runGraphqlRequest(definitionDeleteMutation, { id: definitionId });
    } catch (error) {
      console.warn(`Failed to cleanup metaobject definition ${definitionId}:`, error);
    }
  }
}
