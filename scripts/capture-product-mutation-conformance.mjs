import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

const requiredVars = [
  'SHOPIFY_CONFORMANCE_STORE_DOMAIN',
  'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN',
  'SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN',
];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const adminAccessToken = process.env['SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN'];
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-mutation-conformance-scope-blocker.md');

function buildAdminAuthHeaders(token) {
  if (token.startsWith('shpat_')) {
    return {
      'X-Shopify-Access-Token': token,
    };
  }

  const bearerToken = token.startsWith('Bearer ') ? token : `Bearer ${token}`;
  return {
    Authorization: bearerToken,
    'X-Shopify-Access-Token': bearerToken,
  };
}

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

const productDetailQuery = `#graphql
  query ProductMutationConformanceDetail($id: ID!) {
    product(id: $id) {
      id
      title
      handle
      status
      vendor
      productType
      tags
      descriptionHtml
      templateSuffix
      seo {
        title
        description
      }
    }
  }
`;

const deletedProductLookupQuery = `#graphql
  query ProductMutationConformanceDeletedLookup($id: ID!, $query: String!) {
    product(id: $id) {
      id
      title
    }
    products(first: 5, query: $query) {
      edges {
        node {
          id
          title
          status
        }
      }
    }
  }
`;

const createMutation = `#graphql
  mutation ProductCreateConformance($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        vendor
        productType
        tags
        descriptionHtml
        templateSuffix
        seo {
          title
          description
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const updateMutation = `#graphql
  mutation ProductUpdateConformance($product: ProductUpdateInput!) {
    productUpdate(product: $product) {
      product {
        id
        title
        handle
        status
        vendor
        productType
        tags
        descriptionHtml
        templateSuffix
        seo {
          title
          description
        }
        onlineStorePreviewUrl
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation ProductDeleteConformance($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function buildCreateVariables(runId) {
  return {
    product: {
      title: `Hermes Product Conformance ${runId}`,
      status: 'DRAFT',
      vendor: 'HERMES',
      productType: 'ACCESSORIES',
      tags: ['conformance', 'product-mutation', runId],
      descriptionHtml: `<p>Hermes product mutation conformance ${runId}</p>`,
      templateSuffix: 'product-mutation-parity',
      seo: {
        title: `Hermes Product ${runId}`,
        description: `Hermes product mutation conformance ${runId}`,
      },
    },
  };
}

function buildUpdateVariables(productId, runId) {
  return {
    product: {
      id: productId,
      title: `Hermes Product Conformance ${runId} Updated`,
      vendor: 'HERMES-LABS',
      productType: 'TEST-GOODS',
      tags: ['conformance', 'product-mutation', `${runId}-updated`],
      descriptionHtml: `<p>Updated Hermes product mutation conformance ${runId}</p>`,
      templateSuffix: 'product-mutation-updated',
      seo: {
        title: `Hermes Product ${runId} Updated`,
        description: `Updated Hermes product mutation conformance ${runId}`,
      },
    },
  };
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Product mutation conformance blocker',
    whatFailed:
      'Attempted to capture live conformance for the staged product mutation family (`productCreate`, `productUpdate`, `productDelete`).',
    operations: ['productCreate', 'productUpdate', 'productDelete'],
    blocker,
    whyBlocked:
      'Without a write-capable token, the repo cannot capture successful live mutation payload shape, userErrors behavior for safe writes, or immediate downstream read-after-write parity for `productCreate`, `productUpdate`, and `productDelete`.',
    completedSteps: [
      'added a reusable live-write capture harness for the staged create/update/delete family',
      'kept the rich create/update payload slice aligned with the existing parity-request scaffolds so a future write-capable token can capture the same shapes directly',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with `write_products`, then rerun `corepack pnpm conformance:capture-product-mutations`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const createVariables = buildCreateVariables(runId);
let createdProductId = null;
let createResponse = null;
let updateResponse = null;
let deleteResponse = null;

try {
  createResponse = await runGraphql(createMutation, createVariables);
  createdProductId = createResponse.data?.productCreate?.product?.id ?? null;
  if (!createdProductId) {
    throw new Error('Product create capture did not return a product id.');
  }

  const postCreateDetail = await runGraphql(productDetailQuery, { id: createdProductId });
  const updateVariables = buildUpdateVariables(createdProductId, runId);
  updateResponse = await runGraphql(updateMutation, updateVariables);
  const postUpdateDetail = await runGraphql(productDetailQuery, { id: createdProductId });
  deleteResponse = await runGraphql(deleteMutation, { input: { id: createdProductId } });
  const postDeleteLookup = await runGraphql(deletedProductLookupQuery, {
    id: createdProductId,
    query: `title:${JSON.stringify(createVariables.product.title).slice(1, -1)}`,
  });
  createdProductId = null;

  const captures = {
    'product-create-parity.json': {
      mutation: {
        variables: createVariables,
        response: createResponse,
      },
      downstreamRead: postCreateDetail,
    },
    'product-update-parity.json': {
      mutation: {
        variables: updateVariables,
        response: updateResponse,
      },
      downstreamRead: postUpdateDetail,
    },
    'product-delete-parity.json': {
      mutation: {
        variables: { input: { id: deleteResponse.data?.productDelete?.deletedProductId ?? null } },
        response: deleteResponse,
      },
      downstreamRead: postDeleteLookup,
    },
  };

  for (const [filename, payload] of Object.entries(captures)) {
    await writeFile(path.join(outputDir, filename), `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  }

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: Object.keys(captures),
        productId: deleteResponse.data?.productDelete?.deletedProductId ?? null,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const blocker = parseWriteScopeBlocker(error?.result ?? null);
  if (blocker) {
    await writeScopeBlocker(blocker);
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
