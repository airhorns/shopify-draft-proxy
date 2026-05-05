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

function assertTopLevelCoercionError(result: ConformanceGraphqlResult, rootName: string, context: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed with HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  if (!Array.isArray(result.payload.errors) || result.payload.errors.length === 0) {
    throw new Error(`${context} expected top-level GraphQL errors: ${JSON.stringify(result.payload, null, 2)}`);
  }

  const data = result.payload.data === undefined ? null : readObject(result.payload.data);
  if (data !== null && data[rootName] !== null) {
    throw new Error(`${context} expected data.${rootName} to be null: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
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

function assertEmptyQueryCreateSucceeded(payload: ConformanceGraphqlPayload): void {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.['savedSearchCreate']);
  const savedSearch = readObject(mutationPayload?.['savedSearch']);
  if (savedSearch?.['query'] !== '') {
    throw new Error(`Expected savedSearchCreate to preserve an empty query: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = mutationPayload?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected savedSearchCreate empty-query branch to have no userErrors: ${JSON.stringify(payload)}`);
  }
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

async function cleanup(id: string): Promise<ConformanceGraphqlResult> {
  const document = await readRequest('saved-search-query-grammar-delete.graphql');
  return await client.runGraphqlRequest(document, { input: { id } });
}

const missingNameDocument = await readRequest('saved-search-required-input-missing-name-create.graphql');
const missingResourceTypeDocument = await readRequest(
  'saved-search-required-input-missing-resource-type-create.graphql',
);
const emptyQueryDocument = await readRequest('saved-search-required-input-empty-query-create.graphql');
const missingUpdateIdDocument = await readRequest('saved-search-required-input-missing-id-update.graphql');

const token = `H718-${Date.now().toString(36)}`;
const emptyQueryVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: `H718 Empty Query ${token}`.slice(0, 40),
    query: '',
  },
};

const savedSearchCreateMissingName = await client.runGraphqlRequest(missingNameDocument);
assertTopLevelCoercionError(savedSearchCreateMissingName, 'savedSearchCreate', 'missing-name create capture');

const savedSearchCreateMissingResourceType = await client.runGraphqlRequest(missingResourceTypeDocument);
assertTopLevelCoercionError(
  savedSearchCreateMissingResourceType,
  'savedSearchCreate',
  'missing-resourceType create capture',
);

const savedSearchCreateEmptyQuery = await client.runGraphqlRequest(emptyQueryDocument, emptyQueryVariables);
assertNoTopLevelErrors(savedSearchCreateEmptyQuery, 'empty-query create capture');
assertEmptyQueryCreateSucceeded(savedSearchCreateEmptyQuery.payload);
const createdId = readCreatedSavedSearchId(savedSearchCreateEmptyQuery.payload);

let cleanupDelete: ConformanceGraphqlResult | null = null;
try {
  const savedSearchUpdateMissingId = await client.runGraphqlRequest(missingUpdateIdDocument);
  assertTopLevelCoercionError(savedSearchUpdateMissingId, 'savedSearchUpdate', 'missing-id update capture');

  cleanupDelete = await cleanup(createdId);
  assertNoTopLevelErrors(cleanupDelete, 'empty-query saved-search cleanup');

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    token,
    notes: [
      'HAR-718 capture for savedSearchCreate/savedSearchUpdate required input coercion.',
      'Missing inline SavedSearchCreateInput required fields return top-level GraphQL errors before a data payload is emitted.',
      'An explicitly empty query string is accepted on savedSearchCreate and the disposable saved search is deleted during cleanup.',
      'Missing inline SavedSearchUpdateInput.id returns a top-level GraphQL error before resolver userErrors.',
    ],
    savedSearchCreateMissingName: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-required-input-missing-name-create.graphql',
      variables: {},
      payload: savedSearchCreateMissingName.payload,
    },
    savedSearchCreateMissingResourceType: {
      documentPath:
        'config/parity-requests/saved-searches/saved-search-required-input-missing-resource-type-create.graphql',
      variables: {},
      payload: savedSearchCreateMissingResourceType.payload,
    },
    savedSearchCreateEmptyQuery: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-required-input-empty-query-create.graphql',
      variables: emptyQueryVariables,
      payload: savedSearchCreateEmptyQuery.payload,
    },
    savedSearchUpdateMissingId: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-required-input-missing-id-update.graphql',
      variables: {},
      payload: savedSearchUpdateMissingId.payload,
    },
    cleanupDelete: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-delete.graphql',
      variables: { input: { id: createdId } },
      payload: cleanupDelete.payload,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-required-input-validation.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  if (cleanupDelete === null) {
    try {
      await cleanup(createdId);
    } catch (error) {
      console.error(`Failed to cleanup saved search ${createdId}:`, error);
    }
  }
}
