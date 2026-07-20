/* oxlint-disable no-console -- CLI capture scripts intentionally report status. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type Capture = {
  request: { query: string; variables: JsonRecord };
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const setupDocument = `mutation SavedSearchPartialPageSetup(
  $first: SavedSearchCreateInput!
  $second: SavedSearchCreateInput!
  $third: SavedSearchCreateInput!
) {
  first: savedSearchCreate(input: $first) {
    savedSearch { id name query resourceType }
    userErrors { field message }
  }
  second: savedSearchCreate(input: $second) {
    savedSearch { id name query resourceType }
    userErrors { field message }
  }
  third: savedSearchCreate(input: $third) {
    savedSearch { id name query resourceType }
    userErrors { field message }
  }
}
`;

const partialPageDocument = `query SavedSearchPartialPage {
  productSavedSearches(first: 1) {
    edges { cursor node { id name query resourceType } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
`;

const updateDocument = `mutation SavedSearchPartialPageUpdate($input: SavedSearchUpdateInput!) {
  savedSearchUpdate(input: $input) {
    savedSearch { id name query resourceType }
    userErrors { field message }
  }
}
`;

const deleteDocument = `mutation SavedSearchPartialPageDelete($input: SavedSearchDeleteInput!) {
  savedSearchDelete(input: $input) {
    deletedSavedSearchId
    userErrors { field message }
  }
}
`;

const createDocument = `mutation SavedSearchPartialPageCreate($input: SavedSearchCreateInput!) {
  savedSearchCreate(input: $input) {
    savedSearch { id name query resourceType }
    userErrors { field message }
  }
}
`;

const reversePageDocument = `query SavedSearchPartialPageReverse {
  productSavedSearches(first: 1, reverse: true) {
    edges { cursor node { id name query resourceType } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
`;

const afterPageDocument = `query SavedSearchPartialPageAfter($after: String!) {
  productSavedSearches(first: 1, after: $after) {
    edges { cursor node { id name query resourceType } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
`;

const beforePageDocument = `query SavedSearchPartialPageBefore($before: String!) {
  productSavedSearches(last: 1, before: $before) {
    edges { cursor node { id name query resourceType } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
}
`;

const forwardWindowDocument =
  'query SavedSearchConnectionWindow($first: Int!, $after: String, $before: String, $reverse: Boolean!) {\n  savedSearchWindow: productSavedSearches(first: $first, after: $after, before: $before, reverse: $reverse) {\n    edges { cursor node { id name query resourceType } }\n    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }\n  }\n}';
const backwardWindowDocument =
  'query SavedSearchConnectionWindow($last: Int!, $after: String, $before: String, $reverse: Boolean!) {\n  savedSearchWindow: productSavedSearches(last: $last, after: $after, before: $before, reverse: $reverse) {\n    edges { cursor node { id name query resourceType } }\n    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }\n  }\n}';

const cleanupCatalogDocument = `query SavedSearchPartialPageCleanupCatalog {
  productSavedSearches(first: 250) { nodes { id } }
}
`;
const cleanupDeleteDocument = `mutation SavedSearchPartialPageCleanup($input: SavedSearchDeleteInput!) {
  savedSearchDelete(input: $input) { deletedSavedSearchId userErrors { field message } }
}
`;

function object(value: unknown, label: string): JsonRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`Expected ${label} to be an object.`);
  }
  return value as JsonRecord;
}

function array(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`Expected ${label} to be an array.`);
  return value;
}

function data(capture: Capture): JsonRecord {
  return object(object(capture.response, 'response')['data'], 'response.data');
}

function root(capture: Capture, name: string): JsonRecord {
  return object(data(capture)[name], `response.data.${name}`);
}

function savedSearchId(capture: Capture, alias: string): string {
  const savedSearch = object(root(capture, alias)['savedSearch'], `${alias}.savedSearch`);
  const id = savedSearch['id'];
  if (typeof id !== 'string') throw new Error(`Expected ${alias}.savedSearch.id.`);
  return id;
}

function edgeCursor(capture: Capture, index: number): string {
  const connection = root(capture, 'productSavedSearches');
  const edge = object(array(connection['edges'], 'productSavedSearches.edges')[index], `edge ${index}`);
  const cursor = edge['cursor'];
  if (typeof cursor !== 'string' || cursor.length === 0) {
    throw new Error(`Expected edge ${index} to have a cursor.`);
  }
  return cursor;
}

function assertNoErrors(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertEmptyUserErrors(capture: Capture, aliases: string[]): void {
  for (const alias of aliases) {
    const userErrors = root(capture, alias)['userErrors'];
    if (!Array.isArray(userErrors) || userErrors.length !== 0) {
      throw new Error(`${alias} returned userErrors: ${JSON.stringify(userErrors)}`);
    }
  }
}

async function capture(query: string, variables: JsonRecord, label: string): Promise<Capture> {
  const result = await client.runGraphqlRequest(query, variables);
  assertNoErrors(result, label);
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function upstreamCall(captureResult: Capture, operationName?: string): JsonRecord {
  return {
    ...(operationName === undefined ? {} : { operationName }),
    query: captureResult.request.query,
    variables: captureResult.request.variables,
    response: { status: captureResult.status, body: captureResult.response },
  };
}

async function deleteSavedSearch(id: string): Promise<void> {
  const deleted = await capture(cleanupDeleteDocument, { input: { id } }, `cleanup saved search ${id}`);
  assertEmptyUserErrors(deleted, ['savedSearchDelete']);
}

async function clearProductSavedSearches(): Promise<void> {
  for (;;) {
    const catalog = await capture(cleanupCatalogDocument, {}, 'saved-search cleanup catalog');
    const nodes = array(root(catalog, 'productSavedSearches')['nodes'], 'productSavedSearches.nodes');
    if (nodes.length === 0) return;
    for (const node of nodes) {
      const id = object(node, 'saved-search cleanup node')['id'];
      if (typeof id === 'string') await deleteSavedSearch(id);
    }
  }
}

const suffix = Date.now().toString(36);
const firstName = `Partial window ${suffix} A`;
const secondName = `Partial window ${suffix} B`;
const thirdName = `Partial window ${suffix} C`;
const createdName = `Partial window ${suffix} D`;
const updatedName = `Partial window ${suffix} A updated`;
const setupVariables = {
  first: { resourceType: 'PRODUCT', name: firstName, query: `title:${suffix}a` },
  second: { resourceType: 'PRODUCT', name: secondName, query: `title:${suffix}b` },
  third: { resourceType: 'PRODUCT', name: thirdName, query: `title:${suffix}c` },
};
const createVariables = {
  input: { resourceType: 'PRODUCT', name: createdName, query: `title:${suffix}d` },
};
const cleanupIds = new Set<string>();

await clearProductSavedSearches();
try {
  const setup = await capture(setupDocument, setupVariables, 'saved-search partial-page setup');
  assertEmptyUserErrors(setup, ['first', 'second', 'third']);
  const firstId = savedSearchId(setup, 'first');
  const secondId = savedSearchId(setup, 'second');
  const thirdId = savedSearchId(setup, 'third');
  cleanupIds.add(firstId);
  cleanupIds.add(secondId);
  cleanupIds.add(thirdId);

  const partialPage = await capture(partialPageDocument, {}, 'authoritative partial page');
  const firstCursor = edgeCursor(partialPage, 0);

  const preUpdateCallerPage = await capture(partialPageDocument, {}, 'pre-update caller-shaped partial page');
  const updateWindow = await capture(
    forwardWindowDocument,
    { first: 2, after: null, before: null, reverse: false },
    'pre-update bounded window',
  );
  const update = await capture(updateDocument, { input: { id: firstId, name: updatedName } }, 'saved-search update');
  assertEmptyUserErrors(update, ['savedSearchUpdate']);
  const postUpdatePage = await capture(partialPageDocument, {}, 'partial page after update');
  if (edgeCursor(postUpdatePage, 0) !== firstCursor) {
    throw new Error('Shopify changed the authoritative first-row cursor after savedSearchUpdate.');
  }

  const deleteWindow = await capture(
    forwardWindowDocument,
    { first: 2, after: null, before: null, reverse: false },
    'pre-delete bounded window',
  );
  const deleted = await capture(deleteDocument, { input: { id: firstId } }, 'saved-search delete');
  assertEmptyUserErrors(deleted, ['savedSearchDelete']);
  cleanupIds.delete(firstId);
  const postDeletePage = await capture(partialPageDocument, {}, 'partial page after delete');
  const secondCursor = edgeCursor(postDeletePage, 0);

  const preCreateReversePage = await capture(reversePageDocument, {}, 'pre-create caller-shaped reverse partial page');
  const reverseWindow = await capture(
    forwardWindowDocument,
    { first: 3, after: null, before: null, reverse: true },
    'pre-create reverse bounded window',
  );
  const preCreateAfterPage = await capture(
    afterPageDocument,
    { after: secondCursor },
    'pre-create caller-shaped after partial page',
  );
  const afterWindow = await capture(
    forwardWindowDocument,
    { first: 3, after: secondCursor, before: null, reverse: false },
    'pre-create after bounded window',
  );
  const thirdCursor = edgeCursor(preCreateAfterPage, 0);
  const preCreateBeforePage = await capture(
    beforePageDocument,
    { before: thirdCursor },
    'pre-create caller-shaped before partial page',
  );
  const beforeWindow = await capture(
    backwardWindowDocument,
    { last: 3, after: null, before: thirdCursor, reverse: false },
    'pre-create before bounded window',
  );

  const created = await capture(createDocument, createVariables, 'saved-search create');
  assertEmptyUserErrors(created, ['savedSearchCreate']);
  const createdId = savedSearchId(created, 'savedSearchCreate');
  cleanupIds.add(createdId);
  const reversePage = await capture(reversePageDocument, {}, 'reverse partial page after create');
  const afterPage = await capture(
    afterPageDocument,
    { after: secondCursor },
    'after partial page with staged-equivalent create',
  );
  const beforePage = await capture(
    beforePageDocument,
    { before: edgeCursor(afterPage, 0) },
    'before partial page with staged-equivalent create',
  );

  const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'saved-searches');
  const requestDir = path.join('config', 'parity-requests', 'saved-searches');
  const specDir = path.join('config', 'parity-specs', 'saved-searches');
  await Promise.all([
    mkdir(fixtureDir, { recursive: true }),
    mkdir(requestDir, { recursive: true }),
    mkdir(specDir, { recursive: true }),
  ]);
  const fixturePath = path.join(fixtureDir, 'saved-search-partial-page-overlay.json');
  const requestPaths = {
    partialPage: path.join(requestDir, 'saved-search-partial-page-read.graphql'),
    update: path.join(requestDir, 'saved-search-partial-page-update.graphql'),
    delete: path.join(requestDir, 'saved-search-partial-page-delete.graphql'),
    create: path.join(requestDir, 'saved-search-partial-page-create.graphql'),
    reversePage: path.join(requestDir, 'saved-search-partial-page-reverse.graphql'),
    afterPage: path.join(requestDir, 'saved-search-partial-page-after.graphql'),
    beforePage: path.join(requestDir, 'saved-search-partial-page-before.graphql'),
  };
  const fixture = {
    metadata: {
      source: 'live-shopify-admin-graphql',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      scenario: 'saved-search-partial-page-overlay',
    },
    setup,
    partialPage,
    preUpdateCallerPage,
    update,
    postUpdatePage,
    deleted,
    postDeletePage,
    preCreateReversePage,
    preCreateAfterPage,
    preCreateBeforePage,
    created,
    reversePage,
    afterPage,
    beforePage,
    upstreamCalls: [
      upstreamCall(partialPage),
      upstreamCall(preUpdateCallerPage),
      upstreamCall(updateWindow, 'SavedSearchConnectionWindow'),
      upstreamCall(postUpdatePage),
      upstreamCall(deleteWindow, 'SavedSearchConnectionWindow'),
      upstreamCall(preCreateReversePage),
      upstreamCall(reverseWindow, 'SavedSearchConnectionWindow'),
      upstreamCall(preCreateAfterPage),
      upstreamCall(afterWindow, 'SavedSearchConnectionWindow'),
      upstreamCall(preCreateBeforePage),
      upstreamCall(beforeWindow, 'SavedSearchConnectionWindow'),
    ],
  };
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await Promise.all([
    writeFile(requestPaths.partialPage, partialPageDocument, 'utf8'),
    writeFile(requestPaths.update, updateDocument, 'utf8'),
    writeFile(requestPaths.delete, deleteDocument, 'utf8'),
    writeFile(requestPaths.create, createDocument, 'utf8'),
    writeFile(requestPaths.reversePage, reversePageDocument, 'utf8'),
    writeFile(requestPaths.afterPage, afterPageDocument, 'utf8'),
    writeFile(requestPaths.beforePage, beforePageDocument, 'utf8'),
  ]);

  const spec = {
    scenarioId: 'saved-search-partial-page-overlay',
    operationNames: ['productSavedSearches', 'savedSearchCreate', 'savedSearchUpdate', 'savedSearchDelete'],
    scenarioStatus: 'captured',
    assertionKinds: ['pagination-shape', 'downstream-read-parity', 'mutation-lifecycle'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/products_saved_searches.rs'],
    proxyRequest: {
      documentPath: requestPaths.partialPage,
      variablesCapturePath: '$.partialPage.request.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify evidence for an authoritative first:1 partial page and saved-search update, delete refill, create/reverse, after, and last/before overlays. Authoritative row IDs and cursors compare exactly; only the locally created resource identity and cursor are volatile.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'authoritative-first-partial-page',
          preserveProxyState: true,
          capturePath: '$.partialPage.response.data.productSavedSearches',
          proxyPath: '$.data.productSavedSearches',
        },
        {
          name: 'stage-authoritative-saved-search-update',
          preserveProxyState: true,
          capturePath: '$.update.response.data.savedSearchUpdate',
          proxyPath: '$.data.savedSearchUpdate',
          proxyRequest: {
            documentPath: requestPaths.update,
            variables: {
              input: {
                id: {
                  fromPrimaryProxyPath: '$.data.productSavedSearches.edges[0].node.id',
                },
                name: { fromCapturePath: '$.update.request.variables.input.name' },
              },
            },
            apiVersion,
          },
        },
        {
          name: 'post-update-authoritative-partial-page',
          preserveProxyState: true,
          capturePath: '$.postUpdatePage.response.data.productSavedSearches',
          proxyPath: '$.data.productSavedSearches',
          proxyRequest: {
            documentPath: requestPaths.partialPage,
            variablesCapturePath: '$.postUpdatePage.request.variables',
            apiVersion,
          },
        },
        {
          name: 'stage-authoritative-saved-search-delete',
          preserveProxyState: true,
          capturePath: '$.deleted.response.data.savedSearchDelete',
          proxyPath: '$.data.savedSearchDelete',
          proxyRequest: {
            documentPath: requestPaths.delete,
            variables: {
              input: {
                id: {
                  fromPrimaryProxyPath: '$.data.productSavedSearches.edges[0].node.id',
                },
              },
            },
            apiVersion,
          },
        },
        {
          name: 'post-delete-partial-page-refill',
          preserveProxyState: true,
          capturePath: '$.postDeletePage.response.data.productSavedSearches',
          proxyPath: '$.data.productSavedSearches',
          proxyRequest: {
            documentPath: requestPaths.partialPage,
            variablesCapturePath: '$.postDeletePage.request.variables',
            apiVersion,
          },
        },
        {
          name: 'stage-tail-saved-search-create',
          preserveProxyState: true,
          capturePath: '$.created.response.data.savedSearchCreate',
          proxyPath: '$.data.savedSearchCreate',
          proxyRequest: {
            documentPath: requestPaths.create,
            variablesCapturePath: '$.created.request.variables',
            apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.savedSearch.id',
              matcher: 'shopify-gid:SavedSearch',
              reason:
                'The proxy stages a deterministic local SavedSearch while Shopify created the disposable live record.',
            },
          ],
        },
        {
          name: 'post-create-reverse-partial-page',
          preserveProxyState: true,
          capturePath: '$.reversePage.response.data.productSavedSearches',
          proxyPath: '$.data.productSavedSearches',
          proxyRequest: {
            documentPath: requestPaths.reversePage,
            variablesCapturePath: '$.reversePage.request.variables',
            apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.edges[0].node.id',
              matcher: 'shopify-gid:SavedSearch',
              reason: 'The reverse page selects the proxy-local SavedSearch instead of the disposable live record.',
            },
            {
              path: '$.edges[0].cursor',
              matcher: 'non-empty-string',
              reason:
                'The locally staged SavedSearch uses a stable proxy cursor while Shopify returned an opaque live cursor.',
            },
            {
              path: '$.pageInfo.startCursor',
              matcher: 'non-empty-string',
              reason: 'The selected locally staged SavedSearch has a stable proxy boundary cursor.',
            },
            {
              path: '$.pageInfo.endCursor',
              matcher: 'non-empty-string',
              reason: 'The selected locally staged SavedSearch has a stable proxy boundary cursor.',
            },
          ],
        },
        {
          name: 'post-create-after-authoritative-page',
          preserveProxyState: true,
          capturePath: '$.afterPage.response.data.productSavedSearches',
          proxyPath: '$.data.productSavedSearches',
          proxyRequest: {
            documentPath: requestPaths.afterPage,
            variables: {
              after: {
                fromProxyResponse: 'post-delete-partial-page-refill',
                path: '$.data.productSavedSearches.edges[0].cursor',
              },
            },
            apiVersion,
          },
        },
        {
          name: 'post-create-last-before-authoritative-page',
          preserveProxyState: true,
          capturePath: '$.beforePage.response.data.productSavedSearches',
          proxyPath: '$.data.productSavedSearches',
          proxyRequest: {
            documentPath: requestPaths.beforePage,
            variables: {
              before: {
                fromProxyResponse: 'post-create-after-authoritative-page',
                path: '$.data.productSavedSearches.edges[0].cursor',
              },
            },
            apiVersion,
          },
        },
      ],
    },
  };
  const specPath = path.join(specDir, 'saved-search-partial-page-overlay.json');
  await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath, specPath }, null, 2));
} finally {
  for (const id of cleanupIds) {
    try {
      await deleteSavedSearch(id);
    } catch (error) {
      console.error(`Failed to clean up saved search ${id}:`, error);
    }
  }
}
