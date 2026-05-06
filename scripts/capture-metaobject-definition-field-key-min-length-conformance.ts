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
const outputPath = path.join(outputDir, 'metaobjectDefinition-field-key-min-length.json');
const runId = Date.now().toString();

const requestPaths = {
  create: 'config/parity-requests/metaobjects/metaobjectDefinition-field-key-min-length-create.graphql',
  update: 'config/parity-requests/metaobjects/metaobjectDefinition-field-key-min-length-update.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const deleteDefinitionMutation = `#graphql
  mutation DeleteMetaobjectDefinition($id: ID!) {
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
    const object = readObject(current);
    if (object === null) {
      return undefined;
    }
    current = object[part];
  }

  return current;
}

function readUserErrors(payload: unknown, pathParts: string[]): unknown[] {
  const value = readPath(payload, pathParts);
  return Array.isArray(value) ? value : [];
}

function readDefinitionId(payload: unknown, pathParts: string[], label: string): string | null {
  const value = readPath(payload, pathParts);
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} returned a non-string definition id: ${JSON.stringify(payload, null, 2)}`);
  }
  return value;
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertHasUserErrorCode(payload: unknown, pathParts: string[], code: string, label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (!userErrors.some((error) => readPath(error, ['code']) === code)) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
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
    request: {
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
  };
}

function field(key: string, name = key || 'Empty Key'): Record<string, unknown> {
  return {
    key,
    name,
    type: 'single_line_text_field',
  };
}

function createDefinitionInput(
  type: string,
  name: string,
  displayNameKey: string,
  fieldDefinitions: Array<Record<string, unknown>>,
): Record<string, unknown> {
  return {
    type,
    name,
    displayNameKey,
    fieldDefinitions,
  };
}

function createFieldDefinition(key: string, name = key || 'Empty Key'): Record<string, unknown> {
  return {
    fieldDefinitions: [
      {
        create: {
          key,
          name,
          type: 'single_line_text_field',
        },
      },
    ],
  };
}

function deleteFieldDefinition(key: string): Record<string, unknown> {
  return {
    fieldDefinitions: [
      {
        delete: {
          key,
        },
      },
    ],
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

async function cleanupDefinition(id: string | null, cleanup: Capture[]): Promise<void> {
  if (id === null) {
    return;
  }
  cleanup.push(await captureGraphql(`cleanup-metaobject-definition-delete-${cleanup.length + 1}`, deleteDefinitionMutation, { id }));
}

const cleanup: Capture[] = [];
let setupDefinitionId: string | null = null;
let createBoundaryDefinitionId: string | null = null;

try {
  const setup = await captureGraphql('setup-valid-definition', queries.create, {
    definition: createDefinitionInput(`field_key_min_setup_${runId}`, 'Field Key Min Setup', 'title', [field('title', 'Title')]),
  });
  assertNoUserErrors(setup.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], 'setup-valid-definition');
  setupDefinitionId = readDefinitionId(
    setup.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'setup-valid-definition',
  );

  const createKeyA = await captureGraphql('create-key-a', queries.create, {
    definition: createDefinitionInput(`field_key_min_a_${runId}`, 'Field Key Min A', 'a', [field('a', 'A')]),
  });
  assertHasUserErrorCode(createKeyA.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], 'TOO_SHORT', 'create-key-a');

  const createKeyEmpty = await captureGraphql('create-key-empty', queries.create, {
    definition: createDefinitionInput(`field_key_min_empty_${runId}`, 'Field Key Min Empty', '', [field('', 'Empty Key')]),
  });
  assertHasUserErrorCode(
    createKeyEmpty.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'TOO_SHORT',
    'create-key-empty',
  );

  const createKeyAb = await captureGraphql('create-key-ab', queries.create, {
    definition: createDefinitionInput(`field_key_min_ab_${runId}`, 'Field Key Min AB', 'ab', [field('ab', 'AB')]),
  });
  assertNoUserErrors(createKeyAb.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], 'create-key-ab');
  createBoundaryDefinitionId = readDefinitionId(
    createKeyAb.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'create-key-ab',
  );

  const updateCreateKeyA = await captureGraphql('update-create-key-a', queries.update, {
    id: setupDefinitionId,
    definition: createFieldDefinition('a', 'A'),
  });
  assertHasUserErrorCode(
    updateCreateKeyA.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'TOO_SHORT',
    'update-create-key-a',
  );

  const updateCreateKeyEmpty = await captureGraphql('update-create-key-empty', queries.update, {
    id: setupDefinitionId,
    definition: createFieldDefinition('', 'Empty Key'),
  });
  assertHasUserErrorCode(
    updateCreateKeyEmpty.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'TOO_SHORT',
    'update-create-key-empty',
  );

  const updateCreateKeyAb = await captureGraphql('update-create-key-ab', queries.update, {
    id: setupDefinitionId,
    definition: createFieldDefinition('ab', 'AB'),
  });
  assertNoUserErrors(updateCreateKeyAb.response, ['data', 'metaobjectDefinitionUpdate', 'userErrors'], 'update-create-key-ab');

  const updateDeleteKeyEmpty = await captureGraphql('update-delete-key-empty', queries.update, {
    id: setupDefinitionId,
    definition: deleteFieldDefinition(''),
  });
  assertHasUserErrorCode(
    updateDeleteKeyEmpty.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'UNDEFINED_OBJECT_FIELD',
    'update-delete-key-empty',
  );

  await cleanupDefinition(createBoundaryDefinitionId, cleanup);
  await cleanupDefinition(setupDefinitionId, cleanup);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'MetaobjectDefinition field definition key minimum-length validation for create and update.create, including empty strings and the accepted two-character boundary.',
        setup,
        createKeyA,
        createKeyEmpty,
        createKeyAb,
        updateCreateKeyA,
        updateCreateKeyEmpty,
        updateCreateKeyAb,
        updateDeleteKeyEmpty,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await cleanupDefinition(createBoundaryDefinitionId, cleanup);
    await cleanupDefinition(setupDefinitionId, cleanup);
  } finally {
    console.error(error instanceof Error ? error.message : String(error));
  }
  throw error;
}
