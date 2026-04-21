import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  // oxlint-disable-next-line no-console -- CLI error output is intentionally written to stderr.
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'collection-mutation-conformance-scope-blocker.md');


async function runGraphql(query, variables = {}) {
  const response = await fetch(`${adminOrigin}/admin/api/${apiVersion}/graphql.json`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...buildAdminAuthHeaders(adminAccessToken),
    },
    body: JSON.stringify({ query, variables }),
  });

  const payload = await response.json();
  if (!response.ok || payload.errors) {
    const error = new Error(JSON.stringify({ status: response.status, payload }, null, 2));
    error.result = { status: response.status, payload };
    throw error;
  }

  return payload;
}

function pickSeedProducts(payload) {
  const edges = payload?.data?.products?.edges;
  if (!Array.isArray(edges)) {
    throw new Error('Could not find product edges in collection mutation seed query payload.');
  }

  const products = [];
  for (const edge of edges) {
    const node = edge?.node;
    if (
      typeof node?.id === 'string' &&
      typeof node?.title === 'string' &&
      typeof node?.handle === 'string' &&
      typeof node?.status === 'string'
    ) {
      products.push({
        id: node.id,
        title: node.title,
        handle: node.handle,
        status: node.status,
      });
    }
    if (products.length >= 2) {
      break;
    }
  }

  if (products.length < 2) {
    throw new Error('Need at least two live products to capture collection mutation parity.');
  }

  return products;
}

const seedProductsQuery = `#graphql
  query CollectionMutationSeedProducts {
    products(first: 10, sortKey: UPDATED_AT, reverse: true) {
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

const collectionListSlice = `
  nodes {
    id
    title
    handle
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
`;

const collectionDetailSlice = `
  id
  title
  handle
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
`;

const productCollectionsSlice = `
  id
  collections(first: 10) {
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
`;

const createMutation = `#graphql
  mutation CollectionCreateConformance($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        ${collectionDetailSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const updateMutation = `#graphql
  mutation CollectionUpdateConformance($input: CollectionInput!) {
    collectionUpdate(input: $input) {
      collection {
        ${collectionDetailSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation CollectionDeleteConformance($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const addProductsMutation = `#graphql
  mutation CollectionAddProductsConformance($id: ID!, $productIds: [ID!]!) {
    collectionAddProducts(id: $id, productIds: $productIds) {
      collection {
        ${collectionDetailSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const removeProductsMutation = `#graphql
  mutation CollectionRemoveProductsConformance($id: ID!, $productIds: [ID!]!) {
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

const postCreateReadQuery = `#graphql
  query CollectionCreateDownstream($collectionId: ID!) {
    collection(id: $collectionId) {
      ${collectionDetailSlice}
    }
    collections(first: 5, sortKey: UPDATED_AT, reverse: true) {
      ${collectionListSlice}
    }
  }
`;

const postAddReadQuery = `#graphql
  query CollectionAddProductsDownstream($collectionId: ID!, $firstProductId: ID!, $secondProductId: ID!) {
    collection(id: $collectionId) {
      ${collectionDetailSlice}
    }
    first: product(id: $firstProductId) {
      ${productCollectionsSlice}
    }
    second: product(id: $secondProductId) {
      ${productCollectionsSlice}
    }
  }
`;

const postUpdateReadQuery = `#graphql
  query CollectionUpdateDownstream($collectionId: ID!, $productId: ID!) {
    collection(id: $collectionId) {
      ${collectionDetailSlice}
    }
    collections(first: 5, sortKey: UPDATED_AT, reverse: true) {
      ${collectionListSlice}
    }
    product(id: $productId) {
      ${productCollectionsSlice}
    }
  }
`;

const postRemoveReadQuery = `#graphql
  query CollectionRemoveProductsDownstream($collectionId: ID!, $removedProductId: ID!, $untouchedProductId: ID!) {
    collection(id: $collectionId) {
      ${collectionDetailSlice}
    }
    removed: product(id: $removedProductId) {
      ${productCollectionsSlice}
    }
    untouched: product(id: $untouchedProductId) {
      ${productCollectionsSlice}
    }
  }
`;

const postDeleteReadQuery = `#graphql
  query CollectionDeleteDownstream($collectionId: ID!, $remainingProductId: ID!) {
    collection(id: $collectionId) {
      id
      title
      handle
    }
    collections(first: 5, sortKey: UPDATED_AT, reverse: true) {
      ${collectionListSlice}
    }
    product(id: $remainingProductId) {
      ${productCollectionsSlice}
    }
  }
`;

function buildCreateVariables(runId) {
  return {
    input: {
      title: `Hermes Collection Conformance ${runId}`,
    },
  };
}

function buildUpdateVariables(collectionId, runId) {
  return {
    input: {
      id: collectionId,
      title: `Hermes Collection Conformance ${runId} Updated`,
      handle: `hermes-collection-conformance-${runId.toLowerCase()}-updated`,
    },
  };
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Collection mutation conformance blocker',
    whatFailed:
      'Attempted to capture live conformance for the full collection mutation family (`collectionCreate`, `collectionUpdate`, `collectionDelete`, `collectionAddProducts`, `collectionRemoveProducts`).',
    operations: [
      'collectionCreate',
      'collectionUpdate',
      'collectionDelete',
      'collectionAddProducts',
      'collectionRemoveProducts',
    ],
    blocker,
    whyBlocked:
      'Without a write-capable token plus store/user permissions for collection writes, the repo cannot capture successful live mutation payload shape, userErrors behavior for safe writes, or immediate downstream collection/product membership parity for this family.',
    completedSteps: [
      'added a reusable live-write capture harness for the staged collection mutation family',
      'kept the collection and product downstream read slices aligned with the existing parity-request scaffolds so a future write-capable token can capture the same shapes directly',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with `write_products` plus collection-write permissions, then rerun `corepack pnpm conformance:capture-collection-mutations`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const seedProductsResponse = await runGraphql(seedProductsQuery);
const [firstProduct, secondProduct] = pickSeedProducts(seedProductsResponse);
const createVariables = buildCreateVariables(runId);
let createdCollectionId = null;
let createResponse = null;
let addResponse = null;
let updateResponse = null;
let removeResponse = null;
let deleteResponse = null;

try {
  createResponse = await runGraphql(createMutation, createVariables);
  createdCollectionId = createResponse.data?.collectionCreate?.collection?.id ?? null;
  if (!createdCollectionId) {
    throw new Error('Collection create capture did not return a collection id.');
  }

  const postCreateRead = await runGraphql(postCreateReadQuery, { collectionId: createdCollectionId });

  addResponse = await runGraphql(addProductsMutation, {
    id: createdCollectionId,
    productIds: [firstProduct.id, secondProduct.id],
  });
  const postAddRead = await runGraphql(postAddReadQuery, {
    collectionId: createdCollectionId,
    firstProductId: firstProduct.id,
    secondProductId: secondProduct.id,
  });

  const updateVariables = buildUpdateVariables(createdCollectionId, runId);
  updateResponse = await runGraphql(updateMutation, updateVariables);
  const postUpdateRead = await runGraphql(postUpdateReadQuery, {
    collectionId: createdCollectionId,
    productId: firstProduct.id,
  });

  removeResponse = await runGraphql(removeProductsMutation, {
    id: createdCollectionId,
    productIds: [firstProduct.id],
  });
  const postRemoveRead = await runGraphql(postRemoveReadQuery, {
    collectionId: createdCollectionId,
    removedProductId: firstProduct.id,
    untouchedProductId: secondProduct.id,
  });

  deleteResponse = await runGraphql(deleteMutation, { input: { id: createdCollectionId } });
  const postDeleteRead = await runGraphql(postDeleteReadQuery, {
    collectionId: createdCollectionId,
    remainingProductId: secondProduct.id,
  });
  createdCollectionId = null;

  const captures = {
    'collection-create-parity.json': {
      seedProducts: [firstProduct, secondProduct],
      mutation: {
        variables: createVariables,
        response: createResponse,
      },
      downstreamRead: postCreateRead,
    },
    'collection-add-products-parity.json': {
      seedProducts: [firstProduct, secondProduct],
      mutation: {
        variables: {
          id: addResponse.data?.collectionAddProducts?.collection?.id ?? createdCollectionId,
          productIds: [firstProduct.id, secondProduct.id],
        },
        response: addResponse,
      },
      downstreamRead: postAddRead,
    },
    'collection-update-parity.json': {
      seedProducts: [firstProduct, secondProduct],
      mutation: {
        variables: updateVariables,
        response: updateResponse,
      },
      downstreamRead: postUpdateRead,
    },
    'collection-remove-products-parity.json': {
      seedProducts: [firstProduct, secondProduct],
      mutation: {
        variables: {
          id:
            createdCollectionId ??
            deleteResponse?.data?.collectionDelete?.deletedCollectionId ??
            updateVariables.input.id,
          productIds: [firstProduct.id],
        },
        response: removeResponse,
      },
      downstreamRead: postRemoveRead,
    },
    'collection-delete-parity.json': {
      seedProducts: [firstProduct, secondProduct],
      mutation: {
        variables: {
          input: { id: deleteResponse.data?.collectionDelete?.deletedCollectionId ?? updateVariables.input.id },
        },
        response: deleteResponse,
      },
      downstreamRead: postDeleteRead,
    },
  };

  for (const [filename, payload] of Object.entries(captures)) {
    await writeFile(path.join(outputDir, filename), `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  }

  // oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: Object.keys(captures),
        collectionId: deleteResponse.data?.collectionDelete?.deletedCollectionId ?? null,
        seedProducts: [firstProduct, secondProduct],
      },
      null,
      2,
    ),
  );
} catch (error) {
  const blocker = parseWriteScopeBlocker(error?.result ?? null);
  if (blocker) {
    await writeScopeBlocker(blocker);
    // oxlint-disable-next-line no-console -- CLI blocker result is intentionally written to stdout.
    console.log(
      JSON.stringify(
        {
          ok: false,
          blocked: true,
          blockerPath,
          blocker,
        },
        null,
        2,
      ),
    );
    process.exit(1);
  }

  throw error;
} finally {
  if (createdCollectionId) {
    try {
      await runGraphql(deleteMutation, { input: { id: createdCollectionId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }
}
