import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

type CollectionNode = {
  id?: string;
  title?: string;
  handle?: string;
};

type CollectionPayload = {
  collection?: CollectionNode | null;
  userErrors?: unknown[];
};

type CollectionDeletePayload = {
  deletedCollectionId?: string | null;
  userErrors?: unknown[];
};

type CollectionCreateData = {
  collectionCreate?: CollectionPayload;
};

type CollectionUpdateData = {
  collectionUpdate?: CollectionPayload;
};

type CollectionDeleteData = {
  collectionDelete?: CollectionDeletePayload;
};

type CollectionsReadData = {
  collections?: {
    nodes?: CollectionNode[];
  };
};

const requestDir = path.join('config', 'parity-requests', 'products');
const requestPaths = {
  create: path.join(requestDir, 'collection-top-level-staged-read-create.graphql'),
  update: path.join(requestDir, 'collection-top-level-staged-read-update.graphql'),
  delete: path.join(requestDir, 'collection-top-level-staged-read-delete.graphql'),
  existingHandleLookups: path.join(requestDir, 'collection-top-level-staged-read-existing-handle-lookups.graphql'),
  countOnly: path.join(requestDir, 'collection-top-level-staged-read-count-only.graphql'),
  initialPage1: path.join(requestDir, 'collection-top-level-staged-read-initial-page1.graphql'),
  initialPage2: path.join(requestDir, 'collection-top-level-staged-read-initial-page2.graphql'),
  postUpdate: path.join(requestDir, 'collection-top-level-staged-read-post-update.graphql'),
  postDelete: path.join(requestDir, 'collection-top-level-staged-read-post-delete.graphql'),
} as const;

const EXISTING_COLLECTION_CANDIDATE_QUERY = `#graphql
query CollectionTopLevelStagedReadExistingCandidate {
  collections(first: 10, sortKey: ID) {
    nodes {
      id
      title
      handle
    }
  }
}
`;

async function readRequest(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

function userErrors(payload: CollectionPayload | CollectionDeletePayload | undefined): unknown[] {
  return Array.isArray(payload?.userErrors) ? payload.userErrors : [];
}

function assertNoUserErrors(operation: string, errors: unknown[]): void {
  if (errors.length > 0) {
    throw new Error(`${operation} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function collectionIdFromCreate(payload: ConformanceGraphqlPayload<CollectionCreateData>, label: string): string {
  const collection = payload.data?.collectionCreate?.collection;
  const id = collection?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return a collection id.`);
  }
  assertNoUserErrors(label, userErrors(payload.data?.collectionCreate));
  return id;
}

function collectionIdFromUpdate(payload: ConformanceGraphqlPayload<CollectionUpdateData>, label: string): string {
  const collection = payload.data?.collectionUpdate?.collection;
  const id = collection?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return a collection id.`);
  }
  assertNoUserErrors(label, userErrors(payload.data?.collectionUpdate));
  return id;
}

function deletedCollectionId(payload: ConformanceGraphqlPayload<CollectionDeleteData>, label: string): string {
  const id = payload.data?.collectionDelete?.deletedCollectionId;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return deletedCollectionId.`);
  }
  assertNoUserErrors(label, userErrors(payload.data?.collectionDelete));
  return id;
}

function readEndCursor(payload: ConformanceGraphqlPayload<JsonObject>): string {
  const endCursor = maybeEndCursor(payload);
  if (endCursor === null) {
    throw new Error('Initial page 1 did not return a usable endCursor.');
  }
  return endCursor;
}

function maybeEndCursor(payload: ConformanceGraphqlPayload<JsonObject>): string | null {
  const data = payload.data;
  const sharedFirstPage = data?.sharedFirstPage;
  if (typeof sharedFirstPage !== 'object' || sharedFirstPage === null) {
    return null;
  }
  const pageInfo = (sharedFirstPage as JsonObject).pageInfo;
  if (typeof pageInfo !== 'object' || pageInfo === null) {
    return null;
  }
  const endCursor = (pageInfo as JsonObject).endCursor;
  if (typeof endCursor !== 'string' || endCursor.length === 0) {
    return null;
  }
  return endCursor;
}

function connectionNodes(payload: ConformanceGraphqlPayload<JsonObject>, key: string): unknown[] {
  const connection = payload.data?.[key];
  if (typeof connection !== 'object' || connection === null) {
    return [];
  }
  const nodes = (connection as JsonObject).nodes;
  return Array.isArray(nodes) ? nodes : [];
}

function countValue(payload: ConformanceGraphqlPayload<JsonObject>, key: string): number | null {
  const count = payload.data?.[key];
  if (typeof count !== 'object' || count === null) {
    return null;
  }
  const value = (count as JsonObject).count;
  return typeof value === 'number' ? value : null;
}

function existingCollectionHandle(payload: ConformanceGraphqlPayload<CollectionsReadData>): string {
  const handle = payload.data?.collections?.nodes?.find(
    (node) => typeof node.handle === 'string' && node.handle.length > 0,
  )?.handle;
  if (typeof handle !== 'string' || handle.length === 0) {
    throw new Error('Existing collection candidate read did not return a collection with a handle.');
  }
  return handle;
}

function countOnlyValue(payload: ConformanceGraphqlPayload<JsonObject>): number | null {
  return countValue(payload, 'collectionsCount');
}

function hasObjectData(payload: ConformanceGraphqlPayload<JsonObject>, key: string): boolean {
  const value = payload.data?.[key];
  return typeof value === 'object' && value !== null;
}

function hasNullData(payload: ConformanceGraphqlPayload<JsonObject>, key: string): boolean {
  return payload.data?.[key] === null;
}

function objectHandle(payload: ConformanceGraphqlPayload<JsonObject>, key: string): string | null {
  const value = payload.data?.[key];
  if (typeof value !== 'object' || value === null) {
    return null;
  }
  const handle = (value as JsonObject).handle;
  return typeof handle === 'string' ? handle : null;
}

function page1Ready(payload: ConformanceGraphqlPayload<JsonObject>): boolean {
  const sharedFirstPage = payload.data?.sharedFirstPage;
  if (typeof sharedFirstPage !== 'object' || sharedFirstPage === null) {
    return false;
  }
  const edges = (sharedFirstPage as JsonObject).edges;
  const pageInfo = (sharedFirstPage as JsonObject).pageInfo;
  const hasNextPage =
    typeof pageInfo === 'object' && pageInfo !== null && (pageInfo as JsonObject).hasNextPage === true;
  return (
    hasObjectData(payload, 'createdByIdentifierId') &&
    hasObjectData(payload, 'createdByIdentifierHandle') &&
    hasObjectData(payload, 'createdByHandleRoot') &&
    connectionNodes(payload, 'firstByHandle').length === 1 &&
    Array.isArray(edges) &&
    edges.length === 1 &&
    hasNextPage &&
    maybeEndCursor(payload) !== null
  );
}

function existingHandleLookupReady(
  expectedHandle: string,
): (payload: ConformanceGraphqlPayload<JsonObject>) => boolean {
  return (payload) =>
    hasObjectData(payload, 'existingByIdentifier') &&
    hasObjectData(payload, 'existingByHandle') &&
    objectHandle(payload, 'existingByIdentifier') === expectedHandle &&
    objectHandle(payload, 'existingByHandle') === expectedHandle;
}

function countOnlyReady(expectedCount: number): (payload: ConformanceGraphqlPayload<JsonObject>) => boolean {
  return (payload) => countOnlyValue(payload) === expectedCount;
}

function postUpdateReady(payload: ConformanceGraphqlPayload<JsonObject>): boolean {
  return (
    hasObjectData(payload, 'updatedByIdentifierId') &&
    hasObjectData(payload, 'updatedByIdentifierHandle') &&
    hasObjectData(payload, 'updatedByHandleRoot') &&
    hasNullData(payload, 'oldIdentifierHandle') &&
    hasNullData(payload, 'oldHandleRoot') &&
    connectionNodes(payload, 'oldByHandle').length === 0 &&
    connectionNodes(payload, 'newByHandle').length === 1
  );
}

function postDeleteReady(payload: ConformanceGraphqlPayload<JsonObject>): boolean {
  return (
    connectionNodes(payload, 'remainingShared').length === 1 &&
    connectionNodes(payload, 'deletedByHandle').length === 0 &&
    countValue(payload, 'sharedCount') === 1 &&
    countValue(payload, 'deletedCount') === 0 &&
    hasNullData(payload, 'deletedIdentifierId') &&
    hasNullData(payload, 'deletedIdentifierHandle') &&
    hasNullData(payload, 'deletedHandleRoot')
  );
}

async function pollGraphql<TData>(
  label: string,
  run: () => Promise<ConformanceGraphqlPayload<TData>>,
  ready: (payload: ConformanceGraphqlPayload<TData>) => boolean,
): Promise<ConformanceGraphqlPayload<TData>> {
  let lastPayload: ConformanceGraphqlPayload<TData> | null = null;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    lastPayload = await run();
    if (ready(lastPayload)) {
      return lastPayload;
    }
    await new Promise((resolve) => {
      setTimeout(resolve, 2000);
    });
  }
  throw new Error(`${label} did not reach expected search state: ${JSON.stringify(lastPayload)}`);
}

function operationRecord<TData>(
  document: string,
  variables: Record<string, unknown>,
  response: ConformanceGraphqlPayload<TData>,
): JsonObject {
  return { document, variables, response };
}

function upstreamCallRecord<TData>(
  document: string,
  variables: Record<string, unknown>,
  response: ConformanceGraphqlPayload<TData>,
): JsonObject {
  return {
    method: 'POST',
    path: `/admin/api/${apiVersion}/graphql.json`,
    apiSurface: 'admin',
    query: document,
    variables,
    response: {
      status: 200,
      body: response,
    },
  };
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const documents = {
  create: await readRequest(requestPaths.create),
  update: await readRequest(requestPaths.update),
  delete: await readRequest(requestPaths.delete),
  existingHandleLookups: await readRequest(requestPaths.existingHandleLookups),
  countOnly: await readRequest(requestPaths.countOnly),
  initialPage1: await readRequest(requestPaths.initialPage1),
  initialPage2: await readRequest(requestPaths.initialPage2),
  postUpdate: await readRequest(requestPaths.postUpdate),
  postDelete: await readRequest(requestPaths.postDelete),
};

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'collection-top-level-staged-read.json');
const runId = `${Date.now()}`;
const titleBase = `HermesTopLevel${runId}`;
const handleBase = `hermes-top-level-${runId}`;
const firstHandle = `${handleBase}-alpha`;
const secondHandle = `${handleBase}-beta`;
const updatedFirstHandle = `${handleBase}-alpha-updated`;
const cleanupIds = new Set<string>();

let operations: JsonObject = {};
let upstreamCalls: JsonObject[] = [];

try {
  const existingCandidate = await runGraphql<CollectionsReadData>(EXISTING_COLLECTION_CANDIDATE_QUERY, {});
  const existingHandle = existingCollectionHandle(existingCandidate);

  const countOnlyVariables = {};
  const preCreateCountBaseline = await runGraphql<JsonObject>(documents.countOnly, countOnlyVariables);
  const preCreateCount = countOnlyValue(preCreateCountBaseline);
  if (preCreateCount === null) {
    throw new Error('Pre-create collectionsCount baseline did not return a count.');
  }

  const firstCreateVariables = {
    input: {
      title: `${titleBase} Alpha`,
      handle: firstHandle,
    },
  };
  const firstCreate = await runGraphql<CollectionCreateData>(documents.create, firstCreateVariables);
  const firstCollectionId = collectionIdFromCreate(firstCreate, 'first collectionCreate');
  cleanupIds.add(firstCollectionId);

  const existingHandleVariables = { existingHandle };
  const existingHandleAfterFirstCreate = await pollGraphql<JsonObject>(
    'existing collection handle lookup after first create',
    () => runGraphql<JsonObject>(documents.existingHandleLookups, existingHandleVariables),
    existingHandleLookupReady(existingHandle),
  );

  const countOnlyAfterFirstCreate = await pollGraphql<JsonObject>(
    'count-only collectionsCount after first create',
    () => runGraphql<JsonObject>(documents.countOnly, countOnlyVariables),
    countOnlyReady(preCreateCount + 1),
  );

  const secondCreateVariables = {
    input: {
      title: `${titleBase} Beta`,
      handle: secondHandle,
    },
  };
  const secondCreate = await runGraphql<CollectionCreateData>(documents.create, secondCreateVariables);
  const secondCollectionId = collectionIdFromCreate(secondCreate, 'second collectionCreate');
  cleanupIds.add(secondCollectionId);

  const sharedQuery = `title:${titleBase}*`;
  const firstHandleQuery = `handle:${firstHandle}`;
  const initialPage1Variables = {
    firstCollectionId,
    firstHandle,
    firstHandleQuery,
    sharedQuery,
    first: 1,
    limit: 1,
  };
  const initialPage1 = await pollGraphql<JsonObject>(
    'initial filtered collections read',
    () => runGraphql<JsonObject>(documents.initialPage1, initialPage1Variables),
    page1Ready,
  );

  const initialPage2Variables = {
    sharedQuery,
    after: readEndCursor(initialPage1),
  };
  const initialPage2 = await runGraphql<JsonObject>(documents.initialPage2, initialPage2Variables);

  const updateVariables = {
    input: {
      id: firstCollectionId,
      title: `${titleBase} Alpha Updated`,
      handle: updatedFirstHandle,
    },
  };
  const update = await runGraphql<CollectionUpdateData>(documents.update, updateVariables);
  collectionIdFromUpdate(update, 'collectionUpdate');

  const postUpdateVariables = {
    updatedCollectionId: firstCollectionId,
    oldHandle: firstHandle,
    newHandle: updatedFirstHandle,
    oldHandleQuery: firstHandleQuery,
    newHandleQuery: `handle:${updatedFirstHandle}`,
  };
  const postUpdate = await pollGraphql<JsonObject>(
    'post-update filtered collections read',
    () => runGraphql<JsonObject>(documents.postUpdate, postUpdateVariables),
    postUpdateReady,
  );

  const deleteVariables = {
    input: {
      id: secondCollectionId,
    },
  };
  const deleteResponse = await runGraphql<CollectionDeleteData>(documents.delete, deleteVariables);
  const deletedId = deletedCollectionId(deleteResponse, 'collectionDelete');
  cleanupIds.delete(deletedId);

  const postDeleteVariables = {
    sharedQuery,
    deletedCollectionId: secondCollectionId,
    deletedHandle: secondHandle,
    deletedHandleQuery: `handle:${secondHandle}`,
  };
  const postDelete = await pollGraphql<JsonObject>(
    'post-delete filtered collections read',
    () => runGraphql<JsonObject>(documents.postDelete, postDeleteVariables),
    postDeleteReady,
  );

  operations = {
    preCreateCountBaseline: operationRecord(documents.countOnly, countOnlyVariables, preCreateCountBaseline),
    firstCreate: operationRecord(documents.create, firstCreateVariables, firstCreate),
    existingHandleAfterFirstCreate: operationRecord(
      documents.existingHandleLookups,
      existingHandleVariables,
      existingHandleAfterFirstCreate,
    ),
    countOnlyAfterFirstCreate: operationRecord(documents.countOnly, countOnlyVariables, countOnlyAfterFirstCreate),
    secondCreate: operationRecord(documents.create, secondCreateVariables, secondCreate),
    initialPage1: operationRecord(documents.initialPage1, initialPage1Variables, initialPage1),
    initialPage2: operationRecord(documents.initialPage2, initialPage2Variables, initialPage2),
    update: operationRecord(documents.update, updateVariables, update),
    postUpdate: operationRecord(documents.postUpdate, postUpdateVariables, postUpdate),
    delete: operationRecord(documents.delete, deleteVariables, deleteResponse),
    postDelete: operationRecord(documents.postDelete, postDeleteVariables, postDelete),
  };
  upstreamCalls = [
    upstreamCallRecord(documents.existingHandleLookups, existingHandleVariables, existingHandleAfterFirstCreate),
    upstreamCallRecord(documents.countOnly, countOnlyVariables, preCreateCountBaseline),
  ];

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        fixtureKind: 'collection-top-level-staged-read',
        storeDomain,
        apiVersion,
        runId,
        operations,
        upstreamCalls,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  // oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
  console.log(JSON.stringify({ ok: true, outputPath, runId }, null, 2));
} finally {
  for (const id of cleanupIds) {
    try {
      await runGraphql<CollectionDeleteData>(documents.delete, { input: { id } });
    } catch (error) {
      // oxlint-disable-next-line no-console -- cleanup failures should be visible but not mask capture output.
      console.error(`Cleanup collectionDelete failed for ${id}:`, error);
    }
  }
}
