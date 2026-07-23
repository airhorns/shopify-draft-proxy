/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import {
  buildAdminAuthHeaders,
  buildStorefrontRequestHeaders,
  getStoredStorefrontAccessToken,
  getValidConformanceAccessToken,
} from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: { query: string; variables: Record<string, unknown> };
  status: number;
  response: unknown;
};

type GraphqlUpstreamCapture = {
  name: string;
  method: 'POST';
  apiSurface: 'admin' | 'storefront';
  apiVersion: string;
  path: string;
  endpoint: string;
  authMode: 'admin-access-token' | 'storefront-access-token';
  headers?: Record<string, string>;
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  response: { status: number; body: unknown };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const storedStorefrontAuth = await getStoredStorefrontAccessToken();
if (storedStorefrontAuth.shop && storedStorefrontAuth.shop !== storeDomain) {
  throw new Error(
    `Stored Storefront token is for ${storedStorefrontAuth.shop}, but the configured store is ${storeDomain}.`,
  );
}

const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const adminEndpoint = `${adminOrigin}/admin/api/${apiVersion}/graphql.json`;
const adminPath = `/admin/api/${apiVersion}/graphql.json`;
const storefrontEndpoint = `https://${storeDomain}/api/${apiVersion}/graphql.json`;
const storefrontPath = `/api/${apiVersion}/graphql.json`;
const storefrontOptions = {
  storeOrigin: `https://${storeDomain}`,
  apiVersion,
  storefrontAccessToken: storedStorefrontAuth.storefront_access_token,
};
const storefrontRedactedHeaders = Object.fromEntries(
  Object.keys(buildStorefrontRequestHeaders(storedStorefrontAuth.storefront_access_token)).map((name) => [
    name,
    '<redacted:storefront-access-token>',
  ]),
);

const documents = {
  setup: 'config/parity-requests/storefront/storefront-discovery-setup-admin.graphql',
  articleSetup: 'config/parity-requests/storefront/storefront-discovery-article-setup-admin.graphql',
  publicationHydrate: 'config/parity-requests/storefront/storefront-catalog-publications-hydrate-admin.graphql',
  publish: 'config/parity-requests/storefront/storefront-discovery-publish-admin.graphql',
  discoveryRead: 'config/parity-requests/storefront/storefront-discovery-read.graphql',
  unrelatedSetup: 'config/parity-requests/storefront/storefront-node-unrelated-setup-admin.graphql',
  nodeOverlayRead: 'config/parity-requests/storefront/storefront-node-overlay-read.graphql',
  menuHydrate: 'config/parity-requests/storefront/storefront-content-menu-hydrate.graphql',
  pageTwoSetup: 'config/parity-requests/storefront/storefront-discovery-page-two-setup-admin.graphql',
  pagination: 'config/parity-requests/storefront/storefront-discovery-pagination.graphql',
  invalidId: 'config/parity-requests/storefront/storefront-discovery-invalid-id.graphql',
  invalidLimit: 'config/parity-requests/storefront/storefront-discovery-invalid-limit.graphql',
} as const;
const documentText = Object.fromEntries(
  await Promise.all(
    Object.entries(documents).map(async ([name, documentPath]) => [name, await readFile(documentPath, 'utf8')]),
  ),
) as Record<keyof typeof documents, string>;

const suffix = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const phrase = `Aurora Discovery ${suffix}`;
const productTag = `discovery-${suffix}`;
const setupVariables = {
  product: {
    title: `${phrase} Product`,
    handle: `aurora-discovery-product-${suffix}`,
    status: 'ACTIVE',
    vendor: 'Hermes Discovery',
    productType: 'Discovery Fixture',
    tags: ['aurora', 'discovery', productTag],
    productOptions: [{ name: 'Color', values: [{ name: 'Blue' }] }],
  },
  collection: {
    title: `${phrase} Collection`,
    handle: `aurora-discovery-collection-${suffix}`,
  },
  blog: {
    title: `${phrase} Blog`,
    handle: `aurora-discovery-blog-${suffix}`,
  },
  page: {
    title: `${phrase} Alpha Page`,
    handle: `aurora-discovery-alpha-page-${suffix}`,
    body: `<p>${phrase} alpha page body</p>`,
    isPublished: true,
  },
} satisfies Record<string, unknown>;
const unrelatedSetupVariables = {
  product: {
    title: `Hermes Cold Node Product ${suffix}`,
    handle: `hermes-cold-node-product-${suffix}`,
    status: 'ACTIVE',
    vendor: 'Hermes Node Fidelity',
    productType: 'Node Fixture',
    tags: ['cold-node', `cold-node-${suffix}`],
  },
  collection: {
    title: `Hermes Cold Node Collection ${suffix}`,
    handle: `hermes-cold-node-collection-${suffix}`,
  },
  page: {
    title: `Hermes Cold Node Page ${suffix}`,
    handle: `hermes-cold-node-page-${suffix}`,
    body: `<p>Hermes cold Node page ${suffix}</p>`,
    isPublished: true,
  },
} satisfies Record<string, unknown>;

const adminCaptures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let setupCapture: Capture | null = null;
let articleSetupCapture: Capture | null = null;
let publicationHydrateCapture: GraphqlUpstreamCapture | null = null;
let publishCapture: Capture | null = null;
let discoveryCapture: GraphqlUpstreamCapture | null = null;
let pageTwoSetupCapture: Capture | null = null;
let discoveryPaginationCapture: GraphqlUpstreamCapture | null = null;
let discoveryPageTwoCapture: GraphqlUpstreamCapture | null = null;
let invalidIdCapture: GraphqlUpstreamCapture | null = null;
let invalidLimitCapture: GraphqlUpstreamCapture | null = null;
let unrelatedSetupCapture: Capture | null = null;
let unrelatedPublishCapture: Capture | null = null;
let menuHydrateCapture: GraphqlUpstreamCapture | null = null;
let nodeOverlayExpectedCapture: GraphqlUpstreamCapture | null = null;
let nodeOverlayUpstreamCapture: GraphqlUpstreamCapture | null = null;
let productId: string | null = null;
let collectionId: string | null = null;
let blogId: string | null = null;
let pageId: string | null = null;
let pageTwoId: string | null = null;
let articleId: string | null = null;
let unrelatedProductId: string | null = null;
let unrelatedCollectionId: string | null = null;
let unrelatedPageId: string | null = null;

async function captureAdmin(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  const capture = { name, request: { query, variables }, status: result.status, response: result.payload };
  adminCaptures.push(capture);
  return capture;
}

async function captureAdminCleanup(name: string, query: string, variables: Record<string, unknown>): Promise<void> {
  const result = await runGraphqlRaw(query, variables);
  cleanupCaptures.push({
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  });
}

function adminUpstreamCapture(capture: Capture, operationName: string): GraphqlUpstreamCapture {
  return {
    name: capture.name,
    method: 'POST',
    apiSurface: 'admin',
    apiVersion,
    path: adminPath,
    endpoint: adminEndpoint,
    authMode: 'admin-access-token',
    operationName,
    query: capture.request.query,
    variables: capture.request.variables,
    response: { status: capture.status, body: capture.response },
  };
}

async function storefrontRequest(
  name: string,
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<GraphqlUpstreamCapture> {
  const result = await runStorefrontGraphqlRequest(storefrontOptions, query, variables);
  return {
    name,
    method: 'POST',
    apiSurface: 'storefront',
    apiVersion,
    path: storefrontPath,
    endpoint: storefrontEndpoint,
    authMode: 'storefront-access-token',
    headers: storefrontRedactedHeaders,
    operationName,
    query,
    variables,
    response: { status: result.status, body: result.payload },
  };
}

function readPath(value: unknown, segments: Array<string | number>): unknown {
  return segments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) return null;
    return (current as Record<string | number, unknown>)[segment] ?? null;
  }, value);
}

function requiredString(value: unknown, segments: Array<string | number>, label: string): string {
  const result = readPath(value, segments);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`${label} did not return a string at ${segments.join('.')}: ${JSON.stringify(value)}`);
  }
  return result;
}

function assertNoTopLevelErrors(payload: unknown, label: string): void {
  const errors = readPath(payload, ['errors']);
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, segments: string[], label: string): void {
  const errors = readPath(payload, segments);
  if (Array.isArray(errors) && errors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
}

function discoveryReady(payload: unknown, expectedIds: string[]): boolean {
  const nodes = readPath(payload, ['data', 'aliasedNodes']);
  const predictive = readPath(payload, ['data', 'predictiveEach']);
  const suggestions = readPath(payload, ['data', 'suggestions', 'queries']);
  if (!Array.isArray(nodes) || typeof predictive !== 'object' || predictive === null || !Array.isArray(suggestions)) {
    return false;
  }
  const returnedIds = nodes.map((node) => readPath(node, ['id']));
  const predictiveIds = ['products', 'collections', 'articles', 'pages'].map((key) =>
    readPath(predictive, [key, 0, 'id']),
  );
  const prefixResultIds = Array.isArray(readPath(payload, ['data', 'prefixLast', 'nodes']))
    ? (readPath(payload, ['data', 'prefixLast', 'nodes']) as unknown[]).map((node) => readPath(node, ['id']))
    : [];
  const prefixNoneIds = Array.isArray(readPath(payload, ['data', 'prefixNone', 'nodes']))
    ? (readPath(payload, ['data', 'prefixNone', 'nodes']) as unknown[]).map((node) => readPath(node, ['id']))
    : [];
  const mixedIds = Array.isArray(readPath(payload, ['data', 'mixed', 'nodes']))
    ? (readPath(payload, ['data', 'mixed', 'nodes']) as unknown[]).map((node) => readPath(node, ['id']))
    : [];
  const predictiveAll = readPath(payload, ['data', 'predictiveAll']);
  return (
    returnedIds.length === 6 &&
    returnedIds[1] === null &&
    expectedIds.every((id) => returnedIds.includes(id)) &&
    expectedIds.every((id) => predictiveIds.includes(id)) &&
    suggestions.length > 0 &&
    [expectedIds[0], expectedIds[2], expectedIds[3]].every((id) => prefixResultIds.includes(id)) &&
    mixedIds.length === 1 &&
    mixedIds[0] === expectedIds[0] &&
    readPath(payload, ['data', 'mixed', 'totalCount']) === 1 &&
    prefixNoneIds.length === 1 &&
    prefixNoneIds[0] === expectedIds[0] &&
    readPath(payload, ['data', 'prefixNone', 'totalCount']) === 1 &&
    readPath(payload, ['data', 'prefixLast', 'totalCount']) === 3 &&
    readPath(payload, ['data', 'filteredProducts', 'nodes', 0, 'id']) === expectedIds[0] &&
    readPath(payload, ['data', 'filteredProducts', 'totalCount']) === 1 &&
    readPath(payload, ['data', 'hiddenUnavailableProducts', 'nodes', 0, 'id']) === expectedIds[0] &&
    readPath(payload, ['data', 'hiddenUnavailableProducts', 'totalCount']) === 1 &&
    readPath(payload, ['data', 'empty', 'totalCount']) === 0 &&
    readPath(predictiveAll, ['products', 0, 'id']) === expectedIds[0] &&
    readPath(predictiveAll, ['collections', 0, 'id']) === expectedIds[1] &&
    readPath(predictiveAll, ['pages', 0, 'id']) === expectedIds[3] &&
    Array.isArray(readPath(predictiveAll, ['articles'])) &&
    (readPath(predictiveAll, ['articles']) as unknown[]).length === 0 &&
    readPath(payload, ['data', 'emptyPredictive', 'products', 0]) === null &&
    readPath(payload, ['data', 'emptyPredictive', 'collections', 0]) === null &&
    readPath(payload, ['data', 'emptyPredictive', 'articles', 0]) === null &&
    readPath(payload, ['data', 'emptyPredictive', 'pages', 0]) === null
  );
}

async function waitForDiscovery(
  variables: Record<string, unknown>,
  expectedIds: string[],
): Promise<GraphqlUpstreamCapture> {
  let lastCapture: GraphqlUpstreamCapture | null = null;
  for (let attempt = 1; attempt <= 60; attempt += 1) {
    lastCapture = await storefrontRequest(
      'storefront-discovery-read',
      'StorefrontDiscoveryRead',
      documentText.discoveryRead,
      variables,
    );
    assertNoTopLevelErrors(lastCapture.response.body, `Storefront discovery attempt ${attempt}`);
    if (discoveryReady(lastCapture.response.body, expectedIds)) return lastCapture;
    await delay(2000);
  }
  throw new Error(
    `Storefront discovery did not index the disposable resources: ${JSON.stringify(lastCapture, null, 2)}`,
  );
}

function nodeOverlayMatches(payload: unknown, menuId: string, expectedMixedIds: Array<string | null>): boolean {
  const singleMenuId = readPath(payload, ['data', 'singleMenu', 'id']);
  const mixed = readPath(payload, ['data', 'mixed']);
  if (singleMenuId !== menuId || !Array.isArray(mixed) || mixed.length !== expectedMixedIds.length) {
    return false;
  }
  return expectedMixedIds.every((expectedId, index) => readPath(mixed[index], ['id']) === expectedId);
}

async function waitForNodeOverlay(
  variables: Record<string, unknown>,
  menuId: string,
  expectedMixedIds: Array<string | null>,
): Promise<GraphqlUpstreamCapture> {
  let lastCapture: GraphqlUpstreamCapture | null = null;
  for (let attempt = 1; attempt <= 60; attempt += 1) {
    lastCapture = await storefrontRequest(
      'storefront-node-overlay-expected',
      'StorefrontNodeOverlayRead',
      documentText.nodeOverlayRead,
      variables,
    );
    assertNoTopLevelErrors(lastCapture.response.body, `Storefront Node overlay attempt ${attempt}`);
    if (nodeOverlayMatches(lastCapture.response.body, menuId, expectedMixedIds)) return lastCapture;
    await delay(2000);
  }
  throw new Error(`Storefront Node overlay resources did not become visible: ${JSON.stringify(lastCapture, null, 2)}`);
}

async function waitForPagination(query: string, pageIds: string[]): Promise<GraphqlUpstreamCapture> {
  let lastCapture: GraphqlUpstreamCapture | null = null;
  for (let attempt = 1; attempt <= 60; attempt += 1) {
    lastCapture = await storefrontRequest(
      'storefront-discovery-pagination',
      'StorefrontDiscoveryPagination',
      documentText.pagination,
      { query, after: null },
    );
    assertNoTopLevelErrors(lastCapture.response.body, `Storefront pagination attempt ${attempt}`);
    const firstId = readPath(lastCapture.response.body, ['data', 'pagination', 'nodes', 0, 'id']);
    if (
      typeof firstId === 'string' &&
      pageIds.includes(firstId) &&
      readPath(lastCapture.response.body, ['data', 'pagination', 'totalCount']) === 2 &&
      readPath(lastCapture.response.body, ['data', 'pagination', 'pageInfo', 'hasNextPage']) === true
    ) {
      return lastCapture;
    }
    await delay(2000);
  }
  throw new Error(`Storefront pagination did not index both disposable pages: ${JSON.stringify(lastCapture, null, 2)}`);
}

const productDeleteMutation = `#graphql
  mutation StorefrontDiscoveryProductCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) { deletedProductId userErrors { field message } }
  }
`;
const collectionDeleteMutation = `#graphql
  mutation StorefrontDiscoveryCollectionCleanup($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) { deletedCollectionId userErrors { field message } }
  }
`;
const articleDeleteMutation = `#graphql
  mutation StorefrontDiscoveryArticleCleanup($id: ID!) {
    articleDelete(id: $id) { deletedArticleId userErrors { field message code } }
  }
`;
const pageDeleteMutation = `#graphql
  mutation StorefrontDiscoveryPageCleanup($id: ID!) {
    pageDelete(id: $id) { deletedPageId userErrors { field message code } }
  }
`;
const blogDeleteMutation = `#graphql
  mutation StorefrontDiscoveryBlogCleanup($id: ID!) {
    blogDelete(id: $id) { deletedBlogId userErrors { field message code } }
  }
`;

try {
  setupCapture = await captureAdmin('admin-setup', documentText.setup, setupVariables);
  assertNoTopLevelErrors(setupCapture.response, 'discovery setup');
  for (const root of ['productCreate', 'collectionCreate', 'blogCreate', 'pageCreate']) {
    assertNoUserErrors(setupCapture.response, ['data', root, 'userErrors'], root);
  }
  productId = requiredString(setupCapture.response, ['data', 'productCreate', 'product', 'id'], 'product id');
  collectionId = requiredString(
    setupCapture.response,
    ['data', 'collectionCreate', 'collection', 'id'],
    'collection id',
  );
  blogId = requiredString(setupCapture.response, ['data', 'blogCreate', 'blog', 'id'], 'blog id');
  pageId = requiredString(setupCapture.response, ['data', 'pageCreate', 'page', 'id'], 'page id');

  articleSetupCapture = await captureAdmin('admin-article-setup', documentText.articleSetup, {
    article: {
      title: `${phrase} Article`,
      handle: `aurora-discovery-article-${suffix}`,
      body: `<p>${phrase} article body</p>`,
      summary: `${phrase} article summary`,
      tags: ['aurora', 'discovery'],
      author: { name: 'Discovery Author' },
      blogId,
      isPublished: true,
    },
  });
  assertNoTopLevelErrors(articleSetupCapture.response, 'article setup');
  assertNoUserErrors(articleSetupCapture.response, ['data', 'articleCreate', 'userErrors'], 'articleCreate');
  articleId = requiredString(articleSetupCapture.response, ['data', 'articleCreate', 'article', 'id'], 'article id');

  const publicationCapture = await captureAdmin('admin-publication-hydrate', documentText.publicationHydrate, {});
  assertNoTopLevelErrors(publicationCapture.response, 'publication hydrate');
  publicationHydrateCapture = adminUpstreamCapture(
    publicationCapture,
    'StorePropertiesPublishableInputValidationHydrate',
  );
  const publicationNodes = readPath(publicationCapture.response, ['data', 'publications', 'nodes']);
  if (!Array.isArray(publicationNodes)) throw new Error('Publication hydrate did not return nodes.');
  const storefrontPublication = publicationNodes.find((node) => readPath(node, ['name']) === 'Online Store');
  const publicationId = requiredString(storefrontPublication, ['id'], 'Online Store publication id');
  publishCapture = await captureAdmin('admin-publish', documentText.publish, {
    productId,
    collectionId,
    input: [{ publicationId }],
    publicationId,
  });
  assertNoTopLevelErrors(publishCapture.response, 'discovery publish');
  assertNoUserErrors(publishCapture.response, ['data', 'product', 'userErrors'], 'product publish');
  assertNoUserErrors(publishCapture.response, ['data', 'collection', 'userErrors'], 'collection publish');

  const indexedPrefixQuery = `Aurora Discovery ${suffix.slice(0, 12)}`;
  const discoveryVariables = {
    productId,
    nodeIds: [pageId, 'gid://shopify/Product/999999999999999999', collectionId, articleId, productId, pageId],
    query: indexedPrefixQuery,
    prefixQuery: indexedPrefixQuery,
    emptyQuery: `zz-no-result-${suffix}`,
    suggestionsQuery: 'a',
    tag: productTag,
    after: null,
  } satisfies Record<string, unknown>;
  discoveryCapture = await waitForDiscovery(discoveryVariables, [productId, collectionId, articleId, pageId]);

  pageTwoSetupCapture = await captureAdmin('admin-second-page-setup', documentText.pageTwoSetup, {
    page: {
      title: `${phrase} Aardvark Page`,
      handle: `aurora-discovery-beta-page-${suffix}`,
      body: `<p>${phrase} beta page body</p>`,
      isPublished: true,
    },
  });
  assertNoTopLevelErrors(pageTwoSetupCapture.response, 'second page setup');
  assertNoUserErrors(pageTwoSetupCapture.response, ['data', 'pageCreate', 'userErrors'], 'second page create');
  pageTwoId = requiredString(pageTwoSetupCapture.response, ['data', 'pageCreate', 'page', 'id'], 'second page id');

  discoveryPaginationCapture = await waitForPagination(indexedPrefixQuery, [pageId, pageTwoId]);
  const endCursor = requiredString(
    discoveryPaginationCapture.response.body,
    ['data', 'pagination', 'pageInfo', 'endCursor'],
    'search pagination end cursor',
  );
  const firstPageId = requiredString(
    discoveryPaginationCapture.response.body,
    ['data', 'pagination', 'nodes', 0, 'id'],
    'search pagination first page id',
  );
  discoveryPageTwoCapture = await storefrontRequest(
    'storefront-discovery-page-two',
    'StorefrontDiscoveryPagination',
    documentText.pagination,
    { query: indexedPrefixQuery, after: endCursor },
  );
  assertNoTopLevelErrors(discoveryPageTwoCapture.response.body, 'discovery page two');
  const secondPageId = readPath(discoveryPageTwoCapture.response.body, ['data', 'pagination', 'nodes', 0, 'id']);
  if (
    typeof secondPageId !== 'string' ||
    ![pageId, pageTwoId].includes(secondPageId) ||
    secondPageId === firstPageId ||
    readPath(discoveryPageTwoCapture.response.body, ['data', 'pagination', 'pageInfo', 'hasPreviousPage']) !== true
  ) {
    throw new Error(
      `Storefront discovery page two did not continue with the second disposable Page: ${JSON.stringify(
        discoveryPageTwoCapture,
        null,
        2,
      )}`,
    );
  }

  invalidIdCapture = await storefrontRequest(
    'storefront-discovery-invalid-id',
    'StorefrontDiscoveryInvalidId',
    documentText.invalidId,
    {},
  );
  const invalidIdMessage = readPath(invalidIdCapture.response.body, ['errors', 0, 'message']);
  if (invalidIdMessage !== "Invalid global id 'not-a-gid'") {
    throw new Error(`Invalid-ID capture returned an unexpected response: ${JSON.stringify(invalidIdCapture, null, 2)}`);
  }
  invalidLimitCapture = await storefrontRequest(
    'storefront-discovery-invalid-limit',
    'StorefrontDiscoveryInvalidLimit',
    documentText.invalidLimit,
    {},
  );
  const invalidLimitMessage = readPath(invalidLimitCapture.response.body, ['errors', 0, 'message']);
  if (invalidLimitMessage !== 'limit must be within 1..10') {
    throw new Error(
      `Invalid-limit capture returned an unexpected response: ${JSON.stringify(invalidLimitCapture, null, 2)}`,
    );
  }

  unrelatedSetupCapture = await captureAdmin('admin-unrelated-node-setup', documentText.unrelatedSetup, {
    ...unrelatedSetupVariables,
  });
  assertNoTopLevelErrors(unrelatedSetupCapture.response, 'unrelated Node setup');
  for (const root of ['productCreate', 'collectionCreate', 'pageCreate']) {
    assertNoUserErrors(unrelatedSetupCapture.response, ['data', root, 'userErrors'], `unrelated ${root}`);
  }
  unrelatedProductId = requiredString(
    unrelatedSetupCapture.response,
    ['data', 'productCreate', 'product', 'id'],
    'unrelated product id',
  );
  unrelatedCollectionId = requiredString(
    unrelatedSetupCapture.response,
    ['data', 'collectionCreate', 'collection', 'id'],
    'unrelated collection id',
  );
  unrelatedPageId = requiredString(
    unrelatedSetupCapture.response,
    ['data', 'pageCreate', 'page', 'id'],
    'unrelated page id',
  );
  unrelatedPublishCapture = await captureAdmin('admin-unrelated-node-publish', documentText.publish, {
    productId: unrelatedProductId,
    collectionId: unrelatedCollectionId,
    input: [{ publicationId }],
    publicationId,
  });
  assertNoTopLevelErrors(unrelatedPublishCapture.response, 'unrelated Node publish');
  assertNoUserErrors(unrelatedPublishCapture.response, ['data', 'product', 'userErrors'], 'unrelated product publish');
  assertNoUserErrors(
    unrelatedPublishCapture.response,
    ['data', 'collection', 'userErrors'],
    'unrelated collection publish',
  );
  menuHydrateCapture = await storefrontRequest(
    'storefront-node-menu-hydrate',
    'StorefrontMenuHydrate',
    documentText.menuHydrate,
    { handle: 'main-menu' },
  );
  assertNoTopLevelErrors(menuHydrateCapture.response.body, 'Storefront Node menu hydrate');
  const menuId = requiredString(menuHydrateCapture.response.body, ['data', 'menu', 'id'], 'main menu id');
  const missingPageId = 'gid://shopify/Page/999999999999999999';
  const liveMixedIds = [
    productId,
    unrelatedProductId,
    unrelatedCollectionId,
    unrelatedPageId,
    menuId,
    missingPageId,
    unrelatedPageId,
  ];
  nodeOverlayExpectedCapture = await waitForNodeOverlay({ menuId, ids: liveMixedIds }, menuId, [
    productId,
    unrelatedProductId,
    unrelatedCollectionId,
    unrelatedPageId,
    menuId,
    null,
    unrelatedPageId,
  ]);
  const syntheticProductId = 'gid://shopify/Product/1?shopify-draft-proxy=synthetic';
  const cassetteMixedIds = [syntheticProductId, ...liveMixedIds.slice(1)];
  nodeOverlayUpstreamCapture = await storefrontRequest(
    'storefront-node-overlay-upstream',
    'StorefrontNodeOverlayRead',
    documentText.nodeOverlayRead,
    { menuId, ids: cassetteMixedIds },
  );
  assertNoTopLevelErrors(nodeOverlayUpstreamCapture.response.body, 'Storefront Node overlay upstream');
  if (
    !nodeOverlayMatches(nodeOverlayUpstreamCapture.response.body, menuId, [
      null,
      unrelatedProductId,
      unrelatedCollectionId,
      unrelatedPageId,
      menuId,
      null,
      unrelatedPageId,
    ])
  ) {
    throw new Error(
      `Storefront Node upstream capture did not preserve unrelated nodes and synthetic miss: ${JSON.stringify(
        nodeOverlayUpstreamCapture,
        null,
        2,
      )}`,
    );
  }
} finally {
  if (articleId !== null) await captureAdminCleanup('article-cleanup', articleDeleteMutation, { id: articleId });
  if (pageTwoId !== null) await captureAdminCleanup('second-page-cleanup', pageDeleteMutation, { id: pageTwoId });
  if (unrelatedPageId !== null) {
    await captureAdminCleanup('unrelated-page-cleanup', pageDeleteMutation, { id: unrelatedPageId });
  }
  if (pageId !== null) await captureAdminCleanup('page-cleanup', pageDeleteMutation, { id: pageId });
  if (blogId !== null) await captureAdminCleanup('blog-cleanup', blogDeleteMutation, { id: blogId });
  if (unrelatedCollectionId !== null) {
    await captureAdminCleanup('unrelated-collection-cleanup', collectionDeleteMutation, {
      input: { id: unrelatedCollectionId },
    });
  }
  if (collectionId !== null) {
    await captureAdminCleanup('collection-cleanup', collectionDeleteMutation, { input: { id: collectionId } });
  }
  if (unrelatedProductId !== null) {
    await captureAdminCleanup('unrelated-product-cleanup', productDeleteMutation, {
      input: { id: unrelatedProductId },
    });
  }
  if (productId !== null) {
    await captureAdminCleanup('product-cleanup', productDeleteMutation, { input: { id: productId } });
  }
}

if (
  setupCapture === null ||
  articleSetupCapture === null ||
  publicationHydrateCapture === null ||
  publishCapture === null ||
  discoveryCapture === null ||
  pageTwoSetupCapture === null ||
  discoveryPaginationCapture === null ||
  discoveryPageTwoCapture === null ||
  invalidIdCapture === null ||
  invalidLimitCapture === null ||
  unrelatedSetupCapture === null ||
  unrelatedPublishCapture === null ||
  menuHydrateCapture === null ||
  nodeOverlayExpectedCapture === null ||
  nodeOverlayUpstreamCapture === null
) {
  throw new Error('Storefront discovery capture did not complete.');
}

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'storefront');
await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'storefront-discovery.json');
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'storefront-discovery',
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      apiSurface: 'storefront',
      endpoint: storefrontEndpoint,
      authMode: 'storefront-access-token',
      storefrontToken: {
        id: storedStorefrontAuth.storefront_token_id || '<unknown>',
        title: storedStorefrontAuth.storefront_token_title || '<unknown>',
        accessScopes: storedStorefrontAuth.storefront_access_scopes,
        obtainedAt: storedStorefrontAuth.obtained_at || '<unknown>',
      },
      adminSetup: setupCapture,
      adminArticleSetup: articleSetupCapture,
      adminPublicationHydrate: publicationHydrateCapture,
      adminPublish: publishCapture,
      adminUnrelatedNodeSetup: unrelatedSetupCapture,
      adminUnrelatedNodePublish: unrelatedPublishCapture,
      storefrontNodeMenuHydrate: menuHydrateCapture,
      storefrontNodeOverlayExpected: nodeOverlayExpectedCapture,
      storefrontNodeOverlayUpstream: nodeOverlayUpstreamCapture,
      storefrontDiscovery: discoveryCapture,
      adminPageTwoSetup: pageTwoSetupCapture,
      storefrontDiscoveryPagination: discoveryPaginationCapture,
      storefrontDiscoveryPageTwo: discoveryPageTwoCapture,
      storefrontInvalidId: invalidIdCapture,
      storefrontInvalidLimit: invalidLimitCapture,
      cleanup: cleanupCaptures,
      upstreamCalls: [publicationHydrateCapture, nodeOverlayUpstreamCapture],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
console.log(`Captured Storefront discovery status ${discoveryCapture.response.status}`);
