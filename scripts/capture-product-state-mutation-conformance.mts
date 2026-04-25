// @ts-nocheck
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { runAdminGraphql, runAdminGraphqlRequest } from './conformance-graphql-client.js';
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
const blockerPath = path.join(pendingDir, 'product-state-mutation-conformance-scope-blocker.md');
const tagSearchIndexWaitMs = 10_000;

function sleep(ms: number) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function postGraphql(query, variables = {}) {
  return runAdminGraphqlRequest(
    { adminOrigin, apiVersion, headers: buildAdminAuthHeaders(adminAccessToken) },
    query,
    variables,
  );
}

async function runGraphql(query, variables = {}) {
  return runAdminGraphql(
    { adminOrigin, apiVersion, headers: buildAdminAuthHeaders(adminAccessToken) },
    query,
    variables,
  );
}

async function runGraphqlAllowErrors(query, variables = {}) {
  const { payload } = await postGraphql(query, variables);
  return payload;
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

const postStatusReadQuery = `#graphql
  query ProductChangeStatusDownstream($id: ID!, $query: String!) {
    product(id: $id) {
      id
      status
      updatedAt
    }
    products(first: 10, query: $query) {
      nodes {
        id
        status
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
      nodes {
        id
        tags
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
      nodes {
        id
        tags
      }
    }
    removed: products(first: 10, query: $removedQuery) {
      nodes {
        id
        tags
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
      tags: ['existing'],
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
let createdProductId = null;
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

  statusVariables.productId = createdProductId;
  tagsAddVariables.id = createdProductId;
  tagsRemoveVariables.id = createdProductId;

  statusResponse = await runGraphql(changeStatusMutation, statusVariables);
  unknownStatusResponse = await runGraphql(changeStatusMutation, unknownStatusVariables);
  nullLiteralStatusResponse = await runGraphqlAllowErrors(changeStatusNullLiteralMutation);
  const postStatusRead = await runGraphql(postStatusReadQuery, {
    id: createdProductId,
    query: 'status:archived',
  });

  tagsAddResponse = await runGraphql(tagsAddMutation, tagsAddVariables);
  const tagsAddDownstreamReadVariables = {
    id: createdProductId,
    query: `tag:${uniqueSaleTag}`,
  };
  const postTagsAddRead = await runGraphql(postTagsAddReadQuery, tagsAddDownstreamReadVariables);
  await sleep(tagSearchIndexWaitMs);
  const postTagsAddDelayedRead = await runGraphql(postTagsAddReadQuery, tagsAddDownstreamReadVariables);

  tagsRemoveResponse = await runGraphql(tagsRemoveMutation, tagsRemoveVariables);
  const tagsRemoveDownstreamReadVariables = {
    id: createdProductId,
    remainingQuery: `tag:${uniqueSummerTag}`,
    removedQuery: `tag:${uniqueSaleTag}`,
  };
  const postTagsRemoveRead = await runGraphql(postTagsRemoveReadQuery, tagsRemoveDownstreamReadVariables);

  const captures = {
    'product-change-status-parity.json': {
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
      downstreamRead: postStatusRead,
    },
    'tags-add-parity.json': {
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
    },
    'tags-remove-parity.json': {
      mutation: {
        variables: tagsRemoveVariables,
        response: tagsRemoveResponse,
      },
      downstreamReadVariables: tagsRemoveDownstreamReadVariables,
      downstreamRead: postTagsRemoveRead,
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
  if (createdProductId) {
    try {
      await runGraphql(deleteMutation, { input: { id: createdProductId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }
}
