/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'saved-searches');

function readObject(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPayloadRoot(payload: ConformanceGraphqlPayload, root: string): Record<string, unknown> {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.[root]);
  if (!mutationPayload) {
    throw new Error(`Expected ${root} payload: ${JSON.stringify(payload, null, 2)}`);
  }

  return mutationPayload;
}

function readSavedSearch(payload: ConformanceGraphqlPayload, root: string): Record<string, unknown> {
  const mutationPayload = readPayloadRoot(payload, root);
  const savedSearch = readObject(mutationPayload['savedSearch']);
  if (!savedSearch) {
    throw new Error(`Expected ${root} savedSearch: ${JSON.stringify(mutationPayload, null, 2)}`);
  }

  return savedSearch;
}

function readSavedSearchId(payload: ConformanceGraphqlPayload, root: string): string {
  const id = readSavedSearch(payload, root)['id'];
  if (typeof id !== 'string') {
    throw new Error(`Expected ${root} savedSearch id: ${JSON.stringify(payload, null, 2)}`);
  }

  return id;
}

function assertCreateSucceeded(payload: ConformanceGraphqlPayload): void {
  const mutationPayload = readPayloadRoot(payload, 'savedSearchCreate');
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected savedSearchCreate no userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertBlankNameUpdateRejected(
  payload: ConformanceGraphqlPayload,
  expectedName: string,
  expectedQuery: string,
): void {
  const mutationPayload = readPayloadRoot(payload, 'savedSearchUpdate');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  if (!savedSearch || savedSearch['name'] !== expectedName || savedSearch['query'] !== expectedQuery) {
    throw new Error(
      `Expected savedSearchUpdate to echo unchanged name/query: ${JSON.stringify(mutationPayload, null, 2)}`,
    );
  }

  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`Expected one savedSearchUpdate userError: ${JSON.stringify(userErrors, null, 2)}`);
  }

  const userError = readObject(userErrors[0]);
  const keys = userError ? Object.keys(userError).sort() : [];
  if (
    JSON.stringify(keys) !== JSON.stringify(['field', 'message']) ||
    JSON.stringify(userError?.['field']) !== JSON.stringify(['input', 'name']) ||
    userError?.['message'] !== "Name can't be blank"
  ) {
    throw new Error(`Unexpected savedSearchUpdate blank-name userError: ${JSON.stringify(userError, null, 2)}`);
  }
}

function assertReadPreservedName(
  payload: ConformanceGraphqlPayload,
  id: string,
  expectedName: string,
  expectedQuery: string,
): void {
  const data = readObject(payload.data);
  const connection = readObject(data?.['productSavedSearches']);
  const nodes = connection?.['nodes'];
  if (!Array.isArray(nodes) || nodes.length === 0) {
    throw new Error(`Expected productSavedSearches nodes: ${JSON.stringify(payload, null, 2)}`);
  }

  const first = readObject(nodes[0]);
  if (
    !first ||
    first['id'] !== id ||
    first['name'] !== expectedName ||
    first['query'] !== expectedQuery ||
    first['resourceType'] !== 'PRODUCT'
  ) {
    throw new Error(`Expected downstream read to preserve saved search: ${JSON.stringify(first, null, 2)}`);
  }
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

async function cleanup(id: string, deleteDocument: string): Promise<ConformanceGraphqlResult | null> {
  try {
    return await client.runGraphqlRequest(deleteDocument, { input: { id } });
  } catch (error) {
    console.error(`Failed to cleanup saved search ${id}:`, error);
    return null;
  }
}

const createDocument = await readRequest('saved-search-blank-name-update-create.graphql');
const updateDocument = await readRequest('saved-search-blank-name-update.graphql');
const readDocument = await readRequest('saved-search-blank-name-update-read.graphql');
const deleteDocument = await readRequest('saved-search-query-grammar-delete.graphql');
const token = `blankupdate${Date.now().toString(36)}`;
const seedName = `SDP Blank Update ${token}`.slice(0, 40);
const seedQuery = `vendor:${token}`;
const createVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: seedName,
    query: seedQuery,
  },
};

const savedSearchCreateSeed = await client.runGraphqlRequest(createDocument, createVariables);
assertNoTopLevelErrors(savedSearchCreateSeed, 'saved-search blank-name update create capture');
assertCreateSucceeded(savedSearchCreateSeed.payload);
const savedSearchId = readSavedSearchId(savedSearchCreateSeed.payload, 'savedSearchCreate');

let cleanupResult: ConformanceGraphqlResult | null = null;
try {
  const updateVariables = { input: { id: savedSearchId, name: '' } };
  const savedSearchUpdateBlankName = await client.runGraphqlRequest(updateDocument, updateVariables);
  assertNoTopLevelErrors(savedSearchUpdateBlankName, 'saved-search blank-name update capture');
  assertBlankNameUpdateRejected(savedSearchUpdateBlankName.payload, seedName, seedQuery);

  const productSavedSearchesAfterRejectedUpdate = await client.runGraphqlRequest(readDocument);
  assertNoTopLevelErrors(
    productSavedSearchesAfterRejectedUpdate,
    'saved-search blank-name update downstream read capture',
  );
  assertReadPreservedName(productSavedSearchesAfterRejectedUpdate.payload, savedSearchId, seedName, seedQuery);

  cleanupResult = await cleanup(savedSearchId, deleteDocument);

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    notes: [
      'SavedSearchUpdate with an explicitly supplied empty string name returns a field/message UserError at input.name.',
      'The failed blank-name update leaves the saved search name unchanged on a downstream productSavedSearches read.',
    ],
    savedSearchCreateSeed: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-blank-name-update-create.graphql',
      variables: createVariables,
      payload: savedSearchCreateSeed.payload,
    },
    savedSearchUpdateBlankName: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-blank-name-update.graphql',
      variables: updateVariables,
      payload: savedSearchUpdateBlankName.payload,
    },
    productSavedSearchesAfterRejectedUpdate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-blank-name-update-read.graphql',
      variables: {},
      payload: productSavedSearchesAfterRejectedUpdate.payload,
    },
    cleanup: cleanupResult
      ? {
          documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-delete.graphql',
          variables: { input: { id: savedSearchId } },
          payload: cleanupResult.payload,
        }
      : null,
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-blank-name-update.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (!cleanupResult) {
    await cleanup(savedSearchId, deleteDocument);
  }
}
