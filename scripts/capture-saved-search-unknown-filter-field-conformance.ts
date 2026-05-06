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

function readMutationPayload(payload: ConformanceGraphqlPayload, root: string): Record<string, unknown> {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.[root]);
  if (!mutationPayload) {
    throw new Error(`Expected ${root} payload: ${JSON.stringify(payload, null, 2)}`);
  }

  return mutationPayload;
}

function assertUnknownFilterUserError(payload: ConformanceGraphqlPayload): void {
  const mutationPayload = readMutationPayload(payload, 'unknownProduct');
  if (mutationPayload['savedSearch'] !== null) {
    throw new Error(`Expected unknown filter create to return savedSearch null: ${JSON.stringify(mutationPayload)}`);
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`Expected one unknown filter userError: ${JSON.stringify(mutationPayload)}`);
  }
  const first = readObject(userErrors[0]);
  if (
    JSON.stringify(first?.['field']) !== JSON.stringify(['input', 'query']) ||
    first?.['message'] !== "Query is invalid, 'made_up_filter' is not a valid filter"
  ) {
    throw new Error(`Unexpected unknown filter userError: ${JSON.stringify(first)}`);
  }
}

function assertPositiveCreate(payload: ConformanceGraphqlPayload): string {
  const mutationPayload = readMutationPayload(payload, 'productPositive');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  if (savedSearch?.['query'] !== 'vendor:Acme' || savedSearch['resourceType'] !== 'PRODUCT') {
    throw new Error(`Expected vendor positive create: ${JSON.stringify(mutationPayload)}`);
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected positive create to have no userErrors: ${JSON.stringify(mutationPayload)}`);
  }
  const id = savedSearch['id'];
  if (typeof id !== 'string') {
    throw new Error(`Expected positive create id: ${JSON.stringify(mutationPayload)}`);
  }

  return id;
}

function assertDeleteSucceeded(payload: ConformanceGraphqlPayload, id: string): void {
  const mutationPayload = readMutationPayload(payload, 'savedSearchDelete');
  if (mutationPayload['deletedSavedSearchId'] !== id) {
    throw new Error(`Expected savedSearchDelete to delete ${id}: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected savedSearchDelete to have no userErrors: ${JSON.stringify(payload, null, 2)}`);
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

const createDocument = await readRequest('saved-search-unknown-filter-field.graphql');
const deleteDocument = await readRequest('saved-search-query-grammar-delete.graphql');
const token = `SSF-${Date.now().toString(36)}`;
const createVariables = {
  unknownProduct: {
    resourceType: 'PRODUCT',
    name: `Unknown Filter ${token}`.slice(0, 40),
    query: 'made_up_filter:foo',
  },
  productPositive: {
    resourceType: 'PRODUCT',
    name: `Known Vendor ${token}`.slice(0, 40),
    query: 'vendor:Acme',
  },
};

const savedSearchUnknownFilterField = await client.runGraphqlRequest(createDocument, createVariables);
assertNoTopLevelErrors(savedSearchUnknownFilterField, 'saved-search unknown filter create capture');
assertUnknownFilterUserError(savedSearchUnknownFilterField.payload);
const createdId = assertPositiveCreate(savedSearchUnknownFilterField.payload);

let cleanupComplete = false;
try {
  const deleteVariables = { input: { id: createdId } };
  const cleanupDelete = await client.runGraphqlRequest(deleteDocument, deleteVariables);
  assertNoTopLevelErrors(cleanupDelete, 'saved-search unknown filter cleanup');
  assertDeleteSucceeded(cleanupDelete.payload, createdId);
  cleanupComplete = true;

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    notes: [
      'SavedSearch PRODUCT create rejects made_up_filter as an invalid filter with field ["input", "query"].',
      'SavedSearch PRODUCT create accepts vendor as a valid filter and returns an empty userErrors list.',
      'The positive-control saved search is deleted during cleanup.',
    ],
    savedSearchUnknownFilterField: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-unknown-filter-field.graphql',
      variables: createVariables,
      payload: savedSearchUnknownFilterField.payload,
    },
    cleanupDelete: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-delete.graphql',
      variables: deleteVariables,
      payload: cleanupDelete.payload,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-unknown-filter-field.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (!cleanupComplete) {
    await cleanup(createdId);
  }
}
