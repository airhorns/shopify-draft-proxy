/* oxlint-disable no-console -- CLI recorder intentionally writes capture status to stdout. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CaptureEntry = {
  variables: JsonRecord;
  response: {
    status: number;
    payload: unknown;
  };
};

const scenarioId = 'product-nested-connections-live-parity';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const requestDir = path.join('config', 'parity-requests', 'products');
const specDir = path.join('config', 'parity-specs', 'products');
const fixturePath = path.join(outputDir, 'product-nested-connections.json');
const specPath = path.join(specDir, `${scenarioId}.json`);

const requestPaths = {
  productCreate: path.join(requestDir, 'product-nested-connections-product-create.graphql'),
  variantBulkCreate: path.join(requestDir, 'product-nested-connections-variant-bulk-create.graphql'),
  collectionCreate: path.join(requestDir, 'product-nested-connections-collection-create.graphql'),
  collectionAddProductsV2: path.join(requestDir, 'product-nested-connections-collection-add-products-v2.graphql'),
  productCreateMedia: path.join(requestDir, 'product-nested-connections-product-create-media.graphql'),
  productUpdateMedia: path.join(requestDir, 'product-nested-connections-product-update-media.graphql'),
  variantAppendMedia: path.join(requestDir, 'product-nested-connections-variant-append-media.graphql'),
  variantSortRead: path.join(requestDir, 'product-nested-connections-variant-sort-read.graphql'),
  collectionSortRead: path.join(requestDir, 'product-nested-connections-collection-sort-read.graphql'),
  productMediaFirst: path.join(requestDir, 'product-nested-connections-product-media-first.graphql'),
  productMediaAfter: path.join(requestDir, 'product-nested-connections-product-media-after.graphql'),
  variantMediaRead: path.join(requestDir, 'product-nested-connections-variant-media-read.graphql'),
};

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productCreateDocument = `#graphql
mutation ProductNestedConnectionsCreateProduct($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      handle
      status
      variants(first: 5) {
        nodes {
          id
          title
          sku
          position
          selectedOptions {
            name
            value
          }
        }
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const variantBulkCreateDocument = `#graphql
mutation ProductNestedConnectionsVariantBulkCreate(
  $productId: ID!
  $variants: [ProductVariantsBulkInput!]!
  $strategy: ProductVariantsBulkCreateStrategy
) {
  productVariantsBulkCreate(productId: $productId, variants: $variants, strategy: $strategy) {
    productVariants {
      id
      title
      sku
      position
      inventoryPolicy
      inventoryQuantity
      inventoryItem {
        id
        tracked
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const collectionCreateDocument = `#graphql
mutation ProductNestedConnectionsCollectionCreate($input: CollectionInput!) {
  collectionCreate(input: $input) {
    collection {
      id
      title
      handle
    }
    userErrors {
      field
      message
    }
  }
}
`;

const collectionAddProductsV2Document = `#graphql
mutation ProductNestedConnectionsCollectionAddProductsV2($id: ID!, $productIds: [ID!]!) {
  collectionAddProductsV2(id: $id, productIds: $productIds) {
    job {
      id
      done
    }
    userErrors {
      field
      message
    }
  }
}
`;

const productCreateMediaDocument = `#graphql
mutation ProductNestedConnectionsProductCreateMedia($productId: ID!, $media: [CreateMediaInput!]!) {
  productCreateMedia(productId: $productId, media: $media) {
    media {
      id
      alt
      mediaContentType
      status
    }
    mediaUserErrors {
      field
      message
    }
  }
}
`;

const productUpdateMediaDocument = `#graphql
mutation ProductNestedConnectionsProductUpdateMedia($productId: ID!, $media: [UpdateMediaInput!]!) {
  productUpdateMedia(productId: $productId, media: $media) {
    media {
      id
      alt
      mediaContentType
      status
    }
    mediaUserErrors {
      field
      message
    }
  }
}
`;

const variantAppendMediaDocument = `#graphql
mutation ProductNestedConnectionsVariantAppendMedia(
  $productId: ID!
  $variantMedia: [ProductVariantAppendMediaInput!]!
) {
  productVariantAppendMedia(productId: $productId, variantMedia: $variantMedia) {
    productVariants {
      id
      media(first: 5) {
        nodes {
          alt
          mediaContentType
          status
        }
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const variantSortReadDocument = `#graphql
query ProductNestedConnectionsVariantSortRead($productId: ID!) {
  product(id: $productId) {
    byFullTitle: variants(first: 10, sortKey: FULL_TITLE) {
      nodes { sku title }
    }
    byId: variants(first: 10, sortKey: ID) {
      nodes { sku title }
    }
    byInventoryLevelsAvailable: variants(first: 10, sortKey: INVENTORY_LEVELS_AVAILABLE) {
      nodes { sku inventoryQuantity }
    }
    byInventoryManagement: variants(first: 10, sortKey: INVENTORY_MANAGEMENT) {
      nodes { sku inventoryItem { tracked } }
    }
    byInventoryPolicyReverse: variants(first: 10, sortKey: INVENTORY_POLICY, reverse: true) {
      nodes { sku inventoryPolicy }
    }
    byInventoryQuantityReverse: variants(first: 10, sortKey: INVENTORY_QUANTITY, reverse: true) {
      nodes { sku inventoryQuantity }
    }
    byNameReverseWindow: variants(first: 2, sortKey: NAME, reverse: true) {
      nodes { sku title }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    byPopular: variants(first: 10, sortKey: POPULAR) {
      nodes { sku position }
    }
    byPositionReverse: variants(first: 10, sortKey: POSITION, reverse: true) {
      nodes { sku position }
    }
    byRelevanceReverse: variants(first: 10, sortKey: RELEVANCE, reverse: true) {
      nodes { sku position }
    }
    bySku: variants(first: 10, sortKey: SKU) {
      nodes { sku title }
    }
    byTitle: variants(first: 10, sortKey: TITLE) {
      nodes { sku title }
    }
  }
}
`;

const collectionSortReadDocument = `#graphql
query ProductNestedConnectionsCollectionSortRead($productId: ID!) {
  product(id: $productId) {
    byId: collections(first: 10, sortKey: ID) {
      nodes { title handle }
    }
    byTitleReverseWindow: collections(first: 2, sortKey: TITLE, reverse: true) {
      edges {
        cursor
        node { title handle }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    byDefaultReverse: collections(first: 3, reverse: true) {
      nodes { title handle }
    }
  }
}
`;

const productMediaFirstDocument = `#graphql
query ProductNestedConnectionsProductMediaFirst($productId: ID!) {
  product(id: $productId) {
    media(first: 2, sortKey: POSITION) {
      edges {
        cursor
        node { alt mediaContentType status }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
}
`;

const productMediaAfterDocument = `#graphql
query ProductNestedConnectionsProductMediaAfter($productId: ID!, $after: String!) {
  product(id: $productId) {
    media(first: 1, after: $after, sortKey: POSITION) {
      edges {
        cursor
        node { alt mediaContentType status }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
}
`;

const variantMediaReadDocument = `#graphql
query ProductNestedConnectionsVariantMediaRead($variantId: ID!) {
  productVariant(id: $variantId) {
    media(first: 1) {
      edges {
        cursor
        node { alt mediaContentType status }
      }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
}
`;

const productDeleteDocument = `#graphql
mutation ProductNestedConnectionsDeleteProduct($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors { field message }
  }
}
`;

const collectionDeleteDocument = `#graphql
mutation ProductNestedConnectionsDeleteCollection($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) {
    deletedCollectionId
    userErrors { field message }
  }
}
`;

const productMediaReadyReadDocument = `#graphql
query ProductNestedConnectionsProductMediaReadyRead($productId: ID!) {
  product(id: $productId) {
    media(first: 10, sortKey: POSITION) {
      nodes { id alt mediaContentType status }
    }
  }
}
`;

async function capture(query: string, variables: JsonRecord = {}): Promise<CaptureEntry> {
  const result: ConformanceGraphqlResult = await runGraphqlRequest(query, variables);
  return {
    variables,
    response: {
      status: result.status,
      payload: result.payload,
    },
  };
}

function readRecord(value: unknown, label: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function readArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} was not an array: ${JSON.stringify(value)}`);
  }
  return value;
}

function readPath(value: unknown, parts: string[]): unknown {
  let cursor = value;
  for (const part of parts) {
    if (Array.isArray(cursor)) {
      cursor = cursor[Number(part)];
    } else {
      cursor = readRecord(cursor, parts.join('.'))[part];
    }
  }
  return cursor;
}

function readStringPath(value: unknown, parts: string[], label: string): string {
  const found = readPath(value, parts);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`${label} did not include a string at ${parts.join('.')}: ${JSON.stringify(value)}`);
  }
  return found;
}

function assertNoTopLevelErrors(entry: CaptureEntry, label: string): void {
  const payload = readRecord(entry.response.payload, `${label}.payload`);
  if (entry.response.status < 200 || entry.response.status >= 300 || payload['errors'] !== undefined) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(entry.response.payload, null, 2)}`);
  }
}

function assertNoUserErrors(entry: CaptureEntry, root: string, label: string): void {
  assertNoTopLevelErrors(entry, label);
  const userErrors = readPath(entry.response.payload, ['data', root, 'userErrors']);
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function assertNoMediaUserErrors(entry: CaptureEntry, root: string, label: string): void {
  assertNoTopLevelErrors(entry, label);
  const userErrors = readPath(entry.response.payload, ['data', root, 'mediaUserErrors']);
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned mediaUserErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function mediaNodes(entry: CaptureEntry): JsonRecord[] {
  return readArray(readPath(entry.response.payload, ['data', 'product', 'media', 'nodes']), 'product.media.nodes').map(
    (node, index) => readRecord(node, `product.media.nodes[${index}]`),
  );
}

async function waitForReadyMedia(productId: string, expectedCount: number): Promise<CaptureEntry> {
  let latest: CaptureEntry | null = null;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    if (attempt > 0) await new Promise((resolve) => setTimeout(resolve, 5000));
    latest = await capture(productMediaReadyReadDocument, { productId });
    assertNoTopLevelErrors(latest, 'product media ready read');
    const nodes = mediaNodes(latest);
    if (nodes.length >= expectedCount && nodes.slice(0, expectedCount).every((node) => node['status'] === 'READY')) {
      return latest;
    }
  }
  throw new Error(`Timed out waiting for ${expectedCount} ready product media nodes.`);
}

function connectionCount(entry: CaptureEntry, pathParts: string[]): number {
  return readArray(readPath(entry.response.payload, pathParts), pathParts.join('.')).length;
}

async function waitForProductCollections(productId: string, expectedCount: number): Promise<CaptureEntry> {
  let latest: CaptureEntry | null = null;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    if (attempt > 0) await new Promise((resolve) => setTimeout(resolve, 5000));
    latest = await capture(collectionSortReadDocument, { productId });
    assertNoTopLevelErrors(latest, 'product collection sort read');
    if (connectionCount(latest, ['data', 'product', 'byId', 'nodes']) === expectedCount) {
      return latest;
    }
  }
  throw new Error(`Timed out waiting for ${expectedCount} product collection memberships.`);
}

function anyStringDifference(path: string, reason: string): JsonRecord {
  return { path, matcher: 'non-empty-string', reason };
}

function spec(): JsonRecord {
  return {
    scenarioId,
    operationNames: [
      'productCreate',
      'productVariantsBulkCreate',
      'collectionCreate',
      'collectionAddProductsV2',
      'productCreateMedia',
      'productUpdateMedia',
      'productVariantAppendMedia',
    ],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'downstream-read-parity'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/products_saved_searches.rs'],
    proxyRequest: {
      documentPath: requestPaths.productCreate,
      apiVersion,
      variablesCapturePath: '$.setup.productCreate.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'setup-product-create-user-errors',
          capturePath: '$.setup.productCreate.response.payload.data.productCreate.userErrors',
          proxyPath: '$.data.productCreate.userErrors',
        },
        {
          name: 'setup-variant-bulk-create',
          capturePath: '$.setup.productVariantsBulkCreate.response.payload.data.productVariantsBulkCreate.userErrors',
          proxyPath: '$.data.productVariantsBulkCreate.userErrors',
          proxyRequest: {
            documentPath: requestPaths.variantBulkCreate,
            apiVersion,
            variables: {
              productId: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
              variants: { fromCapturePath: '$.setup.productVariantsBulkCreate.variables.variants' },
              strategy: { fromCapturePath: '$.setup.productVariantsBulkCreate.variables.strategy' },
            },
          },
        },
        ...['zulu', 'alpha', 'middle'].map((key) => ({
          name: `setup-collection-create-${key}`,
          capturePath: `$.setup.collectionCreate.${key}.response.payload.data.collectionCreate.userErrors`,
          proxyPath: '$.data.collectionCreate.userErrors',
          proxyRequest: {
            documentPath: requestPaths.collectionCreate,
            apiVersion,
            variablesCapturePath: `$.setup.collectionCreate.${key}.variables`,
          },
        })),
        ...['zulu', 'alpha', 'middle'].map((key) => ({
          name: `setup-collection-add-products-v2-${key}`,
          capturePath: `$.setup.collectionAddProductsV2.${key}.response.payload.data.collectionAddProductsV2.userErrors`,
          proxyPath: '$.data.collectionAddProductsV2.userErrors',
          proxyRequest: {
            documentPath: requestPaths.collectionAddProductsV2,
            apiVersion,
            variables: {
              id: {
                fromProxyResponse: `setup-collection-create-${key}`,
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromPrimaryProxyPath: '$.data.productCreate.product.id' }],
            },
          },
        })),
        {
          name: 'setup-product-media-create',
          capturePath: '$.setup.productCreateMedia.response.payload.data.productCreateMedia.mediaUserErrors',
          proxyPath: '$.data.productCreateMedia.mediaUserErrors',
          proxyRequest: {
            documentPath: requestPaths.productCreateMedia,
            apiVersion,
            variables: {
              productId: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
              media: { fromCapturePath: '$.setup.productCreateMedia.variables.media' },
            },
          },
        },
        {
          name: 'setup-product-media-update-ready',
          capturePath: '$.setup.productUpdateMedia.response.payload.data.productUpdateMedia.mediaUserErrors',
          proxyPath: '$.data.productUpdateMedia.mediaUserErrors',
          proxyRequest: {
            documentPath: requestPaths.productUpdateMedia,
            apiVersion,
            variables: {
              productId: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
              media: [
                {
                  id: {
                    fromProxyResponse: 'setup-product-media-create',
                    path: '$.data.productCreateMedia.media[0].id',
                  },
                },
                {
                  id: {
                    fromProxyResponse: 'setup-product-media-create',
                    path: '$.data.productCreateMedia.media[1].id',
                  },
                },
                {
                  id: {
                    fromProxyResponse: 'setup-product-media-create',
                    path: '$.data.productCreateMedia.media[2].id',
                  },
                },
              ],
            },
          },
        },
        {
          name: 'setup-variant-media-append',
          capturePath: '$.setup.productVariantAppendMedia.response.payload.data.productVariantAppendMedia.userErrors',
          proxyPath: '$.data.productVariantAppendMedia.userErrors',
          proxyRequest: {
            documentPath: requestPaths.variantAppendMedia,
            apiVersion,
            variables: {
              productId: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
              variantMedia: [
                {
                  variantId: {
                    fromProxyResponse: 'setup-variant-bulk-create',
                    path: '$.data.productVariantsBulkCreate.productVariants[1].id',
                  },
                  mediaIds: [
                    {
                      fromProxyResponse: 'setup-product-media-update-ready',
                      path: '$.data.productUpdateMedia.media[1].id',
                    },
                  ],
                },
              ],
            },
          },
        },
        {
          name: 'variant-sort-read',
          capturePath: '$.reads.variantSorts.response.payload.data.product',
          proxyPath: '$.data.product',
          proxyRequest: {
            documentPath: requestPaths.variantSortRead,
            apiVersion,
            variables: {
              productId: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
            },
          },
          expectedDifferences: [
            anyStringDifference(
              '$.byNameReverseWindow.pageInfo.startCursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
            anyStringDifference(
              '$.byNameReverseWindow.pageInfo.endCursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
          ],
        },
        {
          name: 'collection-sort-read',
          capturePath: '$.reads.collectionSorts.response.payload.data.product',
          proxyPath: '$.data.product',
          proxyRequest: {
            documentPath: requestPaths.collectionSortRead,
            apiVersion,
            variables: {
              productId: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
            },
          },
          expectedDifferences: [
            anyStringDifference(
              '$.byTitleReverseWindow.edges[*].cursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
            anyStringDifference(
              '$.byTitleReverseWindow.pageInfo.startCursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
            anyStringDifference(
              '$.byTitleReverseWindow.pageInfo.endCursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
          ],
        },
        {
          name: 'product-media-first-read',
          capturePath: '$.reads.productMediaFirst.response.payload.data.product',
          proxyPath: '$.data.product',
          proxyRequest: {
            documentPath: requestPaths.productMediaFirst,
            apiVersion,
            variables: {
              productId: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
            },
          },
          expectedDifferences: [
            anyStringDifference(
              '$.media.edges[*].cursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
            anyStringDifference(
              '$.media.pageInfo.startCursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
            anyStringDifference(
              '$.media.pageInfo.endCursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
          ],
        },
        {
          name: 'product-media-after-read',
          capturePath: '$.reads.productMediaAfter.response.payload.data.product',
          proxyPath: '$.data.product',
          proxyRequest: {
            documentPath: requestPaths.productMediaAfter,
            apiVersion,
            variables: {
              productId: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
              after: {
                fromProxyResponse: 'product-media-first-read',
                path: '$.data.product.media.edges[0].cursor',
              },
            },
          },
          expectedDifferences: [
            anyStringDifference(
              '$.media.edges[*].cursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
            anyStringDifference(
              '$.media.pageInfo.startCursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
            anyStringDifference(
              '$.media.pageInfo.endCursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
          ],
        },
        {
          name: 'variant-media-read',
          capturePath: '$.reads.variantMedia.response.payload.data.productVariant',
          proxyPath: '$.data.productVariant',
          proxyRequest: {
            documentPath: requestPaths.variantMediaRead,
            apiVersion,
            variables: {
              variantId: {
                fromProxyResponse: 'setup-variant-bulk-create',
                path: '$.data.productVariantsBulkCreate.productVariants[1].id',
              },
            },
          },
          expectedDifferences: [
            anyStringDifference(
              '$.media.edges[*].cursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
            anyStringDifference(
              '$.media.pageInfo.startCursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
            anyStringDifference(
              '$.media.pageInfo.endCursor',
              'Shopify uses opaque connection cursors while the proxy uses local stable cursors.',
            ),
          ],
        },
      ],
    },
    notes:
      'Captured Shopify nested Product connection mechanics for product.variants sortKey/reverse, product.collections sortKey/reverse/windowing, product.media cursor windows, and ProductVariant.media populated edges/pageInfo. Setup replays only public Admin GraphQL mutations; no proxy state seed is used.',
  };
}

async function writeArtifacts(fixture: JsonRecord): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  await mkdir(requestDir, { recursive: true });
  await mkdir(specDir, { recursive: true });
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await writeFile(requestPaths.productCreate, productCreateDocument, 'utf8');
  await writeFile(requestPaths.variantBulkCreate, variantBulkCreateDocument, 'utf8');
  await writeFile(requestPaths.collectionCreate, collectionCreateDocument, 'utf8');
  await writeFile(requestPaths.collectionAddProductsV2, collectionAddProductsV2Document, 'utf8');
  await writeFile(requestPaths.productCreateMedia, productCreateMediaDocument, 'utf8');
  await writeFile(requestPaths.productUpdateMedia, productUpdateMediaDocument, 'utf8');
  await writeFile(requestPaths.variantAppendMedia, variantAppendMediaDocument, 'utf8');
  await writeFile(requestPaths.variantSortRead, variantSortReadDocument, 'utf8');
  await writeFile(requestPaths.collectionSortRead, collectionSortReadDocument, 'utf8');
  await writeFile(requestPaths.productMediaFirst, productMediaFirstDocument, 'utf8');
  await writeFile(requestPaths.productMediaAfter, productMediaAfterDocument, 'utf8');
  await writeFile(requestPaths.variantMediaRead, variantMediaReadDocument, 'utf8');
  await writeFile(specPath, `${JSON.stringify(spec(), null, 2)}\n`, 'utf8');
}

const runId = Date.now().toString(36);
const productIds: string[] = [];
const collectionIds: string[] = [];
let fixturePayload: JsonRecord | null = null;

try {
  const productCreate = await capture(productCreateDocument, {
    product: {
      title: `Nested connection conformance ${runId}`,
      status: 'DRAFT',
    },
  });
  assertNoUserErrors(productCreate, 'productCreate', 'productCreate setup');
  const productId = readStringPath(
    productCreate.response.payload,
    ['data', 'productCreate', 'product', 'id'],
    'product',
  );
  productIds.push(productId);

  const productVariantsBulkCreate = await capture(variantBulkCreateDocument, {
    productId,
    strategy: 'REMOVE_STANDALONE_VARIANT',
    variants: [
      {
        optionValues: [{ optionName: 'Title', name: 'Zulu' }],
        price: '10.00',
        inventoryPolicy: 'CONTINUE',
        inventoryItem: { sku: `NESTED-${runId}-C`, tracked: true },
      },
      {
        optionValues: [{ optionName: 'Title', name: 'Alpha' }],
        price: '11.00',
        inventoryPolicy: 'DENY',
        inventoryItem: { sku: `NESTED-${runId}-A`, tracked: false },
      },
      {
        optionValues: [{ optionName: 'Title', name: 'Middle' }],
        price: '12.00',
        inventoryPolicy: 'CONTINUE',
        inventoryItem: { sku: `NESTED-${runId}-B`, tracked: true },
      },
    ],
  });
  assertNoUserErrors(productVariantsBulkCreate, 'productVariantsBulkCreate', 'productVariantsBulkCreate setup');
  const variantId = readStringPath(
    productVariantsBulkCreate.response.payload,
    ['data', 'productVariantsBulkCreate', 'productVariants', '1', 'id'],
    'variant',
  );

  const collectionInputs = {
    zulu: { input: { title: `Zulu nested collection ${runId}` } },
    alpha: { input: { title: `Alpha nested collection ${runId}` } },
    middle: { input: { title: `Middle nested collection ${runId}` } },
  };
  const collectionCreate: Record<string, CaptureEntry> = {};
  const collectionAddProductsV2: Record<string, CaptureEntry> = {};
  for (const [key, variables] of Object.entries(collectionInputs)) {
    const created = await capture(collectionCreateDocument, variables);
    assertNoUserErrors(created, 'collectionCreate', `collectionCreate ${key}`);
    collectionCreate[key] = created;
    const collectionId = readStringPath(
      created.response.payload,
      ['data', 'collectionCreate', 'collection', 'id'],
      key,
    );
    collectionIds.push(collectionId);
    const add = await capture(collectionAddProductsV2Document, { id: collectionId, productIds: [productId] });
    assertNoUserErrors(add, 'collectionAddProductsV2', `collectionAddProductsV2 ${key}`);
    collectionAddProductsV2[key] = add;
  }
  const collectionSorts = await waitForProductCollections(productId, 3);

  const productCreateMedia = await capture(productCreateMediaDocument, {
    productId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: `https://placehold.co/640x480/png?text=nested-front-${runId}`,
        alt: `Nested front ${runId}`,
      },
      {
        mediaContentType: 'IMAGE',
        originalSource: `https://placehold.co/640x480/png?text=nested-side-${runId}`,
        alt: `Nested side ${runId}`,
      },
      {
        mediaContentType: 'IMAGE',
        originalSource: `https://placehold.co/640x480/png?text=nested-back-${runId}`,
        alt: `Nested back ${runId}`,
      },
    ],
  });
  assertNoMediaUserErrors(productCreateMedia, 'productCreateMedia', 'productCreateMedia setup');
  await waitForReadyMedia(productId, 3);
  const mediaIds = readArray(
    readPath(productCreateMedia.response.payload, ['data', 'productCreateMedia', 'media']),
    'productCreateMedia.media',
  ).map((media, index) => readStringPath(media, ['id'], `media ${index}`));
  const productUpdateMedia = await capture(productUpdateMediaDocument, {
    productId,
    media: mediaIds.map((id) => ({ id })),
  });
  assertNoMediaUserErrors(productUpdateMedia, 'productUpdateMedia', 'productUpdateMedia ready setup');

  const productVariantAppendMedia = await capture(variantAppendMediaDocument, {
    productId,
    variantMedia: [{ variantId, mediaIds: [mediaIds[1]] }],
  });
  assertNoUserErrors(productVariantAppendMedia, 'productVariantAppendMedia', 'productVariantAppendMedia setup');

  const variantSorts = await capture(variantSortReadDocument, { productId });
  assertNoTopLevelErrors(variantSorts, 'variant sort read');
  const productMediaFirst = await capture(productMediaFirstDocument, { productId });
  assertNoTopLevelErrors(productMediaFirst, 'product media first read');
  const firstCursor = readStringPath(
    productMediaFirst.response.payload,
    ['data', 'product', 'media', 'edges', '0', 'cursor'],
    'product media first cursor',
  );
  const productMediaAfter = await capture(productMediaAfterDocument, { productId, after: firstCursor });
  assertNoTopLevelErrors(productMediaAfter, 'product media after read');
  const variantMedia = await capture(variantMediaReadDocument, { variantId });
  assertNoTopLevelErrors(variantMedia, 'variant media read');

  fixturePayload = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes: [
      'Creates one disposable product, variants, collections, and product media through public Admin GraphQL mutations.',
      'Reads product.variants, product.collections, product.media, and productVariant.media nested connections with sort/reverse/window arguments.',
    ],
    setup: {
      productCreate,
      productVariantsBulkCreate,
      collectionCreate,
      collectionAddProductsV2,
      productCreateMedia,
      productUpdateMedia,
      productVariantAppendMedia,
    },
    reads: {
      variantSorts,
      collectionSorts,
      productMediaFirst,
      productMediaAfter,
      variantMedia,
    },
    upstreamCalls: [],
  };
} finally {
  const cleanup: JsonRecord = {};
  for (const collectionId of collectionIds.reverse()) {
    cleanup[`collection:${collectionId}`] = await capture(collectionDeleteDocument, { input: { id: collectionId } });
  }
  for (const productId of productIds.reverse()) {
    cleanup[`product:${productId}`] = await capture(productDeleteDocument, { input: { id: productId } });
  }
  if (fixturePayload) fixturePayload['cleanup'] = cleanup;
}

if (!fixturePayload) {
  throw new Error('Product nested connections capture did not produce a fixture payload.');
}

await writeArtifacts(fixturePayload);

console.log(
  JSON.stringify(
    {
      ok: true,
      fixtureFiles: [fixturePath],
      specFiles: [specPath],
      requestFiles: Object.values(requestPaths),
    },
    null,
    2,
  ),
);
