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

type HandleCase = {
  name: 'invalid' | 'too-long' | 'blank';
  handle: string;
  expectedCode?: 'INVALID' | 'TOO_LONG' | 'BLANK';
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject_handle_validation.json');
const specPath = path.join('config', 'parity-specs', 'metaobjects', 'metaobject_handle_validation.json');
const runId = Date.now().toString();
const type = `codex_handle_validation_${runId}`;
const validHandle = `valid-${runId}`;
const tooLongHandle = 'x'.repeat(256);

const requestPaths = {
  definitionCreate: 'config/parity-requests/metaobjects/metaobject_handle_validation_definition_create.graphql',
  create: 'config/parity-requests/metaobjects/metaobject_handle_validation_create.graphql',
  update: 'config/parity-requests/metaobjects/metaobject_handle_validation_update.graphql',
  upsert: 'config/parity-requests/metaobjects/metaobject_handle_validation_upsert.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const metaobjectDeleteMutation = `#graphql
  mutation MetaobjectHandleValidationDelete($id: ID!) {
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
  mutation MetaobjectHandleValidationDefinitionDelete($id: ID!) {
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

const cases: HandleCase[] = [
  { name: 'invalid', handle: 'hello world!', expectedCode: 'INVALID' },
  { name: 'too-long', handle: tooLongHandle, expectedCode: 'TOO_LONG' },
  { name: 'blank', handle: '' },
];

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

function assertHandleUserError(
  payload: unknown,
  root: string,
  expectedField: string[],
  expectedCode: string,
  label: string,
): void {
  const userErrors = readUserErrors(payload, root);
  const first = readObject(userErrors[0]);
  const field = first?.['field'];
  const code = first?.['code'];
  const metaobject = readPath(payload, ['data', root, 'metaobject']);
  if (
    userErrors.length !== 1 ||
    !Array.isArray(field) ||
    field.length !== expectedField.length ||
    field.some((part, index) => part !== expectedField[index]) ||
    code !== expectedCode ||
    metaobject !== null
  ) {
    throw new Error(`${label} did not match expected handle error: ${JSON.stringify(payload, null, 2)}`);
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

function createVariables(handle: string): Record<string, unknown> {
  return {
    metaobject: {
      type,
      handle,
      fields: [{ key: 'title', value: `Handle ${handle || 'blank'}` }],
    },
  };
}

function updateVariables(handle: string, id = ''): Record<string, unknown> {
  return {
    id,
    metaobject: {
      handle,
      fields: [{ key: 'title', value: `Handle ${handle || 'blank'}` }],
    },
  };
}

function upsertVariables(handle: string): Record<string, unknown> {
  return {
    handle: { type, handle },
    metaobject: {
      fields: [{ key: 'title', value: `Handle ${handle || 'blank'}` }],
    },
  };
}

function userErrorSelectedPaths(root: 'metaobjectCreate' | 'metaobjectUpdate' | 'metaobjectUpsert'): string[] {
  return [`$.${root}.metaobject`, `$.${root}.userErrors[*].field`, `$.${root}.userErrors[*].code`];
}

function selectedPathsFor(
  root: 'metaobjectCreate' | 'metaobjectUpdate' | 'metaobjectUpsert',
  handleCase: HandleCase,
): string[] {
  if (handleCase.name === 'blank' && root !== 'metaobjectUpdate') {
    return [
      `$.${root}.metaobject.handle`,
      `$.${root}.metaobject.type`,
      `$.${root}.metaobject.displayName`,
      `$.${root}.userErrors`,
    ];
  }

  return userErrorSelectedPaths(root);
}

function buildSpec(): Record<string, unknown> {
  return {
    scenarioId: 'metaobject-handle-validation',
    operationNames: ['metaobjectDefinitionCreate', 'metaobjectCreate', 'metaobjectUpdate', 'metaobjectUpsert'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'validation-semantics'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/metaobject_definitions_test.gleam'],
    proxyRequest: {
      documentPath: requestPaths.definitionCreate,
      variablesCapturePath: '$.definitionCreate.request.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify evidence for explicit metaobject handle validation. The scenario stages a definition and valid row, then compares invalid-character and over-255-character handle errors across metaobjectCreate, metaobjectUpdate, and metaobjectUpsert. It also captures public Admin blank-handle behavior, which treats an empty create handle like omission and generates a handle from display name. Rejected branches compare null metaobject payloads plus userError field/code only; runtime tests cover no-persistence and generated-handle capping.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'definition-create-setup',
          capturePath: '$.definitionCreate.response.data.metaobjectDefinitionCreate.userErrors',
          proxyPath: '$.data.metaobjectDefinitionCreate.userErrors',
        },
        {
          name: 'setup-metaobject-create',
          capturePath: '$.setupMetaobjectCreate.response.data.metaobjectCreate.userErrors',
          proxyPath: '$.data.metaobjectCreate.userErrors',
          proxyRequest: {
            documentPath: requestPaths.create,
            variablesCapturePath: '$.setupMetaobjectCreate.request.variables',
            apiVersion,
          },
        },
        ...cases.flatMap((handleCase) => [
          {
            name: `create-${handleCase.name}`,
            capturePath: `$.invalidCreateCases.${handleCase.name}.response.data`,
            proxyPath: '$.data',
            selectedPaths: selectedPathsFor('metaobjectCreate', handleCase),
            proxyRequest: {
              documentPath: requestPaths.create,
              variablesCapturePath: `$.invalidCreateCases.${handleCase.name}.request.variables`,
              apiVersion,
            },
          },
          {
            name: `update-${handleCase.name}`,
            capturePath: `$.invalidUpdateCases.${handleCase.name}.response.data`,
            proxyPath: '$.data',
            selectedPaths: selectedPathsFor('metaobjectUpdate', handleCase),
            proxyRequest: {
              documentPath: requestPaths.update,
              variables: {
                id: {
                  fromProxyResponse: 'setup-metaobject-create',
                  path: '$.data.metaobjectCreate.metaobject.id',
                },
                metaobject: {
                  fromCapturePath: `$.invalidUpdateCases.${handleCase.name}.request.variables.metaobject`,
                },
              },
              apiVersion,
            },
          },
          {
            name: `upsert-${handleCase.name}`,
            capturePath: `$.invalidUpsertCases.${handleCase.name}.response.data`,
            proxyPath: '$.data',
            selectedPaths: selectedPathsFor('metaobjectUpsert', handleCase),
            proxyRequest: {
              documentPath: requestPaths.upsert,
              variablesCapturePath: `$.invalidUpsertCases.${handleCase.name}.request.variables`,
              apiVersion,
            },
          },
        ]),
      ],
    },
  };
}

const cleanupCaptures: Capture[] = [];
const createdMetaobjectIds: string[] = [];
const definitionIds: string[] = [];
let definitionCreate: Capture | null = null;
let setupMetaobjectCreate: Capture | null = null;
const invalidCreateCases: Record<string, Capture> = {};
const invalidUpdateCases: Record<string, Capture> = {};
const invalidUpsertCases: Record<string, Capture> = {};

try {
  definitionCreate = await captureGraphql('definition-create', queries.definitionCreate, {
    definition: {
      type,
      name: `Handle Validation ${runId}`,
      displayNameKey: 'title',
      fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field', required: true }],
    },
  });
  assertNoUserErrors(definitionCreate.response, 'metaobjectDefinitionCreate', 'definition-create');
  const definitionId = extractString(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition-create',
  );
  definitionIds.push(definitionId);

  setupMetaobjectCreate = await captureGraphql('setup-metaobject-create', queries.create, {
    metaobject: {
      type,
      handle: validHandle,
      fields: [{ key: 'title', value: 'Valid' }],
    },
  });
  assertNoUserErrors(setupMetaobjectCreate.response, 'metaobjectCreate', 'setup-metaobject-create');
  const setupMetaobjectId = extractString(
    setupMetaobjectCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'setup-metaobject-create',
  );
  createdMetaobjectIds.push(setupMetaobjectId);

  for (const handleCase of cases) {
    const createCapture = await captureGraphql(
      `create-${handleCase.name}`,
      queries.create,
      createVariables(handleCase.handle),
    );
    if (handleCase.expectedCode) {
      assertHandleUserError(
        createCapture.response,
        'metaobjectCreate',
        ['metaobject', 'handle'],
        handleCase.expectedCode,
        `create-${handleCase.name}`,
      );
    }
    const createdId = readPath(createCapture.response, ['data', 'metaobjectCreate', 'metaobject', 'id']);
    if (typeof createdId === 'string' && createdId.length > 0) {
      createdMetaobjectIds.push(createdId);
    }
    invalidCreateCases[handleCase.name] = createCapture;

    const updateCapture = await captureGraphql(
      `update-${handleCase.name}`,
      queries.update,
      updateVariables(handleCase.handle, setupMetaobjectId),
    );
    if (handleCase.expectedCode) {
      assertHandleUserError(
        updateCapture.response,
        'metaobjectUpdate',
        ['metaobject', 'handle'],
        handleCase.expectedCode,
        `update-${handleCase.name}`,
      );
    }
    invalidUpdateCases[handleCase.name] = updateCapture;

    const upsertCapture = await captureGraphql(
      `upsert-${handleCase.name}`,
      queries.upsert,
      upsertVariables(handleCase.handle),
    );
    if (handleCase.expectedCode) {
      assertHandleUserError(
        upsertCapture.response,
        'metaobjectUpsert',
        ['handle', 'handle'],
        handleCase.expectedCode,
        `upsert-${handleCase.name}`,
      );
    }
    const upsertedId = readPath(upsertCapture.response, ['data', 'metaobjectUpsert', 'metaobject', 'id']);
    if (typeof upsertedId === 'string' && upsertedId.length > 0) {
      createdMetaobjectIds.push(upsertedId);
    }
    invalidUpsertCases[handleCase.name] = upsertCapture;
  }

  await cleanup(createdMetaobjectIds, definitionIds, cleanupCaptures);
  createdMetaobjectIds.splice(0, createdMetaobjectIds.length);
  definitionIds.splice(0, definitionIds.length);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'Explicit metaobject handle validation for invalid characters, length over 255, and blank strings across create, update, and upsert mutations.',
        seed: {
          runId,
          type,
          validHandle,
          definitionId,
          setupMetaobjectId,
        },
        definitionCreate,
        setupMetaobjectCreate,
        invalidCreateCases,
        invalidUpdateCases,
        invalidUpsertCases,
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
  const blockerPath = path.join(outputDir, `metaobject_handle_validation_blocker_${runId}.json`);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        seed: {
          runId,
          type,
          validHandle,
          definitionIds,
          createdMetaobjectIds,
        },
        blocker: error instanceof Error ? error.message : String(error),
        cleanup: cleanupCaptures,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
  throw error;
}
