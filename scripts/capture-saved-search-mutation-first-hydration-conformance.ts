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

const updateBaseline = {
  name: 'Mutation hydration update',
  query: 'tag:hydrationupdate',
};
const updatedName = 'Mutation hydration updated';
const deleteBaseline = {
  name: 'Mutation hydration delete',
  query: 'tag:hydrationdelete',
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

function mutationPayload(
  result: ConformanceGraphqlResult,
  root: 'savedSearchCreate' | 'savedSearchUpdate' | 'savedSearchDelete',
  context: string,
): Record<string, unknown> {
  assertNoTopLevelErrors(result, context);
  const payload = readObject(readObject(result.payload.data)?.[root]);
  if (!payload) {
    throw new Error(`${context} returned no ${root} payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors)) {
    throw new Error(`${context} returned non-array userErrors: ${JSON.stringify(payload, null, 2)}`);
  }
  return payload;
}

function successfulSavedSearch(
  result: ConformanceGraphqlResult,
  root: 'savedSearchCreate' | 'savedSearchUpdate',
  context: string,
): Record<string, unknown> {
  const payload = mutationPayload(result, root, context);
  if ((payload['userErrors'] as unknown[]).length !== 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(payload['userErrors'], null, 2)}`);
  }
  const savedSearch = readObject(payload['savedSearch']);
  if (!savedSearch || typeof savedSearch['id'] !== 'string') {
    throw new Error(`${context} returned no saved search: ${JSON.stringify(payload, null, 2)}`);
  }
  return savedSearch;
}

function savedSearchNodes(payload: ConformanceGraphqlPayload): Array<Record<string, unknown>> {
  const connection = readObject(readObject(payload.data)?.['productSavedSearches']);
  const nodes = connection?.['nodes'];
  if (!Array.isArray(nodes)) {
    throw new Error(`Expected productSavedSearches.nodes: ${JSON.stringify(payload, null, 2)}`);
  }
  return nodes.map((node) => {
    const object = readObject(node);
    if (!object) {
      throw new Error(`Expected saved-search node object: ${JSON.stringify(node, null, 2)}`);
    }
    return object;
  });
}

async function readRequest(filename: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', filename), 'utf8');
}

const updateDocument = await readRequest('saved-search-mutation-first-update.graphql');
const deleteDocument = await readRequest('saved-search-mutation-first-delete.graphql');
const nodeReadDocument = await readRequest('saved-search-mutation-first-node-read.graphql');
const listReadDocument = await readRequest('saved-search-mutation-first-list-read.graphql');

const createDocument = `#graphql
  mutation SavedSearchMutationHydrationSetupCreate($input: SavedSearchCreateInput!) {
    savedSearchCreate(input: $input) {
      savedSearch {
        id
        legacyResourceId
        name
        query
        resourceType
        searchTerms
        filters { key value }
      }
      userErrors { field message }
    }
  }
`;
const hydrationDocument =
  'query SavedSearchMutationTargetHydrate($id: ID!) {\n  node(id: $id) {\n    __typename\n    ... on SavedSearch {\n      id\n      legacyResourceId\n      name\n      query\n      resourceType\n      searchTerms\n      filters {\n        key\n        value\n      }\n    }\n  }\n}';
const baselineDocument =
  'query SavedSearchConnectionBaseline($first: Int!, $after: String) {\n  savedSearchBaseline: productSavedSearches(first: $first, after: $after) {\n    edges { cursor node { id name query resourceType } }\n    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }\n  }\n}';

async function ensurePersistentTarget(target: typeof updateBaseline): Promise<Record<string, unknown>> {
  const read = await client.runGraphqlRequest(listReadDocument, {});
  assertNoTopLevelErrors(read, `read persistent ${target.name}`);
  const existing = savedSearchNodes(read.payload).find((node) => node['name'] === target.name);
  if (existing) {
    if (existing['query'] === target.query && existing['resourceType'] === 'PRODUCT') {
      return existing;
    }
    const updated = await client.runGraphqlRequest(updateDocument, {
      input: { id: existing['id'], name: target.name, query: target.query },
    });
    return successfulSavedSearch(updated, 'savedSearchUpdate', `normalize persistent ${target.name}`);
  }

  const created = await client.runGraphqlRequest(createDocument, {
    input: { name: target.name, query: target.query, resourceType: 'PRODUCT' },
  });
  return successfulSavedSearch(created, 'savedSearchCreate', `create persistent ${target.name}`);
}

function captureEntry(
  documentPath: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Record<string, unknown> {
  return { documentPath, variables, response: result.payload };
}

function upstreamCall(
  query: string,
  operationName: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Record<string, unknown> {
  return {
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName,
    variables,
    query,
    response: { status: result.status, body: result.payload },
  };
}

const updateTarget = await ensurePersistentTarget(updateBaseline);
const deleteTarget = await ensurePersistentTarget(deleteBaseline);
const updateTargetId = updateTarget['id'];
const deleteTargetId = deleteTarget['id'];
if (typeof updateTargetId !== 'string' || typeof deleteTargetId !== 'string') {
  throw new Error('Persistent saved-search setup did not return string IDs.');
}
const missingId = `gid://shopify/SavedSearch/${Date.now()}999`;

const updateVariables = { input: { id: updateTargetId, name: updatedName } };
const updateNodeVariables = { id: updateTargetId };
const updateListVariables = {};
const deleteVariables = { input: { id: deleteTargetId } };
const deleteNodeVariables = { id: deleteTargetId };
const deleteListVariables = {};
const missingUpdateVariables = { input: { id: missingId, name: 'Missing mutation target' } };
const missingDeleteVariables = { input: { id: missingId } };

const updateHydrationVariables = { id: updateTargetId };
const updateHydration = await client.runGraphqlRequest(hydrationDocument, updateHydrationVariables);
assertNoTopLevelErrors(updateHydration, 'capture update-target hydration query');
const missingHydrationVariables = { id: missingId };
const missingHydration = await client.runGraphqlRequest(hydrationDocument, missingHydrationVariables);
assertNoTopLevelErrors(missingHydration, 'capture missing-target hydration query');
if (readObject(missingHydration.payload.data)?.['node'] !== null) {
  throw new Error(
    `Generated missing saved-search ID unexpectedly resolved: ${JSON.stringify(missingHydration.payload)}`,
  );
}

let updateStarted = false;
let deleteStarted = false;
let fixture: Record<string, unknown> | null = null;
let restoreUpdate: ConformanceGraphqlResult | null = null;
let recreateDelete: ConformanceGraphqlResult | null = null;

try {
  updateStarted = true;
  const updateExisting = await client.runGraphqlRequest(updateDocument, updateVariables);
  const updated = successfulSavedSearch(updateExisting, 'savedSearchUpdate', 'update existing saved search');
  if (updated['query'] !== updateBaseline.query || updated['name'] !== updatedName) {
    throw new Error(`Existing update did not preserve the authoritative query: ${JSON.stringify(updated, null, 2)}`);
  }

  const nodeAfterUpdate = await client.runGraphqlRequest(nodeReadDocument, updateNodeVariables);
  assertNoTopLevelErrors(nodeAfterUpdate, 'node read after saved-search update');
  const listAfterUpdate = await client.runGraphqlRequest(listReadDocument, updateListVariables);
  assertNoTopLevelErrors(listAfterUpdate, 'list read after saved-search update');

  deleteStarted = true;
  const deleteExisting = await client.runGraphqlRequest(deleteDocument, deleteVariables);
  const deletedPayload = mutationPayload(deleteExisting, 'savedSearchDelete', 'delete existing saved search');
  if (deletedPayload['deletedSavedSearchId'] !== deleteTargetId || (deletedPayload['userErrors'] as unknown[]).length) {
    throw new Error(`Existing delete did not succeed: ${JSON.stringify(deletedPayload, null, 2)}`);
  }

  const nodeAfterDelete = await client.runGraphqlRequest(nodeReadDocument, deleteNodeVariables);
  assertNoTopLevelErrors(nodeAfterDelete, 'node read after saved-search delete');
  const listAfterDelete = await client.runGraphqlRequest(listReadDocument, deleteListVariables);
  assertNoTopLevelErrors(listAfterDelete, 'list read after saved-search delete');

  const updateMissing = await client.runGraphqlRequest(updateDocument, missingUpdateVariables);
  const missingUpdatePayload = mutationPayload(updateMissing, 'savedSearchUpdate', 'update missing saved search');
  if (missingUpdatePayload['savedSearch'] !== null || (missingUpdatePayload['userErrors'] as unknown[]).length === 0) {
    throw new Error(`Missing update did not return a not-found payload: ${JSON.stringify(missingUpdatePayload)}`);
  }

  const deleteMissing = await client.runGraphqlRequest(deleteDocument, missingDeleteVariables);
  const missingDeletePayload = mutationPayload(deleteMissing, 'savedSearchDelete', 'delete missing saved search');
  if (
    missingDeletePayload['deletedSavedSearchId'] !== null ||
    (missingDeletePayload['userErrors'] as unknown[]).length === 0
  ) {
    throw new Error(`Missing delete did not return a not-found payload: ${JSON.stringify(missingDeletePayload)}`);
  }

  fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes: [
      'Live Shopify evidence for mutation-first savedSearchUpdate and savedSearchDelete against arbitrary persisted PRODUCT saved searches plus an ID-specific missing target.',
      'The existing update changes only name and proves Shopify preserves the target query, search terms, filters, resource type, and identity. Node/list reads capture downstream update and delete materialization.',
      'Persistent disposable targets are restored/recreated during cleanup so parity cassette recording can hydrate real targets without runtime Shopify writes.',
    ],
    updateExisting: captureEntry(
      'config/parity-requests/saved-searches/saved-search-mutation-first-update.graphql',
      updateVariables,
      updateExisting,
    ),
    nodeAfterUpdate: captureEntry(
      'config/parity-requests/saved-searches/saved-search-mutation-first-node-read.graphql',
      updateNodeVariables,
      nodeAfterUpdate,
    ),
    listAfterUpdate: captureEntry(
      'config/parity-requests/saved-searches/saved-search-mutation-first-list-read.graphql',
      updateListVariables,
      listAfterUpdate,
    ),
    deleteExisting: captureEntry(
      'config/parity-requests/saved-searches/saved-search-mutation-first-delete.graphql',
      deleteVariables,
      deleteExisting,
    ),
    nodeAfterDelete: captureEntry(
      'config/parity-requests/saved-searches/saved-search-mutation-first-node-read.graphql',
      deleteNodeVariables,
      nodeAfterDelete,
    ),
    listAfterDelete: captureEntry(
      'config/parity-requests/saved-searches/saved-search-mutation-first-list-read.graphql',
      deleteListVariables,
      listAfterDelete,
    ),
    updateMissing: captureEntry(
      'config/parity-requests/saved-searches/saved-search-mutation-first-update.graphql',
      missingUpdateVariables,
      updateMissing,
    ),
    deleteMissing: captureEntry(
      'config/parity-requests/saved-searches/saved-search-mutation-first-delete.graphql',
      missingDeleteVariables,
      deleteMissing,
    ),
    upstreamCalls: [],
  };
} finally {
  if (updateStarted) {
    restoreUpdate = await client.runGraphqlRequest(updateDocument, {
      input: { id: updateTargetId, name: updateBaseline.name, query: updateBaseline.query },
    });
    successfulSavedSearch(restoreUpdate, 'savedSearchUpdate', 'restore persistent update target');
  }
  if (deleteStarted) {
    recreateDelete = await client.runGraphqlRequest(createDocument, {
      input: { name: deleteBaseline.name, query: deleteBaseline.query, resourceType: 'PRODUCT' },
    });
    successfulSavedSearch(recreateDelete, 'savedSearchCreate', 'recreate persistent delete target');
  }
}

if (!fixture || !recreateDelete) {
  throw new Error('Saved-search mutation-first hydration capture did not complete.');
}
fixture['cleanup'] = {
  restoreUpdate: restoreUpdate?.payload ?? null,
  recreateDelete: recreateDelete.payload,
};
const replayDeleteTarget = successfulSavedSearch(
  recreateDelete,
  'savedSearchCreate',
  'read recreated delete target for cassette',
);
const replayDeleteTargetId = replayDeleteTarget['id'];
if (typeof replayDeleteTargetId !== 'string') {
  throw new Error('Recreated delete target did not return a string ID.');
}
const replayDeleteHydrationVariables = { id: replayDeleteTargetId };
const replayDeleteHydration = await client.runGraphqlRequest(hydrationDocument, replayDeleteHydrationVariables);
assertNoTopLevelErrors(replayDeleteHydration, 'capture recreated delete-target hydration query');
const replayBaselineVariables = { first: 250, after: null };
const replayBaseline = await client.runGraphqlRequest(baselineDocument, replayBaselineVariables);
assertNoTopLevelErrors(replayBaseline, 'capture restored complete saved-search baseline for proxy overlay');
fixture['upstreamCalls'] = [
  upstreamCall(hydrationDocument, 'SavedSearchMutationTargetHydrate', updateHydrationVariables, updateHydration),
  upstreamCall(baselineDocument, 'SavedSearchConnectionBaseline', replayBaselineVariables, replayBaseline),
  upstreamCall(
    hydrationDocument,
    'SavedSearchMutationTargetHydrate',
    replayDeleteHydrationVariables,
    replayDeleteHydration,
  ),
  upstreamCall(baselineDocument, 'SavedSearchConnectionBaseline', replayBaselineVariables, replayBaseline),
  upstreamCall(hydrationDocument, 'SavedSearchMutationTargetHydrate', missingHydrationVariables, missingHydration),
  upstreamCall(hydrationDocument, 'SavedSearchMutationTargetHydrate', missingHydrationVariables, missingHydration),
];

await mkdir(outputDir, { recursive: true });
const fixturePath = path.join(outputDir, 'saved-search-mutation-first-hydration.json');
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
