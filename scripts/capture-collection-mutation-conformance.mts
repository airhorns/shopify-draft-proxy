// @ts-nocheck
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mts';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mts';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'collection-mutation-conformance-scope-blocker.md');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

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

const seedPublicationsQuery = `#graphql
  query CollectionPublicationSeed {
    publications(first: 10) {
      nodes {
        id
        name
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

const collectionPublicationDetailSlice = `
  id
  title
  handle
  publishedOnCurrentPublication
  publishedOnPublication(publicationId: $publicationId)
  availablePublicationsCount {
    count
    precision
  }
  resourcePublicationsCount {
    count
    precision
  }
`;

const collectionPublicationListSlice = `
  nodes {
    ${collectionPublicationDetailSlice}
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

const reorderProductsMutation = `#graphql
  mutation CollectionReorderProductsConformance($id: ID!, $moves: [MoveInput!]!) {
    collectionReorderProducts(id: $id, moves: $moves) {
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

const publicationReadQuery = `#graphql
  query CollectionPublicationRead($collectionId: ID!, $publicationId: ID!) {
    collection(id: $collectionId) {
      ${collectionPublicationDetailSlice}
    }
    publishedCollections: collections(first: 5, query: "published_status:published") {
      ${collectionPublicationListSlice}
    }
    unpublishedCollections: collections(first: 5, query: "published_status:unpublished") {
      ${collectionPublicationListSlice}
    }
  }
`;

const publishablePublishMutation = `#graphql
  mutation CollectionPublishablePublish($id: ID!, $input: [PublicationInput!]!, $publicationId: ID!) {
    publishablePublish(id: $id, input: $input) {
      publishable {
        ... on Collection {
          ${collectionPublicationDetailSlice}
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const publishableUnpublishMutation = `#graphql
  mutation CollectionPublishableUnpublish($id: ID!, $input: [PublicationInput!]!, $publicationId: ID!) {
    publishableUnpublish(id: $id, input: $input) {
      publishable {
        ... on Collection {
          ${collectionPublicationDetailSlice}
        }
      }
      userErrors {
        field
        message
      }
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

const postReorderReadQuery = `#graphql
  query CollectionReorderProductsDownstream($collectionId: ID!, $firstProductId: ID!, $secondProductId: ID!) {
    collection(id: $collectionId) {
      id
      title
      handle
      defaultProducts: products(first: 10, sortKey: COLLECTION_DEFAULT) {
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
      manualProducts: products(first: 10, sortKey: MANUAL) {
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
      sortOrder: 'MANUAL',
    },
  };
}

function pickPublication(payload) {
  const publications = payload?.data?.publications?.nodes;
  if (!Array.isArray(publications)) {
    throw new Error('Could not find publication nodes in collection publication seed payload.');
  }

  const publication = publications.find((node) => typeof node?.id === 'string');
  if (!publication) {
    throw new Error('Need at least one live publication to capture collection publication parity.');
  }

  return {
    id: publication.id,
    name: typeof publication.name === 'string' ? publication.name : null,
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
      'collectionReorderProducts',
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
const seedPublicationsResponse = await runGraphql(seedPublicationsQuery);
const publication = pickPublication(seedPublicationsResponse);
const createVariables = buildCreateVariables(runId);
let createdCollectionId = null;
let createResponse = null;
let publishResponse = null;
let unpublishResponse = null;
let addResponse = null;
let reorderResponse = null;
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
  const postCreatePublicationRead = await runGraphql(publicationReadQuery, {
    collectionId: createdCollectionId,
    publicationId: publication.id,
  });

  const publicationVariables = {
    id: createdCollectionId,
    input: [{ publicationId: publication.id }],
    publicationId: publication.id,
  };
  publishResponse = await runGraphql(publishablePublishMutation, publicationVariables);
  const postPublishRead = await runGraphql(publicationReadQuery, {
    collectionId: createdCollectionId,
    publicationId: publication.id,
  });

  unpublishResponse = await runGraphql(publishableUnpublishMutation, publicationVariables);
  const postUnpublishRead = await runGraphql(publicationReadQuery, {
    collectionId: createdCollectionId,
    publicationId: publication.id,
  });

  addResponse = await runGraphql(addProductsMutation, {
    id: createdCollectionId,
    productIds: [firstProduct.id, secondProduct.id],
  });
  const postAddReadVariables = {
    collectionId: createdCollectionId,
    firstProductId: firstProduct.id,
    secondProductId: secondProduct.id,
  };
  const postAddRead = await runGraphql(postAddReadQuery, postAddReadVariables);

  const reorderVariables = {
    id: createdCollectionId,
    moves: [
      {
        id: secondProduct.id,
        newPosition: '0',
      },
    ],
  };
  reorderResponse = await runGraphql(reorderProductsMutation, reorderVariables);
  const postReorderReadVariables = {
    collectionId: createdCollectionId,
    firstProductId: firstProduct.id,
    secondProductId: secondProduct.id,
  };
  const postReorderRead = await runGraphql(postReorderReadQuery, postReorderReadVariables);

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
      seedPublication: publication,
      mutation: {
        variables: createVariables,
        response: createResponse,
      },
      downstreamRead: postCreateRead,
    },
    'collection-publication-parity.json': {
      seedProducts: [firstProduct, secondProduct],
      seedPublication: publication,
      mutation: {
        variables: publicationVariables,
        response: publishResponse,
      },
      publishMutation: {
        variables: publicationVariables,
        response: publishResponse,
      },
      postCreatePublicationRead,
      postPublishRead,
      unpublishMutation: {
        variables: publicationVariables,
        response: unpublishResponse,
      },
      postUnpublishRead,
    },
    'collection-add-products-parity.json': {
      seedProducts: [firstProduct, secondProduct],
      seedPublication: publication,
      mutation: {
        variables: {
          id: addResponse.data?.collectionAddProducts?.collection?.id ?? createdCollectionId,
          productIds: [firstProduct.id, secondProduct.id],
        },
        response: addResponse,
      },
      downstreamReadVariables: postAddReadVariables,
      downstreamRead: postAddRead,
    },
    'collection-reorder-products-parity.json': {
      seedProducts: [firstProduct, secondProduct],
      initialCollectionRead: postAddRead,
      mutation: {
        variables: reorderVariables,
        response: reorderResponse,
      },
      downstreamReadVariables: postReorderReadVariables,
      downstreamRead: postReorderRead,
    },
    'collection-update-parity.json': {
      seedProducts: [firstProduct, secondProduct],
      seedPublication: publication,
      mutation: {
        variables: updateVariables,
        response: updateResponse,
      },
      downstreamRead: postUpdateRead,
    },
    'collection-remove-products-parity.json': {
      seedProducts: [firstProduct, secondProduct],
      seedPublication: publication,
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
      seedPublication: publication,
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
        seedPublication: publication,
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
