import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productsDir = path.join('config', 'parity-requests', 'products');
const specsDir = path.join('config', 'parity-specs', 'products');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');

const hydrateDocumentPath = path.join(productsDir, 'products-hydrate-nodes-observation.graphql');
const productCreateDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-product-create.graphql');
const productPriceUpdateDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-price-update.graphql');
const createDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-create.graphql');
const addDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-add.graphql');
const removeDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-remove.graphql');
const readDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-count-read.graphql');
const windowReadDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-window-read.graphql');
const afterReadDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-after-read.graphql');
const sortReadDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-sort-read.graphql');
const specPath = path.join(specsDir, 'collectionCreate-and-add-products-parity.json');
const fixturePath = path.join(fixtureDir, 'collection-create-and-add-products-parity.json');
const productsHydrateNodesObservationDocument = await readFile(hydrateDocumentPath, 'utf8');

const seedProductsQuery = `#graphql
query CollectionMembershipSeedProduct {
  products(first: 1, sortKey: UPDATED_AT, reverse: true) {
    nodes {
      id
      title
      handle
      status
    }
  }
}
`;

const productCreateDocument = `mutation CollectionMembershipProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      handle
      createdAt
      variants(first: 1) {
        nodes {
          id
          price
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

const productPriceUpdateDocument = `mutation CollectionMembershipProductPriceUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
  productVariantsBulkUpdate(productId: $productId, variants: $variants) {
    product {
      id
    }
    productVariants {
      id
      price
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const createDocument = `mutation CollectionMembershipCreate($input: CollectionInput!) {
  collectionCreate(input: $input) {
    collection {
      id
      title
      handle
      sortOrder
      ruleSet {
        appliedDisjunctively
        rules {
          column
          relation
          condition
        }
      }
      productsCount {
        count
        precision
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const addDocument = `mutation CollectionMembershipAddProducts($id: ID!, $productIds: [ID!]!) {
  collectionAddProducts(id: $id, productIds: $productIds) {
    collection {
      id
      products(first: 5) {
        nodes {
          id
          title
          handle
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

const removeDocument = `mutation CollectionMembershipRemoveProducts($id: ID!, $productIds: [ID!]!) {
  collectionRemoveProducts(id: $id, productIds: $productIds) {
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

const readDocument = `query CollectionMembershipProductsCount($id: ID!) {
  collection(id: $id) {
    id
    productsCount {
      count
      precision
    }
    products(first: 5) {
      nodes {
        id
        title
        handle
      }
    }
  }
}
`;

const windowReadDocument = `query CollectionMembershipProductsWindowFirst($id: ID!, $first: Int!) {
  collection(id: $id) {
    id
    products(first: $first, sortKey: MANUAL) {
      edges {
        cursor
        node {
          id
          title
        }
      }
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
}
`;

const afterReadDocument = `query CollectionMembershipProductsWindowAfter($id: ID!, $first: Int!, $after: String!) {
  collection(id: $id) {
    id
    products(first: $first, after: $after, sortKey: MANUAL) {
      edges {
        cursor
        node {
          id
          title
        }
      }
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
}
`;

const sortReadDocument = `query CollectionMembershipProductsSortKeys($id: ID!) {
  collection(id: $id) {
    id
    collectionDefault: products(first: 5, sortKey: COLLECTION_DEFAULT) {
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    manual: products(first: 5, sortKey: MANUAL) {
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    bestSelling: products(first: 5, sortKey: BEST_SELLING) {
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    created: products(first: 5, sortKey: CREATED) {
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    idSort: products(first: 5, sortKey: ID) {
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    price: products(first: 5, sortKey: PRICE) {
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    relevance: products(first: 5, sortKey: RELEVANCE) {
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    titleReverse: products(first: 5, sortKey: TITLE, reverse: true) {
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
}
`;

const deleteDocument = `#graphql
mutation CollectionMembershipCleanup($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) {
    deletedCollectionId
    userErrors {
      field
      message
    }
  }
}
`;

const productDeleteDocument = `#graphql
mutation CollectionMembershipProductCleanup($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

type GraphqlPayload = Record<string, unknown>;

async function runGraphqlPayload(query: string, variables?: Record<string, unknown>): Promise<GraphqlPayload> {
  try {
    return await runGraphql(query, variables);
  } catch (error) {
    const payload = (error as { result?: { payload?: GraphqlPayload } }).result?.payload;
    if (payload) {
      return payload;
    }
    throw error;
  }
}

function firstSeedProduct(payload: GraphqlPayload) {
  const product = (
    ((payload['data'] as Record<string, unknown> | undefined)?.['products'] as Record<string, unknown> | undefined)?.[
      'nodes'
    ] as Array<Record<string, unknown>> | undefined
  )?.[0];
  if (
    typeof product?.['id'] !== 'string' ||
    typeof product['title'] !== 'string' ||
    typeof product['handle'] !== 'string' ||
    typeof product['status'] !== 'string'
  ) {
    throw new Error('Need at least one live product to capture collection membership parity.');
  }
  return {
    id: product['id'],
    title: product['title'],
    handle: product['handle'],
    status: product['status'],
  };
}

function collectionId(payload: GraphqlPayload): string | null {
  const id = (
    (
      (payload['data'] as Record<string, unknown> | undefined)?.['collectionCreate'] as
        | Record<string, unknown>
        | undefined
    )?.['collection'] as Record<string, unknown> | undefined
  )?.['id'];
  return typeof id === 'string' ? id : null;
}

function productId(payload: GraphqlPayload): string {
  const id = (
    (
      (payload['data'] as Record<string, unknown> | undefined)?.['productCreate'] as Record<string, unknown> | undefined
    )?.['product'] as Record<string, unknown> | undefined
  )?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`productCreate did not return a product id: ${JSON.stringify(payload)}`);
  }
  return id;
}

function productVariantId(payload: GraphqlPayload): string {
  const id = (
    (
      (
        (
          (payload['data'] as Record<string, unknown> | undefined)?.['productCreate'] as
            | Record<string, unknown>
            | undefined
        )?.['product'] as Record<string, unknown> | undefined
      )?.['variants'] as Record<string, unknown> | undefined
    )?.['nodes'] as Array<Record<string, unknown>> | undefined
  )?.[0]?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`productCreate did not return a default variant id: ${JSON.stringify(payload)}`);
  }
  return id;
}

function pageEndCursor(payload: GraphqlPayload): string {
  const cursor = (
    (
      (
        (payload['data'] as Record<string, unknown> | undefined)?.['collection'] as Record<string, unknown> | undefined
      )?.['products'] as Record<string, unknown> | undefined
    )?.['pageInfo'] as Record<string, unknown> | undefined
  )?.['endCursor'];
  if (typeof cursor !== 'string' || cursor.length === 0) {
    throw new Error(`First page read did not return a non-empty endCursor: ${JSON.stringify(payload)}`);
  }
  return cursor;
}

async function cleanupCollection(id: string | null) {
  if (!id) {
    return;
  }
  try {
    await runGraphql(deleteDocument, { input: { id } });
  } catch {
    // Best-effort cleanup only; the capture should still preserve the original failure.
  }
}

async function cleanupProduct(id: string | null) {
  if (!id) {
    return;
  }
  try {
    await runGraphql(productDeleteDocument, { input: { id } });
  } catch {
    // Best-effort cleanup only; the capture should still preserve the original failure.
  }
}

function productHydrationCassette(seedProduct: ReturnType<typeof firstSeedProduct>) {
  return {
    operationName: 'ProductsHydrateNodes',
    variables: {
      ids: [seedProduct.id],
    },
    query: productsHydrateNodesObservationDocument,
    response: {
      status: 200,
      body: {
        data: {
          nodes: [
            {
              id: seedProduct.id,
              title: seedProduct.title,
              handle: seedProduct.handle,
              status: seedProduct.status,
              collections: {
                nodes: [],
                pageInfo: {
                  hasNextPage: false,
                  hasPreviousPage: false,
                },
              },
              variants: {
                nodes: [],
              },
              options: [],
              media: {
                nodes: [],
              },
              metafields: {
                nodes: [],
              },
              sellingPlanGroups: {
                nodes: [],
              },
            },
          ],
        },
      },
    },
  };
}

function productIdDifferences(pathPrefix: string) {
  return [
    {
      path: `${pathPrefix}.nodes[*].id`,
      matcher: 'shopify-gid:Product',
      reason: 'Shopify and the local parity harness allocate product identifiers independently.',
    },
    {
      path: `${pathPrefix}.edges[*].node.id`,
      matcher: 'shopify-gid:Product',
      reason: 'Shopify and the local parity harness allocate product identifiers independently.',
    },
  ];
}

function cursorDifferences(pathPrefix: string) {
  return [
    {
      path: `${pathPrefix}.edges[*].cursor`,
      matcher: 'any-string',
      reason: 'Collection product connection cursors are opaque server tokens.',
    },
    {
      path: `${pathPrefix}.pageInfo.startCursor`,
      matcher: 'any-string',
      reason: 'Collection product connection cursors are opaque server tokens.',
    },
    {
      path: `${pathPrefix}.pageInfo.endCursor`,
      matcher: 'any-string',
      reason: 'Collection product connection cursors are opaque server tokens.',
    },
  ];
}

function sortReadProductIdDifferences() {
  const aliases = [
    'collectionDefault',
    'manual',
    'bestSelling',
    'created',
    'idSort',
    'price',
    'relevance',
    'titleReverse',
  ];
  return aliases.map((alias) => ({
    path: `$.collection.${alias}.nodes[*].id`,
    matcher: 'shopify-gid:Product',
    reason: 'Shopify and the local parity harness allocate product identifiers independently.',
  }));
}

await mkdir(productsDir, { recursive: true });
await mkdir(specsDir, { recursive: true });
await mkdir(fixtureDir, { recursive: true });

const runId = `${Date.now()}`;
const seedProduct = firstSeedProduct(await runGraphql(seedProductsQuery));
const createdIds: string[] = [];
const createdProductIds: string[] = [];

try {
  const longTitleVariables = { input: { title: 'T'.repeat(256) } };
  const reservedLikeVariables = { input: { title: `Frontpage collection membership ${runId}` } };
  const invalidSortVariables = {
    input: {
      title: `Collection membership invalid sort ${runId}`,
      sortOrder: 'INVALID_VALUE',
    },
  };
  const smartCreateVariables = {
    input: {
      title: `Collection membership Smart ${runId}`,
      ruleSet: {
        appliedDisjunctively: false,
        rules: [{ column: 'TITLE', relation: 'CONTAINS', condition: `No matching smart seed ${runId}` }],
      },
    },
  };
  const customCreateVariables = { input: { title: `Collection membership Custom ${runId}` } };
  const windowProductCases = [
    { key: 'zulu', title: `Collection window Zulu ${runId}`, price: '30.00' },
    { key: 'alpha', title: `Collection window Alpha ${runId}`, price: '10.00' },
    { key: 'mike', title: `Collection window Mike ${runId}`, price: '20.00' },
  ];

  const longTitle = await runGraphqlPayload(createDocument, longTitleVariables);
  const reservedLike = await runGraphqlPayload(createDocument, reservedLikeVariables);
  const reservedLikeId = collectionId(reservedLike);
  if (reservedLikeId) {
    createdIds.push(reservedLikeId);
  }
  const invalidSort = await runGraphqlPayload(createDocument, invalidSortVariables);

  const smartCreate = await runGraphqlPayload(createDocument, smartCreateVariables);
  const smartId = collectionId(smartCreate);
  if (!smartId) {
    throw new Error('Smart collection create did not return a collection id.');
  }
  createdIds.push(smartId);

  const smartAddVariables = { id: smartId, productIds: [seedProduct.id] };
  const smartAdd = await runGraphqlPayload(addDocument, smartAddVariables);
  const smartRemove = await runGraphqlPayload(removeDocument, smartAddVariables);

  const customCreate = await runGraphqlPayload(createDocument, customCreateVariables);
  const customId = collectionId(customCreate);
  if (!customId) {
    throw new Error('Custom collection create did not return a collection id.');
  }
  createdIds.push(customId);

  const customAddVariables = { id: customId, productIds: [seedProduct.id] };
  const customAdd = await runGraphqlPayload(addDocument, customAddVariables);
  const customDuplicateAddVariables = { id: customId, productIds: [seedProduct.id] };
  const customDuplicateAdd = await runGraphqlPayload(addDocument, customDuplicateAddVariables);
  const customReadVariables = { id: customId };
  const customRead = await runGraphqlPayload(readDocument, customReadVariables);

  const windowProducts: Record<
    string,
    {
      create: { variables: GraphqlPayload; response: GraphqlPayload };
      priceUpdate: { variables: GraphqlPayload; response: GraphqlPayload };
      productId: string;
      variantId: string;
    }
  > = {};
  for (const productCase of windowProductCases) {
    const createVariables = { product: { title: productCase.title, status: 'ACTIVE' } };
    const createResponse = await runGraphqlPayload(productCreateDocument, createVariables);
    const createdProductId = productId(createResponse);
    const createdVariantId = productVariantId(createResponse);
    createdProductIds.push(createdProductId);
    const priceUpdateVariables = {
      productId: createdProductId,
      variants: [{ id: createdVariantId, price: productCase.price }],
    };
    const priceUpdateResponse = await runGraphqlPayload(productPriceUpdateDocument, priceUpdateVariables);
    windowProducts[productCase.key] = {
      create: { variables: createVariables, response: createResponse },
      priceUpdate: { variables: priceUpdateVariables, response: priceUpdateResponse },
      productId: createdProductId,
      variantId: createdVariantId,
    };
  }
  const windowProductIds = windowProductCases.map((productCase) => windowProducts[productCase.key]?.productId ?? '');
  if (windowProductIds.some((id) => id.length === 0)) {
    throw new Error(`Window product setup did not record every product id: ${JSON.stringify(windowProducts)}`);
  }

  const windowCollectionCreateVariables = {
    input: { title: `Collection products window ${runId}`, sortOrder: 'MANUAL' },
  };
  const windowCollectionCreate = await runGraphqlPayload(createDocument, windowCollectionCreateVariables);
  const windowCollectionId = collectionId(windowCollectionCreate);
  if (!windowCollectionId) {
    throw new Error('Window collection create did not return a collection id.');
  }
  createdIds.push(windowCollectionId);

  const windowCollectionAddVariables = { id: windowCollectionId, productIds: windowProductIds };
  const windowCollectionAdd = await runGraphqlPayload(addDocument, windowCollectionAddVariables);
  const windowFirstReadVariables = { id: windowCollectionId, first: 2 };
  const windowFirstRead = await runGraphqlPayload(windowReadDocument, windowFirstReadVariables);
  const windowAfterReadVariables = {
    id: windowCollectionId,
    first: 2,
    after: pageEndCursor(windowFirstRead),
  };
  const windowAfterRead = await runGraphqlPayload(afterReadDocument, windowAfterReadVariables);
  const windowSortReadVariables = { id: windowCollectionId };
  const windowSortRead = await runGraphqlPayload(sortReadDocument, windowSortReadVariables);

  const fixture = {
    storeDomain,
    apiVersion,
    seedProduct,
    longTitle: { variables: longTitleVariables, response: longTitle },
    reservedLike: { variables: reservedLikeVariables, response: reservedLike },
    invalidSort: { variables: invalidSortVariables, response: invalidSort },
    smartCreate: { variables: smartCreateVariables, response: smartCreate },
    smartAdd: { variables: smartAddVariables, response: smartAdd },
    smartRemove: { variables: smartAddVariables, response: smartRemove },
    customCreate: { variables: customCreateVariables, response: customCreate },
    customAdd: { variables: customAddVariables, response: customAdd },
    customDuplicateAdd: { variables: customDuplicateAddVariables, response: customDuplicateAdd },
    customRead: { variables: customReadVariables, response: customRead },
    windowProducts,
    windowCollectionCreate: { variables: windowCollectionCreateVariables, response: windowCollectionCreate },
    windowCollectionAdd: { variables: windowCollectionAddVariables, response: windowCollectionAdd },
    windowFirstRead: { variables: windowFirstReadVariables, response: windowFirstRead },
    windowAfterRead: { variables: windowAfterReadVariables, response: windowAfterRead },
    windowSortRead: { variables: windowSortReadVariables, response: windowSortRead },
    upstreamCalls: [productHydrationCassette(seedProduct)],
  };

  const spec = {
    scenarioId: 'collection-create-and-add-products-parity',
    operationNames: [
      'productCreate',
      'productVariantsBulkUpdate',
      'collectionCreate',
      'collectionAddProducts',
      'collectionRemoveProducts',
      'collection',
    ],
    scenarioStatus: 'captured',
    assertionKinds: [
      'payload-shape',
      'user-errors-parity',
      'custom-vs-smart-collection-behavior',
      'downstream-read-parity',
      'connection-windowing-parity',
      'sort-key-parity',
    ],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: createDocumentPath,
      variablesCapturePath: '$.longTitle.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Executable parity for collectionCreate validation, enum coercion, smart collection add/remove behavior, productsCount read-after-add behavior, collectionAddProducts duplicate-membership no-op success, and Collection.products windowing/pageInfo/sortKey/reverse behavior. The duplicate target creates a custom collection, adds the seed product, then adds the same product again; Shopify returns the collection with empty userErrors. The live 2025-01 capture found reserved-looking titles such as Frontpage are accepted by Admin GraphQL, and v1 collectionAddProducts/collectionRemoveProducts accept smart collections, so this spec preserves the observed allowed behavior instead of adding local rejections. The products connection targets create disposable products with distinct titles/prices, add them to a manual custom collection, compare first/after cursor windows, and compare the ProductCollectionSortKeys exposed by this API version. Admin API 2025-01 rejects INVENTORY on this field, so inventory ordering is documented and covered by focused Rust tests rather than this 2025-01 parity target.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [
        {
          path: '$.collectionCreate.collection.id',
          matcher: 'shopify-gid:Collection',
          reason: 'Shopify and the local parity harness allocate collection identifiers independently.',
        },
        {
          path: '$.collectionAddProducts.collection.id',
          matcher: 'shopify-gid:Collection',
          reason: 'The add-products mutation is issued against the locally staged custom collection id.',
        },
        {
          path: '$.collection.id',
          matcher: 'shopify-gid:Collection',
          reason: 'The downstream read targets the locally staged custom collection id.',
        },
      ],
      targets: [
        {
          name: 'long-title-user-error',
          capturePath: '$.longTitle.response.data',
          proxyPath: '$.data',
        },
        {
          name: 'reserved-like-title-live-behavior',
          capturePath: '$.reservedLike.response.data',
          proxyRequest: {
            documentPath: createDocumentPath,
            variablesCapturePath: '$.reservedLike.variables',
          },
          proxyPath: '$.data',
          selectedPaths: ['$.collectionCreate.userErrors'],
        },
        {
          name: 'invalid-sort-order-coercion',
          capturePath: '$.invalidSort.response.errors',
          proxyRequest: {
            documentPath: createDocumentPath,
            variablesCapturePath: '$.invalidSort.variables',
          },
          proxyPath: '$.errors',
        },
        {
          name: 'smart-create',
          capturePath: '$.smartCreate.response.data',
          proxyRequest: {
            documentPath: createDocumentPath,
            variablesCapturePath: '$.smartCreate.variables',
          },
          proxyPath: '$.data',
        },
        {
          name: 'smart-add-products-live-behavior',
          capturePath: '$.smartAdd.response.data',
          proxyRequest: {
            documentPath: addDocumentPath,
            variables: {
              id: {
                fromProxyResponse: 'smart-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromCapturePath: '$.seedProduct.id' }],
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'smart-remove-products-live-behavior',
          capturePath: '$.smartRemove.response.data',
          proxyRequest: {
            documentPath: removeDocumentPath,
            variables: {
              id: {
                fromProxyResponse: 'smart-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromCapturePath: '$.seedProduct.id' }],
            },
          },
          proxyPath: '$.data',
          expectedDifferences: [
            {
              path: '$.collectionRemoveProducts.job.id',
              matcher: 'shopify-gid:Job',
              reason: 'Shopify and the local parity harness allocate collection remove job identifiers independently.',
            },
          ],
        },
        {
          name: 'custom-create',
          capturePath: '$.customCreate.response.data',
          proxyRequest: {
            documentPath: createDocumentPath,
            variablesCapturePath: '$.customCreate.variables',
          },
          proxyPath: '$.data',
        },
        {
          name: 'custom-add-products',
          capturePath: '$.customAdd.response.data',
          proxyRequest: {
            documentPath: addDocumentPath,
            variables: {
              id: {
                fromProxyResponse: 'custom-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromCapturePath: '$.seedProduct.id' }],
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'custom-duplicate-add-products-noop-success',
          capturePath: '$.customDuplicateAdd.response.data',
          proxyRequest: {
            documentPath: addDocumentPath,
            variables: {
              id: {
                fromProxyResponse: 'custom-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [{ fromCapturePath: '$.seedProduct.id' }],
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'custom-products-count-read',
          capturePath: '$.customRead.response.data',
          proxyRequest: {
            documentPath: readDocumentPath,
            variables: {
              id: {
                fromProxyResponse: 'custom-create',
                path: '$.data.collectionCreate.collection.id',
              },
            },
          },
          proxyPath: '$.data',
        },
        {
          name: 'window-product-zulu-create',
          capturePath: '$.windowProducts.zulu.create.response.data',
          proxyRequest: {
            documentPath: productCreateDocumentPath,
            variablesCapturePath: '$.windowProducts.zulu.create.variables',
          },
          proxyPath: '$.data',
          selectedPaths: ['$.productCreate.product.title', '$.productCreate.userErrors'],
        },
        {
          name: 'window-product-alpha-create',
          capturePath: '$.windowProducts.alpha.create.response.data',
          proxyRequest: {
            documentPath: productCreateDocumentPath,
            variablesCapturePath: '$.windowProducts.alpha.create.variables',
          },
          proxyPath: '$.data',
          selectedPaths: ['$.productCreate.product.title', '$.productCreate.userErrors'],
        },
        {
          name: 'window-product-mike-create',
          capturePath: '$.windowProducts.mike.create.response.data',
          proxyRequest: {
            documentPath: productCreateDocumentPath,
            variablesCapturePath: '$.windowProducts.mike.create.variables',
          },
          proxyPath: '$.data',
          selectedPaths: ['$.productCreate.product.title', '$.productCreate.userErrors'],
        },
        {
          name: 'window-product-zulu-price-update',
          capturePath: '$.windowProducts.zulu.priceUpdate.response.data',
          proxyRequest: {
            documentPath: productPriceUpdateDocumentPath,
            variables: {
              productId: {
                fromProxyResponse: 'window-product-zulu-create',
                path: '$.data.productCreate.product.id',
              },
              variants: [
                {
                  id: {
                    fromProxyResponse: 'window-product-zulu-create',
                    path: '$.data.productCreate.product.variants.nodes[0].id',
                  },
                  price: { fromCapturePath: '$.windowProducts.zulu.priceUpdate.variables.variants[0].price' },
                },
              ],
            },
          },
          proxyPath: '$.data',
          selectedPaths: [
            '$.productVariantsBulkUpdate.productVariants[0].price',
            '$.productVariantsBulkUpdate.userErrors',
          ],
        },
        {
          name: 'window-product-alpha-price-update',
          capturePath: '$.windowProducts.alpha.priceUpdate.response.data',
          proxyRequest: {
            documentPath: productPriceUpdateDocumentPath,
            variables: {
              productId: {
                fromProxyResponse: 'window-product-alpha-create',
                path: '$.data.productCreate.product.id',
              },
              variants: [
                {
                  id: {
                    fromProxyResponse: 'window-product-alpha-create',
                    path: '$.data.productCreate.product.variants.nodes[0].id',
                  },
                  price: { fromCapturePath: '$.windowProducts.alpha.priceUpdate.variables.variants[0].price' },
                },
              ],
            },
          },
          proxyPath: '$.data',
          selectedPaths: [
            '$.productVariantsBulkUpdate.productVariants[0].price',
            '$.productVariantsBulkUpdate.userErrors',
          ],
        },
        {
          name: 'window-product-mike-price-update',
          capturePath: '$.windowProducts.mike.priceUpdate.response.data',
          proxyRequest: {
            documentPath: productPriceUpdateDocumentPath,
            variables: {
              productId: {
                fromProxyResponse: 'window-product-mike-create',
                path: '$.data.productCreate.product.id',
              },
              variants: [
                {
                  id: {
                    fromProxyResponse: 'window-product-mike-create',
                    path: '$.data.productCreate.product.variants.nodes[0].id',
                  },
                  price: { fromCapturePath: '$.windowProducts.mike.priceUpdate.variables.variants[0].price' },
                },
              ],
            },
          },
          proxyPath: '$.data',
          selectedPaths: [
            '$.productVariantsBulkUpdate.productVariants[0].price',
            '$.productVariantsBulkUpdate.userErrors',
          ],
        },
        {
          name: 'window-collection-create',
          capturePath: '$.windowCollectionCreate.response.data',
          proxyRequest: {
            documentPath: createDocumentPath,
            variablesCapturePath: '$.windowCollectionCreate.variables',
          },
          proxyPath: '$.data',
          selectedPaths: [
            '$.collectionCreate.collection.title',
            '$.collectionCreate.collection.sortOrder',
            '$.collectionCreate.userErrors',
          ],
        },
        {
          name: 'window-collection-add-products',
          capturePath: '$.windowCollectionAdd.response.data',
          proxyRequest: {
            documentPath: addDocumentPath,
            variables: {
              id: {
                fromProxyResponse: 'window-collection-create',
                path: '$.data.collectionCreate.collection.id',
              },
              productIds: [
                {
                  fromProxyResponse: 'window-product-zulu-create',
                  path: '$.data.productCreate.product.id',
                },
                {
                  fromProxyResponse: 'window-product-alpha-create',
                  path: '$.data.productCreate.product.id',
                },
                {
                  fromProxyResponse: 'window-product-mike-create',
                  path: '$.data.productCreate.product.id',
                },
              ],
            },
          },
          proxyPath: '$.data',
          selectedPaths: [
            '$.collectionAddProducts.collection.products.nodes[0].title',
            '$.collectionAddProducts.collection.products.nodes[1].title',
            '$.collectionAddProducts.collection.products.nodes[2].title',
            '$.collectionAddProducts.userErrors',
          ],
        },
        {
          name: 'collection-products-manual-first-page',
          capturePath: '$.windowFirstRead.response.data',
          proxyRequest: {
            documentPath: windowReadDocumentPath,
            variables: {
              id: {
                fromProxyResponse: 'window-collection-create',
                path: '$.data.collectionCreate.collection.id',
              },
              first: { fromCapturePath: '$.windowFirstRead.variables.first' },
            },
          },
          proxyPath: '$.data',
          expectedDifferences: [
            ...productIdDifferences('$.collection.products'),
            ...cursorDifferences('$.collection.products'),
          ],
        },
        {
          name: 'collection-products-manual-after-page',
          capturePath: '$.windowAfterRead.response.data',
          proxyRequest: {
            documentPath: afterReadDocumentPath,
            variables: {
              id: {
                fromProxyResponse: 'window-collection-create',
                path: '$.data.collectionCreate.collection.id',
              },
              first: { fromCapturePath: '$.windowAfterRead.variables.first' },
              after: {
                fromProxyResponse: 'collection-products-manual-first-page',
                path: '$.data.collection.products.pageInfo.endCursor',
              },
            },
          },
          proxyPath: '$.data',
          expectedDifferences: [
            ...productIdDifferences('$.collection.products'),
            ...cursorDifferences('$.collection.products'),
          ],
        },
        {
          name: 'collection-products-sort-keys-and-reverse',
          capturePath: '$.windowSortRead.response.data',
          proxyRequest: {
            documentPath: sortReadDocumentPath,
            variables: {
              id: {
                fromProxyResponse: 'window-collection-create',
                path: '$.data.collectionCreate.collection.id',
              },
            },
          },
          proxyPath: '$.data',
          expectedDifferences: sortReadProductIdDifferences(),
        },
      ],
    },
  };

  await writeFile(productCreateDocumentPath, productCreateDocument, 'utf8');
  await writeFile(productPriceUpdateDocumentPath, productPriceUpdateDocument, 'utf8');
  await writeFile(createDocumentPath, createDocument, 'utf8');
  await writeFile(addDocumentPath, addDocument, 'utf8');
  await writeFile(removeDocumentPath, removeDocument, 'utf8');
  await writeFile(readDocumentPath, readDocument, 'utf8');
  await writeFile(windowReadDocumentPath, windowReadDocument, 'utf8');
  await writeFile(afterReadDocumentPath, afterReadDocument, 'utf8');
  await writeFile(sortReadDocumentPath, sortReadDocument, 'utf8');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        specPath,
        requestFiles: [
          productCreateDocumentPath,
          productPriceUpdateDocumentPath,
          createDocumentPath,
          addDocumentPath,
          removeDocumentPath,
          readDocumentPath,
          windowReadDocumentPath,
          afterReadDocumentPath,
          sortReadDocumentPath,
        ],
      },
      null,
      2,
    ),
  );
} finally {
  await Promise.allSettled(createdIds.map(cleanupCollection));
  await Promise.allSettled(createdProductIds.map(cleanupProduct));
}
