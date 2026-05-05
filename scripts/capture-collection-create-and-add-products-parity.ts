import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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

const createDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-create.graphql');
const addDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-add.graphql');
const removeDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-remove.graphql');
const readDocumentPath = path.join(productsDir, 'collectionCreate-and-add-products-count-read.graphql');
const specPath = path.join(specsDir, 'collectionCreate-and-add-products-parity.json');
const fixturePath = path.join(fixtureDir, 'collection-create-and-add-products-parity.json');

const seedProductsQuery = `#graphql
query HAR594SeedProduct {
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

const createDocument = `mutation HAR594CollectionCreate($input: CollectionInput!) {
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

const addDocument = `mutation HAR594CollectionAddProducts($id: ID!, $productIds: [ID!]!) {
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

const removeDocument = `mutation HAR594CollectionRemoveProducts($id: ID!, $productIds: [ID!]!) {
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

const readDocument = `query HAR594CollectionProductsCount($id: ID!) {
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

const deleteDocument = `#graphql
mutation HAR594CleanupCollection($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) {
    deletedCollectionId
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
    ((payload.data as Record<string, unknown> | undefined)?.products as Record<string, unknown> | undefined)?.nodes as
      | Array<Record<string, unknown>>
      | undefined
  )?.[0];
  if (
    typeof product?.id !== 'string' ||
    typeof product.title !== 'string' ||
    typeof product.handle !== 'string' ||
    typeof product.status !== 'string'
  ) {
    throw new Error('Need at least one live product to capture HAR-594 collection parity.');
  }
  return {
    id: product.id,
    title: product.title,
    handle: product.handle,
    status: product.status,
  };
}

function collectionId(payload: GraphqlPayload): string | null {
  const id = (
    ((payload.data as Record<string, unknown> | undefined)?.collectionCreate as Record<string, unknown> | undefined)
      ?.collection as Record<string, unknown> | undefined
  )?.id;
  return typeof id === 'string' ? id : null;
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

function productHydrationCassette(seedProduct: ReturnType<typeof firstSeedProduct>) {
  return {
    operationName: 'ProductsHydrateNodes',
    variables: {
      ids: [seedProduct.id],
    },
    query: 'hand-synthesized from HAR-594 live seed product for mutation hydration',
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

await mkdir(productsDir, { recursive: true });
await mkdir(specsDir, { recursive: true });
await mkdir(fixtureDir, { recursive: true });

const runId = `${Date.now()}`;
const seedProduct = firstSeedProduct(await runGraphql(seedProductsQuery));
const createdIds: string[] = [];

try {
  const longTitleVariables = { input: { title: 'T'.repeat(256) } };
  const reservedLikeVariables = { input: { title: `Frontpage HAR-594 ${runId}` } };
  const invalidSortVariables = {
    input: {
      title: `HAR-594 invalid sort ${runId}`,
      sortOrder: 'INVALID_VALUE',
    },
  };
  const smartCreateVariables = {
    input: {
      title: `HAR-594 Smart ${runId}`,
      ruleSet: {
        appliedDisjunctively: false,
        rules: [{ column: 'TITLE', relation: 'CONTAINS', condition: 'HAR-594' }],
      },
    },
  };
  const customCreateVariables = { input: { title: `HAR-594 Custom ${runId}` } };

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
  const customReadVariables = { id: customId };
  const customRead = await runGraphqlPayload(readDocument, customReadVariables);

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
    customRead: { variables: customReadVariables, response: customRead },
    upstreamCalls: [productHydrationCassette(seedProduct)],
  };

  const spec = {
    scenarioId: 'collection-create-and-add-products-parity',
    operationNames: ['collectionCreate', 'collectionAddProducts', 'collectionRemoveProducts'],
    scenarioStatus: 'captured',
    assertionKinds: [
      'payload-shape',
      'user-errors-parity',
      'custom-vs-smart-collection-guards',
      'downstream-read-parity',
    ],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: createDocumentPath,
      variablesCapturePath: '$.longTitle.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'HAR-594 executable parity for collectionCreate validation, enum coercion, smart collection add/remove guards, and productsCount read-after-add behavior. The live 2025-01 capture found reserved-looking titles such as Frontpage are accepted by Admin GraphQL, so this spec preserves the observed allowed behavior instead of adding a local rejection.',
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
          name: 'smart-add-products-user-error',
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
          name: 'smart-remove-products-user-error',
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
      ],
    },
  };

  await writeFile(createDocumentPath, createDocument, 'utf8');
  await writeFile(addDocumentPath, addDocument, 'utf8');
  await writeFile(removeDocumentPath, removeDocument, 'utf8');
  await writeFile(readDocumentPath, readDocument, 'utf8');
  await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        specPath,
        requestFiles: [createDocumentPath, addDocumentPath, removeDocumentPath, readDocumentPath],
      },
      null,
      2,
    ),
  );
} finally {
  await Promise.allSettled(createdIds.map(cleanupCollection));
}
