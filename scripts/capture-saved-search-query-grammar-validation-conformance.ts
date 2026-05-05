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

function readCreatedSavedSearchId(payload: ConformanceGraphqlPayload): string {
  const mutationPayload = readMutationPayload(payload, 'productPositive');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  const id = savedSearch?.['id'];
  if (typeof id !== 'string') {
    throw new Error('Expected productPositive savedSearchCreate to return a savedSearch id.');
  }

  return id;
}

function assertUserError(
  payload: ConformanceGraphqlPayload,
  root: string,
  field: string[],
  message: string,
  context: string,
): void {
  const mutationPayload = readMutationPayload(payload, root);
  if (mutationPayload['savedSearch'] !== null) {
    throw new Error(`${context} expected savedSearch null: ${JSON.stringify(mutationPayload, null, 2)}`);
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`${context} expected userErrors: ${JSON.stringify(mutationPayload, null, 2)}`);
  }
  const first = readObject(userErrors[0]);
  if (JSON.stringify(first?.['field']) !== JSON.stringify(field) || first?.['message'] !== message) {
    throw new Error(`${context} unexpected userError: ${JSON.stringify(first, null, 2)}`);
  }
}

function assertPositiveCreate(payload: ConformanceGraphqlPayload): void {
  const mutationPayload = readMutationPayload(payload, 'productPositive');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  if (savedSearch?.['query'] !== 'collection_id:"12345"' || savedSearch['resourceType'] !== 'PRODUCT') {
    throw new Error(`Expected collection_id-only positive create: ${JSON.stringify(mutationPayload, null, 2)}`);
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected positive create to have no userErrors: ${JSON.stringify(mutationPayload, null, 2)}`);
  }
}

function assertUpdateValidation(payload: ConformanceGraphqlPayload): void {
  const mutationPayload = readMutationPayload(payload, 'savedSearchUpdate');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  if (
    savedSearch?.['query'] !== 'collection_id:"123" tag:"AAA"' ||
    savedSearch['name'] === undefined ||
    savedSearch['resourceType'] !== 'PRODUCT'
  ) {
    throw new Error(`Expected update payload echo with invalid query: ${JSON.stringify(mutationPayload, null, 2)}`);
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`Expected update validation userErrors: ${JSON.stringify(mutationPayload, null, 2)}`);
  }
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

const createDocument = await readRequest('saved-search-query-grammar-validation-create.graphql');
const updateDocument = await readRequest('saved-search-query-grammar-validation-update.graphql');
const deleteDocument = await readRequest('saved-search-query-grammar-delete.graphql');
const token = `H729-${Date.now().toString(36)}`;
const createVariables = {
  orderReserved: {
    resourceType: 'ORDER',
    name: `H729 Reserved ${token}`.slice(0, 40),
    query: 'reference_location_id:1',
  },
  productCollectionTag: {
    resourceType: 'PRODUCT',
    name: `H729 Collection Tag ${token}`.slice(0, 40),
    query: 'collection_id:"123" tag:"AAA"',
  },
  productCollectionPublished: {
    resourceType: 'PRODUCT',
    name: `H729 Published ${token}`.slice(0, 40),
    query: 'collection_id:"123" published_status:published',
  },
  productCollectionErrorFeedback: {
    resourceType: 'PRODUCT',
    name: `H729 Error ${token}`.slice(0, 40),
    query: 'collection_id:"123" error_feedback:"x"',
  },
  productPositive: {
    resourceType: 'PRODUCT',
    name: `H729 Positive ${token}`.slice(0, 40),
    query: 'collection_id:"12345"',
  },
};

const savedSearchCreateValidation = await client.runGraphqlRequest(createDocument, createVariables);
assertNoTopLevelErrors(savedSearchCreateValidation, 'saved-search query grammar validation create capture');
assertUserError(
  savedSearchCreateValidation.payload,
  'orderReserved',
  ['input', 'query'],
  "Search terms is invalid, 'reference_location_id' is a reserved filter name",
  'order reserved-filter create',
);
assertUserError(
  savedSearchCreateValidation.payload,
  'productCollectionTag',
  ['input', 'query'],
  'Query has incompatible filters: collection_id, tag',
  'product collection_id+tag create',
);
assertUserError(
  savedSearchCreateValidation.payload,
  'productCollectionPublished',
  ['input', 'query'],
  'Query has incompatible filters: collection_id, published_status',
  'product collection_id+published_status create',
);
assertUserError(
  savedSearchCreateValidation.payload,
  'productCollectionErrorFeedback',
  ['input', 'query'],
  'Query has incompatible filters: collection_id, error_feedback',
  'product collection_id+error_feedback create',
);
assertPositiveCreate(savedSearchCreateValidation.payload);
const createdId = readCreatedSavedSearchId(savedSearchCreateValidation.payload);

let cleanupComplete = false;
try {
  const updateVariables = {
    input: {
      id: createdId,
      query: 'collection_id:"123" tag:"AAA"',
    },
  };
  const savedSearchUpdateCollectionTag = await client.runGraphqlRequest(updateDocument, updateVariables);
  assertNoTopLevelErrors(savedSearchUpdateCollectionTag, 'saved-search query grammar validation update capture');
  assertUpdateValidation(savedSearchUpdateCollectionTag.payload);

  const deleteVariables = { input: { id: createdId } };
  const cleanupDelete = await client.runGraphqlRequest(deleteDocument, deleteVariables);
  assertNoTopLevelErrors(cleanupDelete, 'saved-search query grammar validation cleanup');
  assertDeleteSucceeded(cleanupDelete.payload, createdId);
  cleanupComplete = true;

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    notes: [
      'HAR-729 capture for saved-search query grammar validation.',
      'ORDER savedSearchCreate rejects reference_location_id as a reserved filter with field ["input", "query"].',
      'PRODUCT savedSearchCreate rejects collection_id combined with tag, error_feedback, or published_status.',
      'PRODUCT savedSearchCreate accepts collection_id alone and cleanup deletes that disposable saved search.',
      'PRODUCT savedSearchUpdate with collection_id+tag returns a non-null payload echo plus userErrors and is not expected to persist locally.',
    ],
    savedSearchCreateValidation: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-validation-create.graphql',
      variables: createVariables,
      payload: savedSearchCreateValidation.payload,
    },
    savedSearchUpdateCollectionTag: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-validation-update.graphql',
      variables: updateVariables,
      payload: savedSearchUpdateCollectionTag.payload,
    },
    cleanupDelete: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-delete.graphql',
      variables: deleteVariables,
      payload: cleanupDelete.payload,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-query-grammar-validation.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (!cleanupComplete) {
    await cleanup(createdId);
  }
}
