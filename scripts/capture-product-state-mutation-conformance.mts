// @ts-nocheck
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-state-mutation-conformance-scope-blocker.md');
const tagSearchIndexWaitMs = Number(process.env.SHOPIFY_CONFORMANCE_SEARCH_INDEX_WAIT_MS ?? '60000');
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function sleep(ms: number) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function runGraphqlAllowErrors(query, variables = {}) {
  const { payload } = await runGraphqlRequest(query, variables);
  return payload;
}

const taggableProductHydrateQuery =
  '\nquery ProductsHydrateNodes($ids: [ID!]!) {\n  nodes(ids: $ids) {\n    __typename\n    id\n    ... on Product {\n      legacyResourceId\n      title\n      handle\n      status\n      vendor\n      productType\n      tags\n      totalInventory\n      tracksInventory\n      createdAt\n      updatedAt\n      publishedAt\n      descriptionHtml\n      onlineStorePreviewUrl\n      templateSuffix\n      seo { title description }\n      availablePublicationsCount { count precision }\n      resourcePublicationsCount { count precision }\n      resourcePublicationsV2(first: 10) { nodes { publication { id } publishDate isPublished } }\n      publications(first: 10) { nodes { isPublished publishDate product { id } } }\n    }\n  }\n}';

async function captureProductHydrateUpstreamCall(productId) {
  const variables = { ids: [productId] };
  const { status, payload } = await runGraphqlRequest(taggableProductHydrateQuery, variables);
  if (status < 200 || status >= 300 || payload.errors) {
    throw new Error(`Product hydrate cassette capture failed: ${JSON.stringify({ status, payload }, null, 2)}`);
  }

  return {
    operationName: 'ProductsHydrateNodes',
    variables,
    query: taggableProductHydrateQuery,
    response: {
      status,
      body: payload,
    },
  };
}

const createMutation = `#graphql
  mutation ProductStateConformanceCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        tags
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation ProductStateConformanceDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const changeStatusMutation = `#graphql
  mutation ProductChangeStatusConformance($productId: ID!, $status: ProductStatus!) {
    productChangeStatus(productId: $productId, status: $status) {
      product {
        id
        status
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const changeStatusNullLiteralMutation = `#graphql
  mutation ProductChangeStatusNullLiteralConformance {
    productChangeStatus(productId: null, status: ARCHIVED) {
      product {
        id
        status
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const tagsAddMutation = `#graphql
  mutation TagsAddConformance($id: ID!, $tags: [String!]!) {
    tagsAdd(id: $id, tags: $tags) {
      node {
        ... on Product {
          id
          tags
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const tagsRemoveMutation = `#graphql
  mutation TagsRemoveConformance($id: ID!, $tags: [String!]!) {
    tagsRemove(id: $id, tags: $tags) {
      node {
        ... on Product {
          id
          tags
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const tagsAddCommaStringMutation = `#graphql
  mutation TagsAddCommaString($id: ID!, $tags: [String!]!) {
    tagsAdd(id: $id, tags: $tags) {
      node {
        ... on Product {
          id
          tags
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const tagsAddCommaListElementMutation = `#graphql
  mutation TagsAddCommaListElement($id: ID!, $tags: [String!]!) {
    tagsAdd(id: $id, tags: $tags) {
      node {
        ... on Product {
          id
          tags
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const tagsAddCaseVariantMutation = `#graphql
  mutation TagsAddCaseVariant($id: ID!, $tags: [String!]!) {
    tagsAdd(id: $id, tags: $tags) {
      node {
        ... on Product {
          id
          tags
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const tagsRemoveCaseVariantMutation = `#graphql
  mutation TagsRemoveCaseVariant($id: ID!, $tags: [String!]!) {
    tagsRemove(id: $id, tags: $tags) {
      node {
        ... on Product {
          id
          tags
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const tagsRemoveStringMutation = `#graphql
  mutation TagsRemoveString($id: ID!, $tags: [String!]!) {
    tagsRemove(id: $id, tags: $tags) {
      node {
        ... on Product {
          id
          tags
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const postStatusReadQuery = `#graphql
  query ProductChangeStatusDownstream($id: ID!, $query: String!) {
    product(id: $id) {
      id
      status
      updatedAt
    }
    products(first: 10, query: $query) {
      edges {
        cursor
        node {
          id
          status
        }
      }
      nodes {
        id
        status
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    productsCount(query: $query) {
      count
      precision
    }
  }
`;

const postTagsAddReadQuery = `#graphql
  query TagsAddDownstream($id: ID!, $query: String!) {
    product(id: $id) {
      id
      tags
    }
    products(first: 10, query: $query) {
      edges {
        cursor
        node {
          id
          tags
        }
      }
      nodes {
        id
        tags
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    productsCount(query: $query) {
      count
      precision
    }
  }
`;

const postTagsRemoveReadQuery = `#graphql
  query TagsRemoveDownstream($id: ID!, $remainingQuery: String!, $removedQuery: String!) {
    product(id: $id) {
      id
      tags
    }
    remaining: products(first: 10, query: $remainingQuery) {
      edges {
        cursor
        node {
          id
          tags
        }
      }
      nodes {
        id
        tags
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    removed: products(first: 10, query: $removedQuery) {
      edges {
        cursor
        node {
          id
          tags
        }
      }
      nodes {
        id
        tags
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    remainingCount: productsCount(query: $remainingQuery) {
      count
      precision
    }
    removedCount: productsCount(query: $removedQuery) {
      count
      precision
    }
  }
`;

function buildCreateVariables(runId) {
  return {
    product: {
      title: `Hermes Product State Conformance ${runId}`,
      status: 'DRAFT',
      tags: ['existing', `hermes-state-${runId}`],
    },
  };
}

function buildTagNormalizationCreateVariables(runId, label) {
  return {
    product: {
      title: `Hermes Product Tag Normalization ${label} ${runId}`,
      status: 'DRAFT',
      tags: ['Red'],
    },
  };
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Product state mutation conformance blocker',
    whatFailed:
      'Attempted to capture live conformance for the staged product state mutation family (`productChangeStatus`, `tagsAdd`, `tagsRemove`).',
    operations: ['productChangeStatus', 'tagsAdd', 'tagsRemove'],
    blocker,
    whyBlocked:
      'Without a write-capable token, the repo cannot capture successful live payload shape, userErrors behavior, or immediate downstream status/tag filter parity for these product state mutations.',
    completedSteps: [
      'added a reusable live-write capture harness for staged product status and tag mutations',
      'aligned the mutation and downstream read slices with the existing parity-request scaffolds so future runs capture the same merchant-relevant shapes directly',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with product write permissions, then rerun `tsx ./scripts/capture-product-state-mutation-conformance.mts` (or a packaged wrapper script if added).',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const uniqueSummerTag = `hermes-summer-${runId}`;
const uniqueSaleTag = `hermes-sale-${runId}`;
const createVariables = buildCreateVariables(runId);
const statusVariables = { productId: null, status: 'ARCHIVED' };
const unknownStatusVariables = { productId: 'gid://shopify/Product/999999999999999', status: 'ARCHIVED' };
const tagsAddVariables = { id: null, tags: ['existing', uniqueSummerTag, uniqueSaleTag] };
const tagsRemoveVariables = { id: null, tags: [uniqueSaleTag, 'missing'] };
const tagNormalizationCases = [
  {
    key: 'commaStringAdd',
    createVariables: buildTagNormalizationCreateVariables(runId, 'Comma String'),
    mutation: tagsAddCommaStringMutation,
    variables: { id: null, tags: 'blue, green' },
  },
  {
    key: 'commaListElementAdd',
    createVariables: buildTagNormalizationCreateVariables(runId, 'Comma List Element'),
    mutation: tagsAddCommaListElementMutation,
    variables: { id: null, tags: ['blue,green'] },
  },
  {
    key: 'caseVariantAdd',
    createVariables: buildTagNormalizationCreateVariables(runId, 'Case Variant Add'),
    mutation: tagsAddCaseVariantMutation,
    variables: { id: null, tags: ['red'] },
  },
  {
    key: 'caseVariantRemove',
    createVariables: buildTagNormalizationCreateVariables(runId, 'Case Variant Remove'),
    mutation: tagsRemoveCaseVariantMutation,
    variables: { id: null, tags: ['red'] },
  },
  {
    key: 'singleStringRemove',
    createVariables: buildTagNormalizationCreateVariables(runId, 'Single String Remove'),
    mutation: tagsRemoveStringMutation,
    variables: { id: null, tags: 'Red' },
  },
];
let createdProductId = null;
const cleanupProductIds = [];
let createResponse = null;
let statusResponse = null;
let unknownStatusResponse = null;
let nullLiteralStatusResponse = null;
let tagsAddResponse = null;
let tagsRemoveResponse = null;

try {
  createResponse = await runGraphql(createMutation, createVariables);
  createdProductId = createResponse.data?.productCreate?.product?.id ?? null;
  if (!createdProductId) {
    throw new Error('Product state mutation capture did not return a product id.');
  }
  cleanupProductIds.push(createdProductId);

  statusVariables.productId = createdProductId;
  tagsAddVariables.id = createdProductId;
  tagsRemoveVariables.id = createdProductId;

  const statusHydrateCall = await captureProductHydrateUpstreamCall(createdProductId);
  statusResponse = await runGraphql(changeStatusMutation, statusVariables);
  unknownStatusResponse = await runGraphql(changeStatusMutation, unknownStatusVariables);
  nullLiteralStatusResponse = await runGraphqlAllowErrors(changeStatusNullLiteralMutation);
  const postStatusReadVariables = {
    id: createdProductId,
    query: `status:archived tag:hermes-state-${runId}`,
  };
  const postStatusRead = await runGraphql(postStatusReadQuery, postStatusReadVariables);
  await sleep(tagSearchIndexWaitMs);
  const postStatusDelayedRead = await runGraphql(postStatusReadQuery, postStatusReadVariables);

  const tagsAddHydrateCall = await captureProductHydrateUpstreamCall(createdProductId);
  tagsAddResponse = await runGraphql(tagsAddMutation, tagsAddVariables);
  const tagsAddDownstreamReadVariables = {
    id: createdProductId,
    query: `tag:${uniqueSaleTag}`,
  };
  const postTagsAddRead = await runGraphql(postTagsAddReadQuery, tagsAddDownstreamReadVariables);
  await sleep(tagSearchIndexWaitMs);
  const postTagsAddDelayedRead = await runGraphql(postTagsAddReadQuery, tagsAddDownstreamReadVariables);

  const tagsRemoveHydrateCall = await captureProductHydrateUpstreamCall(createdProductId);
  tagsRemoveResponse = await runGraphql(tagsRemoveMutation, tagsRemoveVariables);
  const tagsRemoveDownstreamReadVariables = {
    id: createdProductId,
    remainingQuery: `tag:${uniqueSummerTag}`,
    removedQuery: `tag:${uniqueSaleTag}`,
  };
  const postTagsRemoveRead = await runGraphql(postTagsRemoveReadQuery, tagsRemoveDownstreamReadVariables);
  await sleep(tagSearchIndexWaitMs);
  const postTagsRemoveDelayedRead = await runGraphql(postTagsRemoveReadQuery, tagsRemoveDownstreamReadVariables);

  const tagNormalization = {};
  for (const scenario of tagNormalizationCases) {
    const setupResponse = await runGraphql(createMutation, scenario.createVariables);
    const productId = setupResponse.data?.productCreate?.product?.id ?? null;
    if (!productId) {
      throw new Error(`Product tag normalization capture did not return a product id for ${scenario.key}.`);
    }
    cleanupProductIds.push(productId);
    const mutationVariables = { ...scenario.variables, id: productId };
    const mutationResponse = await runGraphql(scenario.mutation, mutationVariables);
    tagNormalization[scenario.key] = {
      seedProduct: setupResponse.data?.productCreate?.product ?? null,
      setup: {
        variables: scenario.createVariables,
        response: setupResponse,
      },
      mutation: {
        query: scenario.mutation,
        variables: mutationVariables,
        response: mutationResponse,
      },
    };
  }

  const captures = {
    'product-change-status-parity.json': {
      seedProduct: createResponse.data?.productCreate?.product ?? null,
      mutation: {
        variables: statusVariables,
        response: statusResponse,
      },
      validation: {
        unknownProduct: {
          variables: unknownStatusVariables,
          response: unknownStatusResponse,
        },
        nullLiteralProductId: {
          query: changeStatusNullLiteralMutation,
          response: nullLiteralStatusResponse,
        },
      },
      downstreamReadVariables: postStatusReadVariables,
      downstreamRead: postStatusRead,
      delayedDownstreamRead: {
        waitMs: tagSearchIndexWaitMs,
        variables: postStatusReadVariables,
        response: postStatusDelayedRead,
      },
      upstreamCalls: [statusHydrateCall],
    },
    'tags-add-parity.json': {
      seedProduct: createResponse.data?.productCreate?.product ?? null,
      mutation: {
        variables: tagsAddVariables,
        response: tagsAddResponse,
      },
      downstreamReadVariables: tagsAddDownstreamReadVariables,
      downstreamRead: postTagsAddRead,
      delayedDownstreamRead: {
        waitMs: tagSearchIndexWaitMs,
        variables: tagsAddDownstreamReadVariables,
        response: postTagsAddDelayedRead,
      },
      upstreamCalls: [tagsAddHydrateCall],
    },
    'tags-remove-parity.json': {
      seedProduct: createResponse.data?.productCreate?.product ?? null,
      mutation: {
        variables: tagsRemoveVariables,
        response: tagsRemoveResponse,
      },
      downstreamReadVariables: tagsRemoveDownstreamReadVariables,
      downstreamRead: postTagsRemoveRead,
      delayedDownstreamRead: {
        waitMs: tagSearchIndexWaitMs,
        variables: tagsRemoveDownstreamReadVariables,
        response: postTagsRemoveDelayedRead,
      },
      upstreamCalls: [tagsRemoveHydrateCall],
    },
    'tags-normalization-parity.json': {
      tagNormalization,
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
        productId: createdProductId,
        cleanupProductIds,
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
  for (const productId of cleanupProductIds.reverse()) {
    try {
      await runGraphql(deleteMutation, { input: { id: productId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }
}
