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

function readCreatedSavedSearchId(payload: ConformanceGraphqlPayload): string {
  const mutationPayload = readMutationPayload(payload, 'savedSearchCreate');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  const id = savedSearch?.['id'];
  if (typeof id !== 'string') {
    throw new Error('Expected savedSearchCreate to return a savedSearch id.');
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

function assertReservedNameUserError(
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
    throw new Error(`Expected ${context} reserved-name userError; got ${JSON.stringify(error)}.`);
  }
}

function assertCreateSucceeded(payload: ConformanceGraphqlPayload, context: string): void {
  const mutationPayload = readMutationPayload(payload, 'savedSearchCreate');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  if (!savedSearch || typeof savedSearch['id'] !== 'string') {
    throw new Error(`Expected ${context} to return a savedSearch id; got ${JSON.stringify(mutationPayload)}.`);
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected ${context} to return no userErrors; got ${JSON.stringify(userErrors)}.`);
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

async function cleanupExistingPositiveControl(deleteDocument: string): Promise<void> {
  const document = `query SavedSearchReservedNamePrecleanup {
    productSavedSearches(first: 50) {
      nodes {
        id
        name
      }
    }
  }`;
  const result = await client.runGraphqlRequest(document);
  assertNoTopLevelErrors(result, 'saved-search reserved-name precleanup query');
  const data = readObject(result.payload.data);
  const connection = readObject(data?.['productSavedSearches']);
  const nodes = Array.isArray(connection?.['nodes']) ? connection['nodes'] : [];
  for (const node of nodes) {
    const savedSearch = readObject(node);
    const id = savedSearch?.['id'];
    if (savedSearch?.['name'] === 'All products v2' && typeof id === 'string') {
      await cleanup(id, deleteDocument);
    }
  }
}

const createDocument = await readRequest('saved-search-local-staging-create.graphql');
const updateDocument = await readRequest('saved-search-name-uniqueness-update-conflict.graphql');
const deleteDocument = await readRequest('saved-search-delete-shop-payload-delete.graphql');
const cleanupIds: string[] = [];

const productExactVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: 'All products',
    query: 'vendor:Acme',
  },
};
const productCaseVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: 'ALL PRODUCTS',
    query: 'vendor:Acme',
  },
};
const orderVariables = {
  input: {
    resourceType: 'ORDER',
    name: 'All',
    query: 'status:open',
  },
};
const draftOrderVariables = {
  input: {
    resourceType: 'DRAFT_ORDER',
    name: 'All Drafts',
    query: 'status:open',
  },
};
const fileVariables = {
  input: {
    resourceType: 'FILE',
    name: 'All Files',
    query: '',
  },
};
const collectionVariables = {
  input: {
    resourceType: 'COLLECTION',
    name: 'All collections',
    query: 'title:Sale',
  },
};
const positiveControlVariables = {
  input: {
    resourceType: 'PRODUCT',
    name: 'All products v2',
    query: 'vendor:Acme',
  },
};

let productReservedCreate: ConformanceGraphqlResult | null = null;
let productReservedCreateCase: ConformanceGraphqlResult | null = null;
let orderReservedCreate: ConformanceGraphqlResult | null = null;
let draftOrderReservedCreate: ConformanceGraphqlResult | null = null;
let fileReservedCreate: ConformanceGraphqlResult | null = null;
let collectionReservedCreate: ConformanceGraphqlResult | null = null;
let productPositiveControlCreate: ConformanceGraphqlResult | null = null;
let productReservedUpdate: ConformanceGraphqlResult | null = null;

try {
  await cleanupExistingPositiveControl(deleteDocument);

  productReservedCreate = await client.runGraphqlRequest(createDocument, productExactVariables);
  assertNoTopLevelErrors(productReservedCreate, 'saved-search reserved-name product exact create capture');
  const unexpectedProductExactId = readOptionalSavedSearchId(productReservedCreate.payload, 'savedSearchCreate');
  if (unexpectedProductExactId) cleanupIds.push(unexpectedProductExactId);
  assertReservedNameUserError(productReservedCreate.payload, 'savedSearchCreate', 'product-exact-create', {
    expectNullSavedSearch: true,
  });

  productReservedCreateCase = await client.runGraphqlRequest(createDocument, productCaseVariables);
  assertNoTopLevelErrors(productReservedCreateCase, 'saved-search reserved-name product case create capture');
  const unexpectedProductCaseId = readOptionalSavedSearchId(productReservedCreateCase.payload, 'savedSearchCreate');
  if (unexpectedProductCaseId) cleanupIds.push(unexpectedProductCaseId);
  assertReservedNameUserError(productReservedCreateCase.payload, 'savedSearchCreate', 'product-case-create', {
    expectNullSavedSearch: true,
  });

  orderReservedCreate = await client.runGraphqlRequest(createDocument, orderVariables);
  assertNoTopLevelErrors(orderReservedCreate, 'saved-search reserved-name order create capture');
  const unexpectedOrderId = readOptionalSavedSearchId(orderReservedCreate.payload, 'savedSearchCreate');
  if (unexpectedOrderId) cleanupIds.push(unexpectedOrderId);
  assertReservedNameUserError(orderReservedCreate.payload, 'savedSearchCreate', 'order-create', {
    expectNullSavedSearch: true,
  });

  draftOrderReservedCreate = await client.runGraphqlRequest(createDocument, draftOrderVariables);
  assertNoTopLevelErrors(draftOrderReservedCreate, 'saved-search reserved-name draft-order create capture');
  const unexpectedDraftOrderId = readOptionalSavedSearchId(draftOrderReservedCreate.payload, 'savedSearchCreate');
  if (unexpectedDraftOrderId) cleanupIds.push(unexpectedDraftOrderId);
  assertReservedNameUserError(draftOrderReservedCreate.payload, 'savedSearchCreate', 'draft-order-create', {
    expectNullSavedSearch: true,
  });

  fileReservedCreate = await client.runGraphqlRequest(createDocument, fileVariables);
  assertNoTopLevelErrors(fileReservedCreate, 'saved-search reserved-name file create capture');
  const unexpectedFileId = readOptionalSavedSearchId(fileReservedCreate.payload, 'savedSearchCreate');
  if (unexpectedFileId) cleanupIds.push(unexpectedFileId);
  assertReservedNameUserError(fileReservedCreate.payload, 'savedSearchCreate', 'file-create', {
    expectNullSavedSearch: true,
  });

  collectionReservedCreate = await client.runGraphqlRequest(createDocument, collectionVariables);
  assertNoTopLevelErrors(collectionReservedCreate, 'saved-search reserved-name collection create capture');
  const unexpectedCollectionId = readOptionalSavedSearchId(collectionReservedCreate.payload, 'savedSearchCreate');
  if (unexpectedCollectionId) cleanupIds.push(unexpectedCollectionId);
  assertReservedNameUserError(collectionReservedCreate.payload, 'savedSearchCreate', 'collection-create', {
    expectNullSavedSearch: true,
  });

  productPositiveControlCreate = await client.runGraphqlRequest(createDocument, positiveControlVariables);
  assertNoTopLevelErrors(productPositiveControlCreate, 'saved-search reserved-name positive-control create capture');
  assertCreateSucceeded(productPositiveControlCreate.payload, 'positive-control create');
  const positiveControlId = readCreatedSavedSearchId(productPositiveControlCreate.payload);
  cleanupIds.push(positiveControlId);

  const productReservedUpdateVariables = {
    input: {
      id: positiveControlId,
      name: 'All products',
      query: 'vendor:Changed',
    },
  };
  productReservedUpdate = await client.runGraphqlRequest(updateDocument, productReservedUpdateVariables);
  assertNoTopLevelErrors(productReservedUpdate, 'saved-search reserved-name product update capture');
  const unexpectedUpdateId = readOptionalSavedSearchId(productReservedUpdate.payload, 'savedSearchUpdate');
  if (unexpectedUpdateId && unexpectedUpdateId !== positiveControlId) cleanupIds.push(unexpectedUpdateId);
  assertReservedNameUserError(productReservedUpdate.payload, 'savedSearchUpdate', 'product-update', {
    expectNullSavedSearch: false,
  });

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes: [
      'Live Shopify evidence for SavedSearch per-resource reserved-name validation.',
      'PRODUCT, ORDER, DRAFT_ORDER, FILE, and COLLECTION reserved names are rejected on create with userErrors[{ field: ["input", "name"], message: "Name has already been taken" }].',
      'PRODUCT reserved-name matching is case-insensitive: "ALL PRODUCTS" is rejected the same way as "All products".',
      'PRODUCT "All products v2" is a positive control proving reserved-name prefixes are not rejected unless they exactly match case-insensitively.',
      'A PRODUCT saved search renamed to "All products" is rejected on update with a non-null savedSearch payload and is deleted during cleanup.',
      'CUSTOMER reserved-name create behavior is deferred to the customer saved-search deprecation flow.',
      'The proxy parity runner stages the positive-control setup create locally; no upstream cassette calls are required.',
    ],
    productReservedCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: productExactVariables,
      payload: productReservedCreate.payload,
    },
    productReservedCreateCase: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: productCaseVariables,
      payload: productReservedCreateCase.payload,
    },
    orderReservedCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: orderVariables,
      payload: orderReservedCreate.payload,
    },
    draftOrderReservedCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: draftOrderVariables,
      payload: draftOrderReservedCreate.payload,
    },
    fileReservedCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: fileVariables,
      payload: fileReservedCreate.payload,
    },
    collectionReservedCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: collectionVariables,
      payload: collectionReservedCreate.payload,
    },
    productPositiveControlCreate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-local-staging-create.graphql',
      variables: positiveControlVariables,
      payload: productPositiveControlCreate.payload,
    },
    productReservedUpdate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-name-uniqueness-update-conflict.graphql',
      variables: productReservedUpdateVariables,
      payload: productReservedUpdate.payload,
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  const fixturePath = path.join(outputDir, 'saved-search-reserved-name.json');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
} finally {
  for (const id of cleanupIds.reverse()) {
    await cleanup(id, deleteDocument);
  }
}
