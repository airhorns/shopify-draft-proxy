// @ts-nocheck
import 'dotenv/config';

import { readFileSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'collection-reorder-products-manual-sort.json');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const collectionIds = [];

const reorderDocument = readFileSync(
  path.join('config', 'parity-requests', 'products', 'collectionReorderProducts-parity-plan.graphql'),
  'utf8',
);
const orderReadDocument = readFileSync(
  path.join('config', 'parity-requests', 'products', 'collectionReorderProducts-order-read.graphql'),
  'utf8',
);
const hydrateDocument = readFileSync(
  path.join('config', 'parity-requests', 'products', 'products-hydrate-nodes-observation.graphql'),
  'utf8',
);
const sortOrderHydrateDocument = readFileSync(
  path.join('config', 'parity-requests', 'products', 'collectionReorderProducts-collection-hydrate.graphql'),
  'utf8',
);

const seedProductsQuery = `#graphql
  query CollectionReorderSeedProducts {
    products(first: 50, sortKey: UPDATED_AT, reverse: true) {
      edges {
        node {
          id
          title
          handle
          status
        }
      }
    }
  }
`;

const createCollectionMutation = `#graphql
  mutation CollectionReorderCreateCollection($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        id
        title
        handle
        sortOrder
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const addProductsMutation = `#graphql
  mutation CollectionReorderAddProducts($id: ID!, $productIds: [ID!]!) {
    collectionAddProducts(id: $id, productIds: $productIds) {
      collection {
        id
        title
        handle
        sortOrder
        products(first: 10) {
          nodes {
            id
            title
            handle
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
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

const deleteCollectionMutation = `#graphql
  mutation CollectionReorderDeleteCollection($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const jobQuery = `#graphql
  query CollectionReorderJob($id: ID!) {
    job(id: $id) {
      id
      done
    }
  }
`;

function pickSeedProducts(payload) {
  const edges = payload?.data?.products?.edges;
  if (!Array.isArray(edges)) {
    throw new Error('Could not find product edges in collection reorder seed query payload.');
  }

  const products = [];
  for (const edge of edges) {
    const node = edge?.node;
    if (
      typeof node?.id === 'string' &&
      typeof node?.title === 'string' &&
      typeof node?.handle === 'string' &&
      node.status === 'ACTIVE'
    ) {
      products.push({
        id: node.id,
        title: node.title,
        handle: node.handle,
        status: node.status,
      });
    }
    if (products.length >= 2) break;
  }

  if (products.length < 2) {
    throw new Error('Need at least two live products to capture collection reorder parity.');
  }

  return products;
}

function sortedHydrateVariables(collectionId, productId = null) {
  return { ids: [collectionId, productId].filter(Boolean).sort() };
}

async function createCollection(runId, sortOrder) {
  const variables = {
    input: {
      title: `Hermes Reorder ${sortOrder} ${runId}`,
      sortOrder,
    },
  };
  const response = await runGraphql(createCollectionMutation, variables);
  const id = response.data?.collectionCreate?.collection?.id ?? null;
  if (!id) {
    throw new Error(`Collection create did not return an id for sortOrder ${sortOrder}.`);
  }
  collectionIds.push(id);
  return { variables, response, id };
}

async function addProducts(collectionId, productIds) {
  const variables = { id: collectionId, productIds };
  const response = await runGraphql(addProductsMutation, variables);
  const errors = response.data?.collectionAddProducts?.userErrors ?? [];
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`collectionAddProducts returned userErrors: ${JSON.stringify(errors)}`);
  }
  return { variables, response };
}

async function captureHydrate(collectionId, movedProductId = null) {
  const variables = sortedHydrateVariables(collectionId, movedProductId);
  const response = await runGraphql(hydrateDocument, variables);
  return {
    operationName: 'ProductsHydrateNodes',
    variables,
    query: hydrateDocument,
    response: { status: 200, body: response },
  };
}

async function captureSortOrderHydrate(collectionId) {
  const variables = { id: collectionId };
  const response = await runGraphql(sortOrderHydrateDocument, variables);
  return {
    operationName: 'CollectionReorderProductsCollectionHydrate',
    variables,
    query: sortOrderHydrateDocument,
    response: { status: 200, body: response },
  };
}

async function waitForJobDone(jobId) {
  if (typeof jobId !== 'string' || jobId.length === 0) return null;
  let latest = null;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    latest = await runGraphql(jobQuery, { id: jobId });
    if (latest.data?.job?.done === true) return latest;
    await delay(500);
  }
  return latest;
}

async function captureManualScenario(runId, firstProduct, secondProduct) {
  const create = await createCollection(runId, 'MANUAL');
  const add = await addProducts(create.id, [firstProduct.id, secondProduct.id]);
  const hydrateCall = await captureHydrate(create.id, secondProduct.id);
  const sortOrderHydrateCall = await captureSortOrderHydrate(create.id);
  const mutation = {
    variables: {
      id: create.id,
      moves: [{ id: secondProduct.id, newPosition: '0' }],
    },
    response: await runGraphql(reorderDocument, {
      id: create.id,
      moves: [{ id: secondProduct.id, newPosition: '0' }],
    }),
  };
  const jobRead = await waitForJobDone(mutation.response.data?.collectionReorderProducts?.job?.id);
  const downstreamReadVariables = { collectionId: create.id };
  const downstreamRead = await runGraphql(orderReadDocument, downstreamReadVariables);
  const downstreamCall = {
    operationName: 'CollectionReorderProductsOrderRead',
    variables: downstreamReadVariables,
    query: orderReadDocument,
    response: { status: 200, body: downstreamRead },
  };
  return {
    create,
    add,
    hydrateCall,
    sortOrderHydrateCall,
    mutation,
    jobRead,
    downstreamReadVariables,
    downstreamRead,
    downstreamCall,
  };
}

async function captureNonManualScenario(runId, firstProduct, secondProduct) {
  const create = await createCollection(runId, 'BEST_SELLING');
  const add = await addProducts(create.id, [firstProduct.id, secondProduct.id]);
  const hydrateCall = await captureHydrate(create.id);
  const sortOrderHydrateCall = await captureSortOrderHydrate(create.id);
  const mutation = {
    variables: {
      id: create.id,
      moves: [{ id: secondProduct.id, newPosition: '0' }],
    },
    response: await runGraphql(reorderDocument, {
      id: create.id,
      moves: [{ id: secondProduct.id, newPosition: '0' }],
    }),
  };
  const downstreamReadVariables = { collectionId: create.id };
  const downstreamRead = await runGraphql(orderReadDocument, downstreamReadVariables);
  const downstreamCall = {
    operationName: 'CollectionReorderProductsOrderRead',
    variables: downstreamReadVariables,
    query: orderReadDocument,
    response: { status: 200, body: downstreamRead },
  };
  return {
    create,
    add,
    hydrateCall,
    sortOrderHydrateCall,
    mutation,
    downstreamReadVariables,
    downstreamRead,
    downstreamCall,
  };
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const seedProductsResponse = await runGraphql(seedProductsQuery);
const [firstProduct, secondProduct] = pickSeedProducts(seedProductsResponse);

try {
  const manualSorted = await captureManualScenario(runId, firstProduct, secondProduct);
  const nonManualSorted = await captureNonManualScenario(runId, firstProduct, secondProduct);

  const payload = {
    storeDomain,
    apiVersion,
    seedProducts: [firstProduct, secondProduct],
    manualSorted: {
      create: manualSorted.create,
      add: manualSorted.add,
      mutation: manualSorted.mutation,
      jobRead: manualSorted.jobRead,
      downstreamReadVariables: manualSorted.downstreamReadVariables,
      downstreamRead: manualSorted.downstreamRead,
    },
    nonManualSorted: {
      create: nonManualSorted.create,
      add: nonManualSorted.add,
      mutation: nonManualSorted.mutation,
      downstreamReadVariables: nonManualSorted.downstreamReadVariables,
      downstreamRead: nonManualSorted.downstreamRead,
    },
    upstreamCalls: [
      manualSorted.hydrateCall,
      manualSorted.sortOrderHydrateCall,
      manualSorted.downstreamCall,
      nonManualSorted.hydrateCall,
      nonManualSorted.sortOrderHydrateCall,
      nonManualSorted.downstreamCall,
    ],
  };

  await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        apiVersion,
        seedProducts: [firstProduct, secondProduct],
        manualCollectionId: manualSorted.create.id,
        nonManualCollectionId: nonManualSorted.create.id,
      },
      null,
      2,
    ),
  );
} finally {
  for (const collectionId of collectionIds.reverse()) {
    try {
      await runGraphql(deleteCollectionMutation, { input: { id: collectionId } });
    } catch {
      // Best-effort cleanup only. The capture should surface the original failure.
    }
  }
}
