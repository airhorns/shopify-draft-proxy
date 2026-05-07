/* oxlint-disable no-console -- CLI recorder intentionally writes capture status to stdout. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureEntry = {
  query: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    payload: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const requestDir = path.join('config', 'parity-requests', 'products');
const specDir = path.join('config', 'parity-specs', 'products');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const collectionCreateDocumentPath = path.join(
  requestDir,
  'productCreate-collections-to-join-collection-create.graphql',
);
const categoryDocumentPath = path.join(requestDir, 'productCreate-category-parity.graphql');
const categoryReadDocumentPath = path.join(requestDir, 'productCreate-category-downstream-read.graphql');
const requiresSellingPlanDocumentPath = path.join(requestDir, 'productCreate-requires-selling-plan-parity.graphql');
const requiresSellingPlanReadDocumentPath = path.join(
  requestDir,
  'productCreate-requires-selling-plan-downstream-read.graphql',
);
const collectionsToJoinDocumentPath = path.join(requestDir, 'productCreate-collections-to-join-parity.graphql');
const collectionsToJoinReadDocumentPath = path.join(
  requestDir,
  'productCreate-collections-to-join-downstream-read.graphql',
);

const categoryFixturePath = path.join(outputDir, 'productCreate-category-parity.json');
const requiresSellingPlanFixturePath = path.join(outputDir, 'productCreate-requires-selling-plan-parity.json');
const collectionsToJoinFixturePath = path.join(outputDir, 'productCreate-collections-to-join-parity.json');

const categorySpecPath = path.join(specDir, 'productCreate-category-parity.json');
const requiresSellingPlanSpecPath = path.join(specDir, 'productCreate-requires-selling-plan-parity.json');
const collectionsToJoinSpecPath = path.join(specDir, 'productCreate-collections-to-join-parity.json');

const collectionCreateDocument = `mutation ProductCreateCollectionsToJoinCollectionCreate($input: CollectionInput!) {
  collectionCreate(input: $input) {
    collection {
      id
      title
      handle
      products(first: 5) {
        nodes {
          id
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

const collectionDeleteDocument = `mutation ProductCreateCollectionsToJoinCollectionDelete($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) {
    deletedCollectionId
    userErrors {
      field
      message
    }
  }
}
`;

const productDeleteDocument = `mutation ProductCreateInputFieldsCleanup($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

const productCreateCategoryDocument = `mutation ProductCreateCategoryParity($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      handle
      productType
      category {
        id
        fullName
      }
      requiresSellingPlan
    }
    userErrors {
      field
      message
    }
  }
}
`;

const productCreateCategoryReadDocument = `query ProductCreateCategoryDownstreamRead($id: ID!) {
  product(id: $id) {
    id
    title
    handle
    productType
    category {
      id
      fullName
    }
    requiresSellingPlan
  }
}
`;

const productCreateRequiresSellingPlanDocument = `mutation ProductCreateRequiresSellingPlanParity($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      handle
      requiresSellingPlan
      category {
        id
        fullName
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const productCreateRequiresSellingPlanReadDocument = `query ProductCreateRequiresSellingPlanDownstreamRead($id: ID!) {
  product(id: $id) {
    id
    title
    handle
    requiresSellingPlan
    category {
      id
      fullName
    }
  }
}
`;

const productCreateCollectionsToJoinDocument = `mutation ProductCreateCollectionsToJoinParity($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      handle
      collections(first: 10) {
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

const productCreateCollectionsToJoinReadDocument = `query ProductCreateCollectionsToJoinDownstreamRead($productId: ID!, $firstCollectionId: ID!, $secondCollectionId: ID!) {
  product(id: $productId) {
    id
    title
    handle
    collections(first: 10) {
      nodes {
        id
        title
        handle
      }
    }
  }
  firstCollection: collection(id: $firstCollectionId) {
    id
    title
    handle
    hasProduct(id: $productId)
    productsCount {
      count
      precision
    }
    products(first: 10) {
      nodes {
        id
        title
        handle
      }
    }
  }
  secondCollection: collection(id: $secondCollectionId) {
    id
    title
    handle
    hasProduct(id: $productId)
    productsCount {
      count
      precision
    }
    products(first: 10) {
      nodes {
        id
        title
        handle
      }
    }
  }
}
`;

async function capture(query: string, variables: Record<string, unknown>): Promise<CaptureEntry> {
  const result: ConformanceGraphqlResult = await runGraphqlRequest(query, variables);
  return {
    query,
    variables,
    response: {
      status: result.status,
      payload: result.payload,
    },
  };
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    current = readRecord(current)?.[part];
  }
  return current;
}

function readStringPath(value: unknown, pathParts: string[], context: string): string {
  const found = readPath(value, pathParts);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`Expected ${context} string at ${pathParts.join('.')}: ${JSON.stringify(value)}`);
  }
  return found;
}

function readUserErrors(entry: CaptureEntry, root: string): unknown[] {
  const userErrors = readPath(entry.response.payload, ['data', root, 'userErrors']);
  return Array.isArray(userErrors) ? userErrors : [];
}

function assertNoTopLevelErrors(entry: CaptureEntry, context: string): void {
  const payload = readRecord(entry.response.payload);
  if (entry.response.status < 200 || entry.response.status >= 300 || payload?.['errors'] !== undefined) {
    throw new Error(`${context} returned top-level errors: ${JSON.stringify(entry.response.payload)}`);
  }
}

function assertNoUserErrors(entry: CaptureEntry, root: string, context: string): void {
  assertNoTopLevelErrors(entry, context);
  const userErrors = readUserErrors(entry, root);
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(entry.response.payload)}`);
  }
}

function productIdFromCreate(entry: CaptureEntry): string {
  return readStringPath(entry.response.payload, ['data', 'productCreate', 'product', 'id'], 'productCreate id');
}

function collectionIdFromCreate(entry: CaptureEntry): string {
  return readStringPath(
    entry.response.payload,
    ['data', 'collectionCreate', 'collection', 'id'],
    'collectionCreate id',
  );
}

async function cleanupProduct(productId: string | null): Promise<CaptureEntry | null> {
  if (productId === null) {
    return null;
  }
  return capture(productDeleteDocument, { input: { id: productId } });
}

async function cleanupCollection(collectionId: string | null): Promise<CaptureEntry | null> {
  if (collectionId === null) {
    return null;
  }
  return capture(collectionDeleteDocument, { input: { id: collectionId } });
}

function buildProductIdDifferences(root = '$.productCreate.product'): Record<string, string>[] {
  return [
    {
      path: `${root}.id`,
      matcher: 'shopify-gid:Product',
      reason: 'Shopify and the local staging registry allocate product ids independently.',
    },
  ];
}

function buildSpecBase(args: {
  scenarioId: string;
  operationNames: string[];
  fixturePath: string;
  requestPath: string;
  notes: string;
  targets: unknown[];
  expectedDifferences?: unknown[];
}): Record<string, unknown> {
  return {
    scenarioId: args.scenarioId,
    operationNames: args.operationNames,
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'downstream-read-parity'],
    liveCaptureFiles: [args.fixturePath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/products_mutation_test.gleam'],
    proxyRequest: {
      documentPath: args.requestPath,
      apiVersion,
      variablesCapturePath: '$.mutation.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes: args.notes,
    comparison: {
      mode: 'strict-json',
      expectedDifferences: args.expectedDifferences ?? [],
      targets: args.targets,
    },
  };
}

function categorySpec(): Record<string, unknown> {
  return buildSpecBase({
    scenarioId: 'productCreate-category-parity',
    operationNames: ['productCreate'],
    fixturePath: categoryFixturePath,
    requestPath: categoryDocumentPath,
    notes:
      'Captured Shopify productCreate with ProductCreateInput.category and an immediate product readback. The capture also stores invalid taxonomy and category-plus-productType probes; public 2025-01 accepts productType alongside category.',
    expectedDifferences: buildProductIdDifferences(),
    targets: [
      {
        name: 'mutation-data',
        capturePath: '$.mutation.response.payload.data',
        proxyPath: '$.data',
      },
      {
        name: 'downstream-read-data',
        capturePath: '$.downstreamRead.response.payload.data',
        proxyRequest: {
          documentPath: categoryReadDocumentPath,
          apiVersion,
          variables: {
            id: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
          },
        },
        proxyPath: '$.data',
        expectedDifferences: buildProductIdDifferences('$.product'),
      },
    ],
  });
}

function requiresSellingPlanSpec(): Record<string, unknown> {
  return buildSpecBase({
    scenarioId: 'productCreate-requires-selling-plan-parity',
    operationNames: ['productCreate'],
    fixturePath: requiresSellingPlanFixturePath,
    requestPath: requiresSellingPlanDocumentPath,
    notes:
      'Captured Shopify productCreate with ProductCreateInput.requiresSellingPlan true and an immediate product readback. The proxy must stage and expose the explicit subscription-only flag instead of falling back to false.',
    expectedDifferences: buildProductIdDifferences(),
    targets: [
      {
        name: 'mutation-data',
        capturePath: '$.mutation.response.payload.data',
        proxyPath: '$.data',
      },
      {
        name: 'downstream-read-data',
        capturePath: '$.downstreamRead.response.payload.data',
        proxyRequest: {
          documentPath: requiresSellingPlanReadDocumentPath,
          apiVersion,
          variables: {
            id: { fromPrimaryProxyPath: '$.data.productCreate.product.id' },
          },
        },
        proxyPath: '$.data',
        expectedDifferences: buildProductIdDifferences('$.product'),
      },
    ],
  });
}

function collectionsToJoinSpec(): Record<string, unknown> {
  return {
    scenarioId: 'productCreate-collections-to-join-parity',
    operationNames: ['collectionCreate', 'productCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'downstream-read-parity'],
    liveCaptureFiles: [collectionsToJoinFixturePath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/products_mutation_test.gleam'],
    proxyRequest: {
      documentPath: collectionCreateDocumentPath,
      apiVersion,
      variablesCapturePath: '$.setup.firstCollection.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Captured Shopify productCreate with ProductCreateInput.collectionsToJoin after creating two disposable collections in the same scenario. The parity replay earns collection state through the replayed collectionCreate requests before staging the productCreate membership writes. The capture also stores Shopify behavior for unknown collection ids, which are ignored in public 2025-01 rather than returned as userErrors.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [
        {
          path: '$.collectionCreate.collection.id',
          matcher: 'shopify-gid:Collection',
          reason: 'Shopify and the local staging registry allocate collection ids independently.',
        },
      ],
      targets: [
        {
          name: 'first-collection-create-data',
          capturePath: '$.setup.firstCollection.response.payload.data',
          proxyPath: '$.data',
        },
        {
          name: 'second-collection-create-data',
          capturePath: '$.setup.secondCollection.response.payload.data',
          proxyRequest: {
            documentPath: collectionCreateDocumentPath,
            apiVersion,
            variablesCapturePath: '$.setup.secondCollection.variables',
          },
          proxyPath: '$.data',
          expectedDifferences: [
            {
              path: '$.collectionCreate.collection.id',
              matcher: 'shopify-gid:Collection',
              reason: 'Shopify and the local staging registry allocate collection ids independently.',
            },
          ],
        },
        {
          name: 'mutation-data',
          capturePath: '$.mutation.response.payload.data.productCreate',
          proxyRequest: {
            documentPath: collectionsToJoinDocumentPath,
            apiVersion,
            variables: {
              product: {
                title: { fromCapturePath: '$.mutation.variables.product.title' },
                status: { fromCapturePath: '$.mutation.variables.product.status' },
                collectionsToJoin: [
                  { fromPrimaryProxyPath: '$.data.collectionCreate.collection.id' },
                  { fromProxyResponse: 'second-collection-create-data', path: '$.data.collectionCreate.collection.id' },
                ],
              },
            },
          },
          proxyPath: '$.data.productCreate',
          expectedDifferences: [
            ...buildProductIdDifferences('$.product'),
            {
              path: '$.product.collections.nodes[0].id',
              matcher: 'shopify-gid:Collection',
              reason: 'The joined collection id differs between live capture and local replay.',
            },
            {
              path: '$.product.collections.nodes[1].id',
              matcher: 'shopify-gid:Collection',
              reason: 'The joined collection id differs between live capture and local replay.',
            },
          ],
        },
        {
          name: 'downstream-read-data',
          capturePath: '$.downstreamRead.response.payload.data',
          proxyRequest: {
            documentPath: collectionsToJoinReadDocumentPath,
            apiVersion,
            variables: {
              productId: { fromProxyResponse: 'mutation-data', path: '$.data.productCreate.product.id' },
              firstCollectionId: { fromPrimaryProxyPath: '$.data.collectionCreate.collection.id' },
              secondCollectionId: {
                fromProxyResponse: 'second-collection-create-data',
                path: '$.data.collectionCreate.collection.id',
              },
            },
          },
          proxyPath: '$.data',
          expectedDifferences: [
            {
              path: '$.product.id',
              matcher: 'shopify-gid:Product',
              reason: 'The downstream product id differs between live capture and local replay.',
            },
            {
              path: '$.product.collections.nodes[0].id',
              matcher: 'shopify-gid:Collection',
              reason: 'The downstream joined collection id differs between live capture and local replay.',
            },
            {
              path: '$.product.collections.nodes[1].id',
              matcher: 'shopify-gid:Collection',
              reason: 'The downstream joined collection id differs between live capture and local replay.',
            },
            {
              path: '$.firstCollection.id',
              matcher: 'shopify-gid:Collection',
              reason: 'The first collection id differs between live capture and local replay.',
            },
            {
              path: '$.firstCollection.products.nodes[0].id',
              matcher: 'shopify-gid:Product',
              reason: 'The first collection product id differs between live capture and local replay.',
            },
            {
              path: '$.secondCollection.id',
              matcher: 'shopify-gid:Collection',
              reason: 'The second collection id differs between live capture and local replay.',
            },
            {
              path: '$.secondCollection.products.nodes[0].id',
              matcher: 'shopify-gid:Product',
              reason: 'The second collection product id differs between live capture and local replay.',
            },
          ],
        },
      ],
    },
  };
}

await mkdir(outputDir, { recursive: true });
await mkdir(requestDir, { recursive: true });
await mkdir(specDir, { recursive: true });

const runId = `${Date.now()}`;
let categoryProductId: string | null = null;
let requiresSellingPlanProductId: string | null = null;
let collectionsProductId: string | null = null;
let firstCollectionId: string | null = null;
let secondCollectionId: string | null = null;

try {
  const categoryVariables = {
    product: {
      title: `Hermes Product Category ${runId}`,
      status: 'DRAFT',
      category: 'gid://shopify/TaxonomyCategory/aa-1-1',
    },
  };
  const categoryMutation = await capture(productCreateCategoryDocument, categoryVariables);
  assertNoUserErrors(categoryMutation, 'productCreate', 'category productCreate');
  categoryProductId = productIdFromCreate(categoryMutation);
  const categoryDownstreamRead = await capture(productCreateCategoryReadDocument, { id: categoryProductId });
  assertNoTopLevelErrors(categoryDownstreamRead, 'category downstream read');
  const invalidCategoryGid = await capture(productCreateCategoryDocument, {
    product: {
      title: `Hermes Product Bad Category ${runId}`,
      status: 'DRAFT',
      category: 'not-a-gid',
    },
  });
  const unknownCategory = await capture(productCreateCategoryDocument, {
    product: {
      title: `Hermes Product Unknown Category ${runId}`,
      status: 'DRAFT',
      category: 'gid://shopify/TaxonomyCategory/not-a-real-node',
    },
  });
  const categoryAndProductType = await capture(productCreateCategoryDocument, {
    product: {
      title: `Hermes Product Category Type ${runId}`,
      status: 'DRAFT',
      category: 'gid://shopify/TaxonomyCategory/aa-1-1',
      productType: 'Boards',
    },
  });
  assertNoUserErrors(categoryAndProductType, 'productCreate', 'category plus productType productCreate');
  const categoryAndProductTypeProductId = productIdFromCreate(categoryAndProductType);
  const categoryAndProductTypeCleanup = await cleanupProduct(categoryAndProductTypeProductId);

  await writeFile(
    categoryFixturePath,
    `${JSON.stringify(
      {
        scenarioId: 'productCreate-category-parity',
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        mutation: categoryMutation,
        downstreamRead: categoryDownstreamRead,
        validation: {
          invalidCategoryGid,
          unknownCategory,
          categoryAndProductType,
          categoryAndProductTypeCleanup,
        },
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );

  const requiresSellingPlanVariables = {
    product: {
      title: `Hermes Product Requires Selling Plan ${runId}`,
      status: 'DRAFT',
      requiresSellingPlan: true,
    },
  };
  const requiresSellingPlanMutation = await capture(
    productCreateRequiresSellingPlanDocument,
    requiresSellingPlanVariables,
  );
  assertNoUserErrors(requiresSellingPlanMutation, 'productCreate', 'requiresSellingPlan productCreate');
  requiresSellingPlanProductId = productIdFromCreate(requiresSellingPlanMutation);
  const requiresSellingPlanDownstreamRead = await capture(productCreateRequiresSellingPlanReadDocument, {
    id: requiresSellingPlanProductId,
  });
  assertNoTopLevelErrors(requiresSellingPlanDownstreamRead, 'requiresSellingPlan downstream read');

  await writeFile(
    requiresSellingPlanFixturePath,
    `${JSON.stringify(
      {
        scenarioId: 'productCreate-requires-selling-plan-parity',
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        mutation: requiresSellingPlanMutation,
        downstreamRead: requiresSellingPlanDownstreamRead,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );

  const firstCollection = await capture(collectionCreateDocument, {
    input: {
      title: `Hermes Product Join First ${runId}`,
    },
  });
  assertNoUserErrors(firstCollection, 'collectionCreate', 'first collectionCreate');
  firstCollectionId = collectionIdFromCreate(firstCollection);
  const secondCollection = await capture(collectionCreateDocument, {
    input: {
      title: `Hermes Product Join Second ${runId}`,
    },
  });
  assertNoUserErrors(secondCollection, 'collectionCreate', 'second collectionCreate');
  secondCollectionId = collectionIdFromCreate(secondCollection);

  const collectionsToJoinVariables = {
    product: {
      title: `Hermes Product Collections To Join ${runId}`,
      status: 'DRAFT',
      collectionsToJoin: [firstCollectionId, secondCollectionId],
    },
  };
  const collectionsToJoinMutation = await capture(productCreateCollectionsToJoinDocument, collectionsToJoinVariables);
  assertNoUserErrors(collectionsToJoinMutation, 'productCreate', 'collectionsToJoin productCreate');
  collectionsProductId = productIdFromCreate(collectionsToJoinMutation);
  const collectionsToJoinDownstreamRead = await capture(productCreateCollectionsToJoinReadDocument, {
    productId: collectionsProductId,
    firstCollectionId,
    secondCollectionId,
  });
  assertNoTopLevelErrors(collectionsToJoinDownstreamRead, 'collectionsToJoin downstream read');

  const unknownCollectionsIgnored = await capture(productCreateCollectionsToJoinDocument, {
    product: {
      title: `Hermes Product Unknown Collections ${runId}`,
      status: 'DRAFT',
      collectionsToJoin: ['gid://shopify/Collection/not-a-real-node', 'gid://shopify/Collection/999999999999999'],
    },
  });
  assertNoUserErrors(unknownCollectionsIgnored, 'productCreate', 'unknown collectionsToJoin productCreate');
  const unknownCollectionsProductId = productIdFromCreate(unknownCollectionsIgnored);
  const unknownCollectionsCleanup = await cleanupProduct(unknownCollectionsProductId);

  await writeFile(
    collectionsToJoinFixturePath,
    `${JSON.stringify(
      {
        scenarioId: 'productCreate-collections-to-join-parity',
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        setup: {
          firstCollection,
          secondCollection,
        },
        mutation: collectionsToJoinMutation,
        downstreamRead: collectionsToJoinDownstreamRead,
        validation: {
          unknownCollectionsIgnored,
          unknownCollectionsCleanup,
        },
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );

  await writeFile(collectionCreateDocumentPath, collectionCreateDocument);
  await writeFile(categoryDocumentPath, productCreateCategoryDocument);
  await writeFile(categoryReadDocumentPath, productCreateCategoryReadDocument);
  await writeFile(requiresSellingPlanDocumentPath, productCreateRequiresSellingPlanDocument);
  await writeFile(requiresSellingPlanReadDocumentPath, productCreateRequiresSellingPlanReadDocument);
  await writeFile(collectionsToJoinDocumentPath, productCreateCollectionsToJoinDocument);
  await writeFile(collectionsToJoinReadDocumentPath, productCreateCollectionsToJoinReadDocument);

  await writeFile(categorySpecPath, `${JSON.stringify(categorySpec(), null, 2)}\n`);
  await writeFile(requiresSellingPlanSpecPath, `${JSON.stringify(requiresSellingPlanSpec(), null, 2)}\n`);
  await writeFile(collectionsToJoinSpecPath, `${JSON.stringify(collectionsToJoinSpec(), null, 2)}\n`);

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixtureFiles: [categoryFixturePath, requiresSellingPlanFixturePath, collectionsToJoinFixturePath],
        specFiles: [categorySpecPath, requiresSellingPlanSpecPath, collectionsToJoinSpecPath],
        requestFiles: [
          categoryDocumentPath,
          categoryReadDocumentPath,
          requiresSellingPlanDocumentPath,
          requiresSellingPlanReadDocumentPath,
          collectionCreateDocumentPath,
          collectionsToJoinDocumentPath,
          collectionsToJoinReadDocumentPath,
        ],
      },
      null,
      2,
    ),
  );
} finally {
  const cleanup: Record<string, unknown> = {};
  cleanup['categoryProduct'] = await cleanupProduct(categoryProductId);
  cleanup['requiresSellingPlanProduct'] = await cleanupProduct(requiresSellingPlanProductId);
  cleanup['collectionsProduct'] = await cleanupProduct(collectionsProductId);
  cleanup['firstCollection'] = await cleanupCollection(firstCollectionId);
  cleanup['secondCollection'] = await cleanupCollection(secondCollectionId);
  console.log(`Cleanup: ${JSON.stringify(cleanup)}`);
}
