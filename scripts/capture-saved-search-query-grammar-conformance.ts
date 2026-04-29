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

function readCreatedSavedSearchId(payload: ConformanceGraphqlPayload): string {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.['savedSearchCreate']);
  const savedSearch = readObject(mutationPayload?.['savedSearch']);
  const id = savedSearch?.['id'];
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
const readDocument = await readRequest('saved-search-query-grammar-read-after-create.graphql');
const deleteDocument = await readRequest('saved-search-query-grammar-delete.graphql');
const token = `H458-${Date.now().toString(36)}`;
const createVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: `H458 Grammar ${token}`.slice(0, 40),
    query: `title:'${token} Alpha' OR (status:ACTIVE tag:'${token}-tag') -vendor:Archived`,
  },
};

const create = await client.runGraphqlRequest(createDocument, createVariables);
assertNoTopLevelErrors(create, 'saved-search query grammar create capture');
const createdId = readCreatedSavedSearchId(create.payload);

let cleanupComplete = false;
try {
  const readVariables = {};
  const readAfterCreate = await client.runGraphqlRequest(readDocument, readVariables);
  assertNoTopLevelErrors(readAfterCreate, 'saved-search query grammar read-after-create capture');

  const deleteVariables = { input: { id: createdId } };
  const cleanupDelete = await client.runGraphqlRequest(deleteDocument, deleteVariables);
  assertNoTopLevelErrors(cleanupDelete, 'saved-search query grammar cleanup delete capture');
  assertDeleteSucceeded(cleanupDelete.payload, createdId);
  cleanupComplete = true;

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    notes: [
      'HAR-458 capture for saved-search query grammar normalization.',
      'savedSearchCreate preserved the submitted single-quoted grouped/OR query in the mutation payload.',
      'SavedSearch searchTerms normalized quoted field values to double quotes and kept the grouped/OR expression as searchTerms.',
      'The top-level negated field term -vendor:Archived was extracted as filters[{ key: "vendor_not", value: "Archived" }].',
      'Downstream productSavedSearches normalized the stored query to double-quoted grouped terms plus the negated vendor filter.',
      'The fixture includes cleanup delete evidence for the created saved search.',
    ],
    savedSearchCreateProductQueryGrammar: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createVariables,
      payload: create.payload,
    },
    productSavedSearchesAfterCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-read-after-create.graphql',
      variables: readVariables,
      payload: readAfterCreate.payload,
    },
    cleanupDelete: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-delete.graphql',
      variables: deleteVariables,
      payload: cleanupDelete.payload,
    },
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-query-grammar.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (!cleanupComplete) {
    await cleanup(createdId);
  }
}
