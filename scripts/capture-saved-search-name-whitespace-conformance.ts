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

type MutationRoot = 'savedSearchCreate' | 'savedSearchUpdate' | 'savedSearchDelete';

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

function readMutationPayload(payload: ConformanceGraphqlPayload, root: MutationRoot): Record<string, unknown> {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.[root]);
  if (!mutationPayload) {
    throw new Error(`Expected ${root} payload.`);
  }

  return mutationPayload;
}

function readSavedSearchId(
  payload: ConformanceGraphqlPayload,
  root: 'savedSearchCreate' | 'savedSearchUpdate',
): string {
  const mutationPayload = readMutationPayload(payload, root);
  const savedSearch = readObject(mutationPayload['savedSearch']);
  const id = savedSearch?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`Expected ${root} to return a savedSearch id.`);
  }

  return id;
}

function readOptionalSavedSearchId(
  payload: ConformanceGraphqlPayload,
  root: 'savedSearchCreate' | 'savedSearchUpdate',
): string | null {
  const mutationPayload = readMutationPayload(payload, root);
  const savedSearch = readObject(mutationPayload['savedSearch']);
  return typeof savedSearch?.['id'] === 'string' ? savedSearch['id'] : null;
}

function assertMutationSucceeded(
  payload: ConformanceGraphqlPayload,
  root: 'savedSearchCreate' | 'savedSearchUpdate',
  expectedName: string,
  context: string,
): void {
  const mutationPayload = readMutationPayload(payload, root);
  const savedSearch = readObject(mutationPayload['savedSearch']);
  if (!savedSearch || typeof savedSearch['id'] !== 'string' || savedSearch['name'] !== expectedName) {
    throw new Error(
      `Expected ${context} to return savedSearch.name ${JSON.stringify(expectedName)}; got ${JSON.stringify(mutationPayload)}.`,
    );
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected ${context} to return no userErrors; got ${JSON.stringify(userErrors)}.`);
  }
}

function assertNameTakenUserError(
  payload: ConformanceGraphqlPayload,
  root: 'savedSearchCreate' | 'savedSearchUpdate',
  context: string,
  options: { expectNullSavedSearch: boolean },
): void {
  const mutationPayload = readMutationPayload(payload, root);
  if (options.expectNullSavedSearch && mutationPayload['savedSearch'] !== null) {
    throw new Error(
      `Expected ${context} savedSearch to be null; got ${JSON.stringify(mutationPayload['savedSearch'])}.`,
    );
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`Expected ${context} to return exactly one userError; got ${JSON.stringify(userErrors)}.`);
  }
  const error = readObject(userErrors[0]);
  const field = error?.['field'];
  if (
    !Array.isArray(field) ||
    field.length !== 2 ||
    field[0] !== 'input' ||
    field[1] !== 'name' ||
    error?.['message'] !== 'Name has already been taken'
  ) {
    throw new Error(`Expected ${context} name-taken userError; got ${JSON.stringify(error)}.`);
  }
}

function assertLatestProductSavedSearchName(
  payload: ConformanceGraphqlPayload,
  expectedName: string,
  context: string,
): void {
  const data = readObject(payload.data);
  const connection = readObject(data?.['productSavedSearches']);
  const nodes = Array.isArray(connection?.['nodes']) ? connection['nodes'] : [];
  const first = readObject(nodes[0]);
  if (first?.['name'] !== expectedName) {
    throw new Error(
      `Expected ${context} latest product saved search ${JSON.stringify(expectedName)}; got ${JSON.stringify(nodes)}.`,
    );
  }
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

async function cleanup(id: string, deleteDocument: string): Promise<void> {
  try {
    await client.runGraphqlRequest(deleteDocument, { input: { id } });
  } catch (error) {
    console.error(`Failed to cleanup saved search ${id}:`, error);
  }
}

async function cleanupExistingTestRecords(deleteDocument: string): Promise<void> {
  const document = `query SavedSearchNameWhitespacePrecleanup {
    productSavedSearches(first: 100) {
      nodes {
        id
        name
      }
    }
  }`;
  const result = await client.runGraphqlRequest(document);
  assertNoTopLevelErrors(result, 'saved-search name-whitespace precleanup query');
  const data = readObject(result.payload.data);
  const connection = readObject(data?.['productSavedSearches']);
  const nodes = Array.isArray(connection?.['nodes']) ? connection['nodes'] : [];
  const disposableNames = new Set(['Weekend', ' Weekend', ' All products', 'Update target', 'Weekend ']);
  for (const node of nodes) {
    const savedSearch = readObject(node);
    const id = savedSearch?.['id'];
    const name = savedSearch?.['name'];
    if (typeof id === 'string' && typeof name === 'string' && disposableNames.has(name)) {
      await cleanup(id, deleteDocument);
    }
  }
}

const createDocument = await readRequest('saved-search-local-staging-create.graphql');
const updateDocument = await readRequest('saved-search-name-uniqueness-update-conflict.graphql');
const readLatestDocument = await readRequest('saved-search-query-grammar-read-after-create.graphql');
const deleteDocument = await readRequest('saved-search-delete-shop-payload-delete.graphql');
const cleanupIds: string[] = [];

const createWeekendVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: 'Weekend',
    query: 'vendor:Acme',
  },
};
const createLeadingWeekendVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: ' Weekend',
    query: 'vendor:Acme',
  },
};
const createLeadingReservedVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: ' All products',
    query: '*',
  },
};
const createExactDuplicateVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: 'Weekend',
    query: 'vendor:Duplicate',
  },
};
const createExactReservedVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: 'All products',
    query: '*',
  },
};
const createUpdateTargetVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: 'Update target',
    query: 'vendor:Seed',
  },
};

let savedSearchCreateWeekend: ConformanceGraphqlResult | null = null;
let savedSearchCreateLeadingWeekend: ConformanceGraphqlResult | null = null;
let productSavedSearchesAfterLeadingWeekend: ConformanceGraphqlResult | null = null;
let savedSearchCreateLeadingReserved: ConformanceGraphqlResult | null = null;
let productSavedSearchesAfterLeadingReserved: ConformanceGraphqlResult | null = null;
let savedSearchCreateExactDuplicate: ConformanceGraphqlResult | null = null;
let savedSearchCreateExactReserved: ConformanceGraphqlResult | null = null;
let savedSearchCreateUpdateTarget: ConformanceGraphqlResult | null = null;
let savedSearchUpdateTrailingWeekend: ConformanceGraphqlResult | null = null;
let productSavedSearchesAfterUpdate: ConformanceGraphqlResult | null = null;

try {
  await cleanupExistingTestRecords(deleteDocument);

  savedSearchCreateWeekend = await client.runGraphqlRequest(createDocument, createWeekendVariables);
  assertNoTopLevelErrors(savedSearchCreateWeekend, 'saved-search name-whitespace Weekend create capture');
  assertMutationSucceeded(savedSearchCreateWeekend.payload, 'savedSearchCreate', 'Weekend', 'Weekend create');
  cleanupIds.push(readSavedSearchId(savedSearchCreateWeekend.payload, 'savedSearchCreate'));

  savedSearchCreateLeadingWeekend = await client.runGraphqlRequest(createDocument, createLeadingWeekendVariables);
  assertNoTopLevelErrors(
    savedSearchCreateLeadingWeekend,
    'saved-search name-whitespace leading Weekend create capture',
  );
  assertMutationSucceeded(
    savedSearchCreateLeadingWeekend.payload,
    'savedSearchCreate',
    ' Weekend',
    'leading Weekend create',
  );
  cleanupIds.push(readSavedSearchId(savedSearchCreateLeadingWeekend.payload, 'savedSearchCreate'));

  productSavedSearchesAfterLeadingWeekend = await client.runGraphqlRequest(readLatestDocument);
  assertNoTopLevelErrors(
    productSavedSearchesAfterLeadingWeekend,
    'saved-search name-whitespace leading Weekend read capture',
  );
  assertLatestProductSavedSearchName(
    productSavedSearchesAfterLeadingWeekend.payload,
    ' Weekend',
    'leading Weekend read',
  );

  savedSearchCreateLeadingReserved = await client.runGraphqlRequest(createDocument, createLeadingReservedVariables);
  assertNoTopLevelErrors(
    savedSearchCreateLeadingReserved,
    'saved-search name-whitespace leading reserved create capture',
  );
  assertMutationSucceeded(
    savedSearchCreateLeadingReserved.payload,
    'savedSearchCreate',
    ' All products',
    'leading reserved create',
  );
  cleanupIds.push(readSavedSearchId(savedSearchCreateLeadingReserved.payload, 'savedSearchCreate'));

  productSavedSearchesAfterLeadingReserved = await client.runGraphqlRequest(readLatestDocument);
  assertNoTopLevelErrors(
    productSavedSearchesAfterLeadingReserved,
    'saved-search name-whitespace leading reserved read capture',
  );
  assertLatestProductSavedSearchName(
    productSavedSearchesAfterLeadingReserved.payload,
    ' All products',
    'leading reserved read',
  );

  savedSearchCreateExactDuplicate = await client.runGraphqlRequest(createDocument, createExactDuplicateVariables);
  assertNoTopLevelErrors(
    savedSearchCreateExactDuplicate,
    'saved-search name-whitespace exact duplicate create capture',
  );
  const unexpectedDuplicateId = readOptionalSavedSearchId(savedSearchCreateExactDuplicate.payload, 'savedSearchCreate');
  if (unexpectedDuplicateId) cleanupIds.push(unexpectedDuplicateId);
  assertNameTakenUserError(savedSearchCreateExactDuplicate.payload, 'savedSearchCreate', 'exact duplicate create', {
    expectNullSavedSearch: true,
  });

  savedSearchCreateExactReserved = await client.runGraphqlRequest(createDocument, createExactReservedVariables);
  assertNoTopLevelErrors(savedSearchCreateExactReserved, 'saved-search name-whitespace exact reserved create capture');
  const unexpectedReservedId = readOptionalSavedSearchId(savedSearchCreateExactReserved.payload, 'savedSearchCreate');
  if (unexpectedReservedId) cleanupIds.push(unexpectedReservedId);
  assertNameTakenUserError(savedSearchCreateExactReserved.payload, 'savedSearchCreate', 'exact reserved create', {
    expectNullSavedSearch: true,
  });

  savedSearchCreateUpdateTarget = await client.runGraphqlRequest(createDocument, createUpdateTargetVariables);
  assertNoTopLevelErrors(savedSearchCreateUpdateTarget, 'saved-search name-whitespace update target create capture');
  assertMutationSucceeded(
    savedSearchCreateUpdateTarget.payload,
    'savedSearchCreate',
    'Update target',
    'update target create',
  );
  const updateTargetId = readSavedSearchId(savedSearchCreateUpdateTarget.payload, 'savedSearchCreate');
  cleanupIds.push(updateTargetId);

  const updateTrailingWeekendVariables = {
    input: {
      id: updateTargetId,
      name: 'Weekend ',
      query: 'vendor:Renamed',
    },
  };
  savedSearchUpdateTrailingWeekend = await client.runGraphqlRequest(updateDocument, updateTrailingWeekendVariables);
  assertNoTopLevelErrors(
    savedSearchUpdateTrailingWeekend,
    'saved-search name-whitespace trailing Weekend update capture',
  );
  assertMutationSucceeded(
    savedSearchUpdateTrailingWeekend.payload,
    'savedSearchUpdate',
    'Weekend ',
    'trailing Weekend update',
  );

  productSavedSearchesAfterUpdate = await client.runGraphqlRequest(readLatestDocument);
  assertNoTopLevelErrors(productSavedSearchesAfterUpdate, 'saved-search name-whitespace update read capture');
  assertLatestProductSavedSearchName(productSavedSearchesAfterUpdate.payload, 'Weekend ', 'trailing Weekend read');

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes: [
      'Live Shopify evidence that SavedSearch name uniqueness compares the raw name string, so leading/trailing whitespace makes an otherwise duplicate PRODUCT name distinct.',
      'Live Shopify evidence that reserved-name matching is case-insensitive but does not strip surrounding whitespace, so " All products" is accepted while exact "All products" is rejected.',
      'The scenario creates disposable PRODUCT saved searches through Admin GraphQL, reads the latest productSavedSearches connection after whitespace-distinct create/update branches, and cleans up created records.',
      'The proxy parity runner replays setup and assertion requests through the public GraphQL surface; no upstream cassette calls are required.',
    ],
    savedSearchCreateWeekend: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createWeekendVariables,
      payload: savedSearchCreateWeekend.payload,
    },
    savedSearchCreateLeadingWeekend: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createLeadingWeekendVariables,
      payload: savedSearchCreateLeadingWeekend.payload,
    },
    productSavedSearchesAfterLeadingWeekend: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-read-after-create.graphql',
      variables: {},
      payload: productSavedSearchesAfterLeadingWeekend.payload,
    },
    savedSearchCreateLeadingReserved: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createLeadingReservedVariables,
      payload: savedSearchCreateLeadingReserved.payload,
    },
    productSavedSearchesAfterLeadingReserved: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-read-after-create.graphql',
      variables: {},
      payload: productSavedSearchesAfterLeadingReserved.payload,
    },
    savedSearchCreateExactDuplicate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createExactDuplicateVariables,
      payload: savedSearchCreateExactDuplicate.payload,
    },
    savedSearchCreateExactReserved: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createExactReservedVariables,
      payload: savedSearchCreateExactReserved.payload,
    },
    savedSearchCreateUpdateTarget: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: createUpdateTargetVariables,
      payload: savedSearchCreateUpdateTarget.payload,
    },
    savedSearchUpdateTrailingWeekend: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-name-uniqueness-update-conflict.graphql',
      variables: updateTrailingWeekendVariables,
      payload: savedSearchUpdateTrailingWeekend.payload,
    },
    productSavedSearchesAfterUpdate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-query-grammar-read-after-create.graphql',
      variables: {},
      payload: productSavedSearchesAfterUpdate.payload,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-name-whitespace.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  for (const id of cleanupIds.reverse()) {
    await cleanup(id, deleteDocument);
  }
}
