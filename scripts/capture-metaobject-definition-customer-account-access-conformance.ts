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
    documentPath?: string;
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: ConformanceGraphqlResult['payload'];
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-definition-customer-account-access.json');
const runId = Date.now().toString();

const requestPaths = {
  create: 'config/parity-requests/metaobjects/metaobject-definition-customer-account-access-create.graphql',
  update: 'config/parity-requests/metaobjects/metaobject-definition-customer-account-access-update.graphql',
  read: 'config/parity-requests/metaobjects/metaobject-definition-customer-account-access-read.graphql',
  invalidCreate:
    'config/parity-requests/metaobjects/metaobject-definition-customer-account-access-invalid-create.graphql',
  invalidUpdate:
    'config/parity-requests/metaobjects/metaobject-definition-customer-account-access-invalid-update.graphql',
};

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

function requireDefinitionId(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a definition id: ${JSON.stringify(payload, null, 2)}`);
  }

  return value;
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertGraphqlErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || !Array.isArray(result.payload.errors)) {
    throw new Error(`${label} did not return top-level GraphQL errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function definitionInput(type: string, name: string, customerAccount: 'NONE' | 'READ'): Record<string, unknown> {
  return {
    type,
    name,
    access: {
      customerAccount,
    },
    displayNameKey: 'title',
    fieldDefinitions: [
      {
        key: 'title',
        name: 'Title',
        type: 'single_line_text_field',
        required: true,
      },
    ],
  };
}

function updateAccessInput(customerAccount: 'NONE' | 'READ'): Record<string, unknown> {
  return {
    access: {
      customerAccount,
    },
  };
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
  documentPath?: string,
): Capture {
  const request = documentPath
    ? {
        documentPath,
        query,
        variables,
      }
    : {
        query,
        variables,
      };

  return {
    name,
    request,
    status: result.status,
    response: result.payload,
  };
}

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  documentPath?: string,
): Promise<Capture> {
  const result = await runGraphqlRequest(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result, documentPath);
}

async function captureGraphqlError(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  documentPath?: string,
): Promise<Capture> {
  const result = await runGraphqlRequest(query, variables);
  assertGraphqlErrors(result, name);
  return captureFromResult(name, query, variables, result, documentPath);
}

const types = {
  read: `customer_account_read_${runId}`,
  none: `customer_account_none_${runId}`,
  invalidCreate: `customer_account_invalid_create_${runId}`,
};
const cleanup: Capture[] = [];
const createdDefinitionIds: string[] = [];

try {
  const createRead = await captureGraphql(
    'create-read-access',
    queries.create,
    {
      definition: definitionInput(types.read, `Customer Account Read ${runId}`, 'READ'),
    },
    requestPaths.create,
  );
  assertNoUserErrors(createRead.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], 'create-read-access');
  const readDefinitionId = requireDefinitionId(
    createRead.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'create-read-access',
  );
  createdDefinitionIds.push(readDefinitionId);

  const readAfterCreate = await captureGraphql(
    'read-after-create-read-access',
    queries.read,
    { type: types.read },
    requestPaths.read,
  );

  const updateNone = await captureGraphql(
    'update-none-access',
    queries.update,
    {
      id: readDefinitionId,
      definition: updateAccessInput('NONE'),
    },
    requestPaths.update,
  );
  assertNoUserErrors(updateNone.response, ['data', 'metaobjectDefinitionUpdate', 'userErrors'], 'update-none-access');

  const readAfterUpdateNone = await captureGraphql(
    'read-after-update-none-access',
    queries.read,
    { type: types.read },
    requestPaths.read,
  );

  const updateRead = await captureGraphql(
    'update-read-access',
    queries.update,
    {
      id: readDefinitionId,
      definition: updateAccessInput('READ'),
    },
    requestPaths.update,
  );
  assertNoUserErrors(updateRead.response, ['data', 'metaobjectDefinitionUpdate', 'userErrors'], 'update-read-access');

  const readAfterUpdateRead = await captureGraphql(
    'read-after-update-read-access',
    queries.read,
    { type: types.read },
    requestPaths.read,
  );

  const createNone = await captureGraphql(
    'create-none-access',
    queries.create,
    {
      definition: definitionInput(types.none, `Customer Account None ${runId}`, 'NONE'),
    },
    requestPaths.create,
  );
  assertNoUserErrors(createNone.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], 'create-none-access');
  createdDefinitionIds.push(
    requireDefinitionId(
      createNone.response,
      ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
      'create-none-access',
    ),
  );

  const invalidCreate = await captureGraphqlError(
    'invalid-create-access',
    queries.invalidCreate,
    { type: types.invalidCreate },
    requestPaths.invalidCreate,
  );

  const invalidUpdate = await captureGraphqlError(
    'invalid-update-access',
    queries.invalidUpdate,
    { id: readDefinitionId },
    requestPaths.invalidUpdate,
  );

  for (const id of createdDefinitionIds) {
    cleanup.push(await captureGraphql('cleanup-metaobject-definition-delete', deleteDefinitionMutation, { id }));
  }

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'Metaobject definition customerAccount access capture for READ/NONE create and update, invalid enum coercion, and read-after-write projection.',
        seed: {
          runId,
          types,
          createdDefinitionIds,
        },
        cases: {
          createRead,
          readAfterCreate,
          updateNone,
          readAfterUpdateNone,
          updateRead,
          readAfterUpdateRead,
          createNone,
          invalidCreate,
          invalidUpdate,
        },
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );

  console.log(`Wrote ${outputPath}`);
} catch (error) {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobject-definition-customer-account-access-blocker-${runId}.json`);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        blocker: error instanceof Error ? error.message : String(error),
        seed: {
          runId,
          types,
          createdDefinitionIds,
        },
        cleanup,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
  throw error;
}
