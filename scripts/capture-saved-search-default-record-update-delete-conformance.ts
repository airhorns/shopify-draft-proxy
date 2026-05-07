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

const orderDefault = {
  id: 'gid://shopify/SavedSearch/3634391515442',
  name: 'Unfulfilled',
  query: 'status:open fulfillment_status:unshipped,partial',
  resourceType: 'ORDER',
};

const draftOrderDefault = {
  id: 'gid://shopify/SavedSearch/3634390597938',
  name: 'Open and invoice sent',
  query: 'status:open_and_invoice_sent',
  resourceType: 'DRAFT_ORDER',
};

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

function readNodes(payload: ConformanceGraphqlPayload, root: string): Array<Record<string, unknown>> {
  const data = readObject(payload.data);
  const connection = readObject(data?.[root]);
  const nodes = connection?.['nodes'];
  if (!Array.isArray(nodes)) {
    throw new Error(`Expected ${root}.nodes in payload: ${JSON.stringify(payload, null, 2)}`);
  }

  return nodes.map((node) => {
    const object = readObject(node);
    if (!object) {
      throw new Error(`Expected ${root}.nodes to contain objects: ${JSON.stringify(nodes, null, 2)}`);
    }

    return object;
  });
}

function requireDefaultNode(
  payload: ConformanceGraphqlPayload,
  root: string,
  expected: typeof orderDefault,
): Record<string, unknown> {
  const found = readNodes(payload, root).find((node) => node['id'] === expected.id);
  if (!found) {
    throw new Error(`Expected seeded default ${expected.id} in ${root}; capture would not target a default record.`);
  }
  if (
    found['name'] !== expected.name ||
    found['query'] !== expected.query ||
    found['resourceType'] !== expected.resourceType
  ) {
    throw new Error(`Expected seeded default shape for ${expected.id}; got ${JSON.stringify(found, null, 2)}`);
  }

  return found;
}

function readMutationPayload(payload: ConformanceGraphqlPayload, root: string): Record<string, unknown> {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.[root]);
  if (!mutationPayload) {
    throw new Error(`Expected ${root} payload: ${JSON.stringify(payload, null, 2)}`);
  }

  return mutationPayload;
}

function assertUpdateSucceeded(payload: ConformanceGraphqlPayload, expectedId: string, expectedName: string): void {
  const mutationPayload = readMutationPayload(payload, 'savedSearchUpdate');
  const savedSearch = readObject(mutationPayload['savedSearch']);
  if (!savedSearch || savedSearch['id'] !== expectedId || savedSearch['name'] !== expectedName) {
    throw new Error(
      `Expected savedSearchUpdate to echo ${expectedId}; got ${JSON.stringify(mutationPayload, null, 2)}`,
    );
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected savedSearchUpdate to have no userErrors; got ${JSON.stringify(userErrors)}`);
  }
}

function assertDeleteSucceeded(payload: ConformanceGraphqlPayload, expectedId: string): void {
  const mutationPayload = readMutationPayload(payload, 'savedSearchDelete');
  if (mutationPayload['deletedSavedSearchId'] !== expectedId) {
    throw new Error(`Expected savedSearchDelete to delete ${expectedId}; got ${JSON.stringify(mutationPayload)}`);
  }
  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`Expected savedSearchDelete to have no userErrors; got ${JSON.stringify(userErrors)}`);
  }
}

function assertReadIncludesUpdatedOrder(
  payload: ConformanceGraphqlPayload,
  expectedId: string,
  expectedName: string,
): void {
  const nodes = readNodes(payload, 'orderSavedSearches');
  if (!nodes.some((node) => node['id'] === expectedId && node['name'] === expectedName)) {
    throw new Error(`Expected orderSavedSearches to include updated default; got ${JSON.stringify(nodes, null, 2)}`);
  }
}

function assertReadSuppressesDeletedDraft(payload: ConformanceGraphqlPayload, deletedId: string): void {
  const nodes = readNodes(payload, 'draftOrderSavedSearches');
  if (nodes.some((node) => node['id'] === deletedId || node['name'] === draftOrderDefault.name)) {
    throw new Error(
      `Expected draftOrderSavedSearches to suppress deleted default; got ${JSON.stringify(nodes, null, 2)}`,
    );
  }
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

async function cleanupUpdate(document: string): Promise<ConformanceGraphqlResult> {
  return await client.runGraphqlRequest(document, {
    input: {
      id: orderDefault.id,
      name: orderDefault.name,
      query: orderDefault.query,
    },
  });
}

async function cleanupCreateDraft(): Promise<ConformanceGraphqlResult> {
  const document = `#graphql
    mutation SavedSearchDefaultRecordCleanupDraft($input: SavedSearchCreateInput!) {
      savedSearchCreate(input: $input) {
        savedSearch {
          id
          name
          query
          resourceType
        }
        userErrors {
          field
          message
        }
      }
    }
  `;
  return await client.runGraphqlRequest(document, {
    input: {
      resourceType: draftOrderDefault.resourceType,
      name: draftOrderDefault.name,
      query: draftOrderDefault.query,
    },
  });
}

const readBeforeDocument = await readRequest('saved-search-default-records-read.graphql');
const updateOrderDocument = await readRequest('saved-search-default-record-update-order.graphql');
const readUpdatedOrderDocument = await readRequest('saved-search-default-record-read-updated-order.graphql');
const deleteDraftOrderDocument = await readRequest('saved-search-default-record-delete-draft-order.graphql');
const readDeletedDraftOrderDocument = await readRequest('saved-search-default-record-read-deleted-draft-order.graphql');

const suffix = Date.now().toString(36).slice(-6);
const renamedOrderName = `Renamed default ${suffix}`;
const updateOrderVariables = {
  input: {
    id: orderDefault.id,
    name: renamedOrderName,
    query: 'status:closed',
  },
};
const readUpdatedOrderVariables = {};
const deleteDraftOrderVariables = { input: { id: draftOrderDefault.id } };
const readDeletedDraftOrderVariables = {};

let updateStarted = false;
let deleteStarted = false;
let cleanupOrderRestore: ConformanceGraphqlResult | null = null;
let cleanupDraftRecreate: ConformanceGraphqlResult | null = null;
let fixture: Record<string, unknown> | null = null;

try {
  const readBefore = await client.runGraphqlRequest(readBeforeDocument, {});
  assertNoTopLevelErrors(readBefore, 'saved-search default read-before capture');
  requireDefaultNode(readBefore.payload, 'orderSavedSearches', orderDefault);
  requireDefaultNode(readBefore.payload, 'draftOrderSavedSearches', draftOrderDefault);

  updateStarted = true;
  const updateOrderDefault = await client.runGraphqlRequest(updateOrderDocument, updateOrderVariables);
  assertNoTopLevelErrors(updateOrderDefault, 'saved-search default update capture');
  assertUpdateSucceeded(updateOrderDefault.payload, orderDefault.id, renamedOrderName);

  const readOrderAfterUpdate = await client.runGraphqlRequest(readUpdatedOrderDocument, readUpdatedOrderVariables);
  assertNoTopLevelErrors(readOrderAfterUpdate, 'saved-search default read-after-update capture');
  assertReadIncludesUpdatedOrder(readOrderAfterUpdate.payload, orderDefault.id, renamedOrderName);

  deleteStarted = true;
  const deleteDraftOrderDefault = await client.runGraphqlRequest(deleteDraftOrderDocument, deleteDraftOrderVariables);
  assertNoTopLevelErrors(deleteDraftOrderDefault, 'saved-search default delete capture');
  assertDeleteSucceeded(deleteDraftOrderDefault.payload, draftOrderDefault.id);

  const readDraftAfterDelete = await client.runGraphqlRequest(
    readDeletedDraftOrderDocument,
    readDeletedDraftOrderVariables,
  );
  assertNoTopLevelErrors(readDraftAfterDelete, 'saved-search default read-after-delete capture');
  assertReadSuppressesDeletedDraft(readDraftAfterDelete.payload, draftOrderDefault.id);

  fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes: [
      'Live Shopify evidence that persisted ORDER and DRAFT_ORDER default saved-search records can be updated and deleted through savedSearchUpdate/savedSearchDelete.',
      'The capture verifies an update echoed the ORDER default record, a downstream orderSavedSearches read reflected the new name/query, deleting the DRAFT_ORDER default succeeded, and a downstream draftOrderSavedSearches read no longer returned the deleted default.',
      'The cleanup path restores the ORDER default fields and recreates the deleted DRAFT_ORDER saved search by name/query/resourceType so the disposable conformance shop keeps a semantically equivalent default view, though Shopify assigns a new id to recreated records.',
    ],
    readBefore: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-default-records-read.graphql',
      variables: {},
      response: readBefore.payload,
    },
    updateOrderDefault: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-default-record-update-order.graphql',
      variables: updateOrderVariables,
      response: updateOrderDefault.payload,
    },
    readOrderAfterUpdate: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-default-record-read-updated-order.graphql',
      variables: readUpdatedOrderVariables,
      response: readOrderAfterUpdate.payload,
    },
    deleteDraftOrderDefault: {
      documentPath: 'config/parity-requests/saved-searches/saved-search-default-record-delete-draft-order.graphql',
      variables: deleteDraftOrderVariables,
      response: deleteDraftOrderDefault.payload,
    },
    readDraftAfterDelete: {
      documentPath:
        'config/parity-requests/saved-searches/saved-search-default-record-read-deleted-draft-order.graphql',
      variables: readDeletedDraftOrderVariables,
      response: readDraftAfterDelete.payload,
    },
    upstreamCalls: [],
  };
} finally {
  if (updateStarted) {
    cleanupOrderRestore = await cleanupUpdate(updateOrderDocument);
    assertNoTopLevelErrors(cleanupOrderRestore, 'saved-search default order cleanup update');
    assertUpdateSucceeded(cleanupOrderRestore.payload, orderDefault.id, orderDefault.name);
  }
  if (deleteStarted) {
    cleanupDraftRecreate = await cleanupCreateDraft();
    assertNoTopLevelErrors(cleanupDraftRecreate, 'saved-search default draft-order cleanup create');
  }
}

if (!fixture) {
  throw new Error('Saved-search default update/delete capture did not complete.');
}

fixture['cleanup'] = {
  restoreOrderDefault: cleanupOrderRestore?.payload ?? null,
  recreateDraftOrderDefault: cleanupDraftRecreate?.payload ?? null,
};

await mkdir(outputDir, { recursive: true });
const fixturePath = path.join(outputDir, 'saved-search-default-record-update-delete.json');
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
