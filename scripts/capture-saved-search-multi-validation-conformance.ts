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

type UserErrorExpectation = {
  field: string[];
  message: string;
};

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

function readPayloadAlias(payload: ConformanceGraphqlPayload, alias: string): Record<string, unknown> {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.[alias]);
  if (!mutationPayload) {
    throw new Error(`Expected ${alias} payload: ${JSON.stringify(payload, null, 2)}`);
  }

  return mutationPayload;
}

function readSavedSearch(payload: ConformanceGraphqlPayload, alias: string): Record<string, unknown> {
  const mutationPayload = readPayloadAlias(payload, alias);
  const savedSearch = readObject(mutationPayload['savedSearch']);
  if (!savedSearch) {
    throw new Error(`Expected ${alias} savedSearch: ${JSON.stringify(mutationPayload, null, 2)}`);
  }

  return savedSearch;
}

function readSavedSearchId(payload: ConformanceGraphqlPayload, alias: string): string {
  const savedSearch = readSavedSearch(payload, alias);
  const id = savedSearch['id'];
  if (typeof id !== 'string') {
    throw new Error(`Expected ${alias} savedSearch id: ${JSON.stringify(savedSearch, null, 2)}`);
  }

  return id;
}

function assertUserErrors(
  payload: ConformanceGraphqlPayload,
  alias: string,
  expected: UserErrorExpectation[],
  options: { expectNullSavedSearch: boolean },
): void {
  const mutationPayload = readPayloadAlias(payload, alias);
  if (options.expectNullSavedSearch && mutationPayload['savedSearch'] !== null) {
    throw new Error(`Expected ${alias} savedSearch null: ${JSON.stringify(mutationPayload, null, 2)}`);
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== expected.length) {
    throw new Error(`Expected ${alias} ${expected.length} userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }

  for (const [index, expectation] of expected.entries()) {
    const actual = readObject(userErrors[index]);
    if (
      JSON.stringify(actual?.['field']) !== JSON.stringify(expectation.field) ||
      actual?.['message'] !== expectation.message
    ) {
      throw new Error(
        `Unexpected ${alias} userError[${index}]: expected ${JSON.stringify(
          expectation,
        )}, got ${JSON.stringify(actual, null, 2)}`,
      );
    }
  }
}

function assertCreateSucceeded(payload: ConformanceGraphqlPayload, alias: string): void {
  readSavedSearchId(payload, alias);
  const mutationPayload = readPayloadAlias(payload, alias);
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected ${alias} to have no userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertUpdatePayloadEcho(
  payload: ConformanceGraphqlPayload,
  expected: { name: string; query: string; resourceType: string },
): void {
  const mutationPayload = readPayloadAlias(payload, 'savedSearchUpdate');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  if (
    !savedSearch ||
    savedSearch['name'] !== expected.name ||
    savedSearch['query'] !== expected.query ||
    savedSearch['resourceType'] !== expected.resourceType
  ) {
    throw new Error(`Unexpected update savedSearch echo: ${JSON.stringify(mutationPayload, null, 2)}`);
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

const createDocument = await readRequest('saved-search-multi-validation-create.graphql');
const updateDocument = await readRequest('saved-search-multi-validation-update.graphql');
const deleteDocument = await readRequest('saved-search-query-grammar-delete.graphql');
const token = `multi-${Date.now().toString(36)}`;
const productPositiveAName = `SDP Multi A ${token}`.slice(0, 40);
const productPositiveBName = `SDP Multi B ${token}`.slice(0, 40);
const createVariables = {
  productReservedUnknown: {
    resourceType: 'PRODUCT',
    name: 'All products',
    query: 'made_up_filter:foo',
  },
  productLongIncompatible: {
    resourceType: 'PRODUCT',
    name: '12345678901234567890123456789012345678901',
    query: 'collection_id:"123" tag:"AAA"',
  },
  orderReservedUnknown: {
    resourceType: 'ORDER',
    name: `SDP Order ${token}`.slice(0, 40),
    query: 'reference_location_id:1 made_up_filter:foo',
  },
  productPositiveA: {
    resourceType: 'PRODUCT',
    name: productPositiveAName,
    query: `vendor:${token} tag:a`,
  },
  productPositiveB: {
    resourceType: 'PRODUCT',
    name: productPositiveBName,
    query: `vendor:${token} tag:b`,
  },
};

const savedSearchCreateMultiValidation = await client.runGraphqlRequest(createDocument, createVariables);
assertNoTopLevelErrors(savedSearchCreateMultiValidation, 'saved-search multi-validation create capture');
assertUserErrors(
  savedSearchCreateMultiValidation.payload,
  'productReservedUnknown',
  [
    { field: ['input', 'name'], message: 'Name has already been taken' },
    { field: ['input', 'query'], message: "Query is invalid, 'made_up_filter' is not a valid filter" },
  ],
  { expectNullSavedSearch: true },
);
assertUserErrors(
  savedSearchCreateMultiValidation.payload,
  'productLongIncompatible',
  [
    { field: ['input', 'name'], message: 'Name is too long (maximum is 40 characters)' },
    { field: ['input', 'query'], message: 'Query has incompatible filters: collection_id, tag' },
  ],
  { expectNullSavedSearch: true },
);
assertUserErrors(
  savedSearchCreateMultiValidation.payload,
  'orderReservedUnknown',
  [
    {
      field: ['input', 'query'],
      message: "Search terms is invalid, 'reference_location_id' is a reserved filter name",
    },
    { field: ['input', 'query'], message: "Query is invalid, 'made_up_filter' is not a valid filter" },
  ],
  { expectNullSavedSearch: true },
);
assertCreateSucceeded(savedSearchCreateMultiValidation.payload, 'productPositiveA');
assertCreateSucceeded(savedSearchCreateMultiValidation.payload, 'productPositiveB');

const productPositiveAId = readSavedSearchId(savedSearchCreateMultiValidation.payload, 'productPositiveA');
const productPositiveBId = readSavedSearchId(savedSearchCreateMultiValidation.payload, 'productPositiveB');
const updateVariables = {
  input: {
    id: productPositiveBId,
    name: productPositiveAName,
    query: 'made_up_filter:foo collection_id:"123" tag:"AAA"',
  },
};

let cleanupA: ConformanceGraphqlResult | null = null;
let cleanupB: ConformanceGraphqlResult | null = null;
try {
  const savedSearchUpdateMultiValidation = await client.runGraphqlRequest(updateDocument, updateVariables);
  assertNoTopLevelErrors(savedSearchUpdateMultiValidation, 'saved-search multi-validation update capture');
  assertUpdatePayloadEcho(savedSearchUpdateMultiValidation.payload, {
    name: productPositiveBName,
    query: updateVariables.input.query,
    resourceType: 'PRODUCT',
  });
  assertUserErrors(
    savedSearchUpdateMultiValidation.payload,
    'savedSearchUpdate',
    [
      { field: ['input', 'name'], message: 'Name has already been taken' },
      { field: ['input', 'query'], message: "Query is invalid, 'made_up_filter' is not a valid filter" },
      { field: ['input', 'query'], message: 'Query has incompatible filters: collection_id, tag' },
    ],
    { expectNullSavedSearch: false },
  );

  cleanupA = await cleanup(productPositiveAId, deleteDocument);
  cleanupB = await cleanup(productPositiveBId, deleteDocument);

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    notes: [
      'SavedSearch create aggregates name and query validation userErrors in one payload.',
      'SavedSearch query validation reports reserved and unknown filters together for ORDER saved searches.',
      'SavedSearch update aggregates duplicate-name, unknown-filter, and incompatible-filter userErrors while returning a non-staged payload echo.',
    ],
    savedSearchCreateMultiValidation: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-multi-validation-create.graphql',
      variables: createVariables,
      payload: savedSearchCreateMultiValidation.payload,
    },
    savedSearchUpdateMultiValidation: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-multi-validation-update.graphql',
      variables: updateVariables,
      payload: savedSearchUpdateMultiValidation.payload,
    },
    cleanupA: cleanupA
      ? {
          documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-delete.graphql',
          variables: { input: { id: productPositiveAId } },
          payload: cleanupA.payload,
        }
      : null,
    cleanupB: cleanupB
      ? {
          documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-delete.graphql',
          variables: { input: { id: productPositiveBId } },
          payload: cleanupB.payload,
        }
      : null,
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-multi-validation.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (!cleanupA) {
    await cleanup(productPositiveAId, deleteDocument);
  }
  if (!cleanupB) {
    await cleanup(productPositiveBId, deleteDocument);
  }
}
