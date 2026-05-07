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

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readSavedSearch(payload: ConformanceGraphqlPayload, root: 'savedSearchCreate' | 'savedSearchUpdate') {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.[root]);
  const savedSearch = readObject(mutationPayload?.['savedSearch']);
  if (savedSearch === null) {
    throw new Error(`Expected ${root} to return a savedSearch: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = mutationPayload?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected ${root} to have no userErrors: ${JSON.stringify(payload, null, 2)}`);
  }

  return savedSearch;
}

function readCreatedSavedSearchId(payload: ConformanceGraphqlPayload): string {
  const id = readSavedSearch(payload, 'savedSearchCreate')['id'];
  if (typeof id !== 'string') {
    throw new Error('Expected savedSearchCreate to return a savedSearch id.');
  }

  return id;
}

function assertDeleteSucceeded(payload: ConformanceGraphqlPayload, id: string): void {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.['savedSearchDelete']);
  if (mutationPayload?.['deletedSavedSearchId'] !== id) {
    throw new Error(`Expected savedSearchDelete to delete ${id}.`);
  }
  const userErrors = mutationPayload?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error('Expected savedSearchDelete to have no userErrors.');
  }
}

function assertResolvedAppQuery(savedSearch: Record<string, unknown>, key: string): string {
  const query = savedSearch['query'];
  if (typeof query !== 'string') {
    throw new Error(`Expected savedSearch.query to be a string: ${JSON.stringify(savedSearch, null, 2)}`);
  }
  const match = query.match(/^metafields\.app--([^.]+)\./u);
  const hasExpectedKeyValue = query.endsWith(`.${key}:gold`) || query.endsWith(`.${key}:true`);
  if (!match?.[1] || !hasExpectedKeyValue) {
    throw new Error(`Expected savedSearch.query to contain a resolved $app namespace: ${query}`);
  }

  return match[1];
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

async function cleanup(id: string): Promise<void> {
  const document = await readRequest('saved-search-query-grammar-delete.graphql');
  try {
    await client.runGraphqlRequest(document, { input: { id } });
  } catch (error) {
    console.error(`Failed to cleanup saved search ${id}:`, error);
  }
}

const createDocument = await readRequest('saved-search-local-staging-create.graphql');
const readDocument = await readRequest('saved-search-app-namespace-read.graphql');
const updateDocument = await readRequest('saved-search-app-namespace-update.graphql');
const deleteDocument = await readRequest('saved-search-query-grammar-delete.graphql');
const token = `APPNS-${Date.now().toString(36)}`;
const createVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: `App namespace ${token}`.slice(0, 40),
    query: 'metafields.$app.tier:gold',
  },
};

const create = await client.runGraphqlRequest(createDocument, createVariables);
assertNoTopLevelErrors(create, 'saved-search app namespace create capture');
const createdId = readCreatedSavedSearchId(create.payload);
const requestingApiClientId = assertResolvedAppQuery(readSavedSearch(create.payload, 'savedSearchCreate'), 'tier');

let cleanupComplete = false;
try {
  const readVariables = {};
  const readAfterCreate = await client.runGraphqlRequest(readDocument, readVariables);
  assertNoTopLevelErrors(readAfterCreate, 'saved-search app namespace read-after-create capture');

  const updateVariables = {
    input: {
      id: createdId,
      query: 'metafields.$app.vip:true',
    },
  };
  const update = await client.runGraphqlRequest(updateDocument, updateVariables);
  assertNoTopLevelErrors(update, 'saved-search app namespace update capture');
  assertResolvedAppQuery(readSavedSearch(update.payload, 'savedSearchUpdate'), 'vip');

  const readAfterUpdate = await client.runGraphqlRequest(readDocument, readVariables);
  assertNoTopLevelErrors(readAfterUpdate, 'saved-search app namespace read-after-update capture');

  const deleteVariables = { input: { id: createdId } };
  const cleanupDelete = await client.runGraphqlRequest(deleteDocument, deleteVariables);
  assertNoTopLevelErrors(cleanupDelete, 'saved-search app namespace cleanup delete capture');
  assertDeleteSucceeded(cleanupDelete.payload, createdId);
  cleanupComplete = true;

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    requestingApiClientId,
    notes: [
      'Live Shopify evidence for SavedSearch $app metafield namespace resolution in query input.',
      'savedSearchCreate resolved metafields.$app.tier before constructing and parsing the SavedSearch record.',
      'savedSearchUpdate resolved metafields.$app.vip on an existing record before returning the mutation payload.',
      'Downstream productSavedSearches exposed the resolved namespace in query, searchTerms, and filters.',
      'The fixture includes cleanup delete evidence for the created saved search.',
    ],
    savedSearchCreateAppNamespace: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createVariables,
      payload: create.payload,
    },
    productSavedSearchesAfterCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-app-namespace-read.graphql',
      variables: readVariables,
      payload: readAfterCreate.payload,
    },
    savedSearchUpdateAppNamespace: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-app-namespace-update.graphql',
      variables: updateVariables,
      payload: update.payload,
    },
    productSavedSearchesAfterUpdate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-app-namespace-read.graphql',
      variables: readVariables,
      payload: readAfterUpdate.payload,
    },
    cleanupDelete: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-delete.graphql',
      variables: deleteVariables,
      payload: cleanupDelete.payload,
    },
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-app-namespace.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (!cleanupComplete) {
    await cleanup(createdId);
  }
}
