// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { runAdminGraphql } from './conformance-graphql-client.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-variant-mutation-conformance-scope-blocker.md');

async function runGraphql(query, variables = {}) {
  return runAdminGraphql(
    { adminOrigin, apiVersion, headers: buildAdminAuthHeaders(adminAccessToken) },
    query,
    variables,
  );
}

function expectNoUserErrors(pathLabel, userErrors) {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${pathLabel} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

const createProductMutation = `#graphql
  mutation ProductVariantConformanceCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const setupOptionsMutation = `#graphql
  mutation ProductVariantConformanceSetupOptions($productId: ID!, $options: [OptionCreateInput!]!) {
    productOptionsCreate(productId: $productId, options: $options) {
      product {
        id
        options {
          id
          name
          position
          values
          optionValues {
            id
            name
            hasVariants
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

const initialVariantQuery = `#graphql
  query ProductVariantConformanceDefaultVariant($id: ID!) {
    product(id: $id) {
      id
      variants(first: 10) {
        nodes {
          id
          title
          sku
          inventoryQuantity
          selectedOptions {
            name
            value
          }
        }
      }
    }
  }
`;

const bulkUpdateMutation = `#graphql
  mutation ProductVariantsBulkUpdateParityPlan($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        totalInventory
        tracksInventory
        variants(first: 10) {
          nodes {
            id
            title
            sku
            barcode
            price
            compareAtPrice
            taxable
            inventoryPolicy
            inventoryQuantity
            inventoryItem {
              id
              tracked
              requiresShipping
            }
          }
        }
      }
      productVariants {
        id
        title
        sku
        barcode
        price
        compareAtPrice
        taxable
        inventoryPolicy
        inventoryQuantity
        inventoryItem {
          id
          tracked
          requiresShipping
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const bulkCreateMutation = `#graphql
  mutation ProductVariantsBulkCreateParityPlan($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkCreate(productId: $productId, variants: $variants) {
      product {
        id
        totalInventory
        tracksInventory
        variants(first: 10) {
          nodes {
            id
            title
            sku
            barcode
            price
            inventoryQuantity
            selectedOptions {
              name
              value
            }
            inventoryItem {
              id
              tracked
              requiresShipping
            }
          }
        }
      }
      productVariants {
        id
        title
        sku
        barcode
        price
        inventoryQuantity
        selectedOptions {
          name
          value
        }
        inventoryItem {
          id
          tracked
          requiresShipping
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const bulkDeleteMutation = `#graphql
  mutation ProductVariantsBulkDeleteParityPlan($productId: ID!, $variantsIds: [ID!]!) {
    productVariantsBulkDelete(productId: $productId, variantsIds: $variantsIds) {
      product {
        id
        totalInventory
        tracksInventory
        variants(first: 10) {
          nodes {
            id
            title
            sku
            inventoryQuantity
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

const bulkDownstreamQuery = `#graphql
  query ProductVariantBulkConformanceRead($id: ID!, $query: String!) {
    product(id: $id) {
      id
      totalInventory
      tracksInventory
      variants(first: 10) {
        nodes {
          id
          title
          sku
          barcode
          price
          compareAtPrice
          taxable
          inventoryPolicy
          inventoryQuantity
          selectedOptions {
            name
            value
          }
          inventoryItem {
            id
            tracked
            requiresShipping
          }
        }
      }
    }
    products(first: 10, query: $query) {
      nodes {
        id
        totalInventory
        tracksInventory
      }
    }
    skuCount: productsCount(query: $query) {
      count
      precision
    }
  }
`;

function buildProductVariables(title) {
  return {
    product: {
      title,
      status: 'DRAFT',
    },
  };
}

function buildBulkUpdateVariables(productId, defaultVariantId, skuPrefix) {
  return {
    productId,
    variants: [
      {
        id: defaultVariantId,
        barcode: '1111111111111',
        price: '24.00',
        compareAtPrice: '30.00',
        taxable: true,
        inventoryPolicy: 'DENY',
        inventoryItem: {
          sku: `${skuPrefix}-RED`,
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  };
}

function buildBulkCreateVariables(productId, skuPrefix) {
  return {
    productId,
    variants: [
      {
        optionValues: [{ optionName: 'Color', name: 'Blue' }],
        barcode: '2222222222222',
        price: '26.00',
        inventoryItem: {
          sku: `${skuPrefix}-BLUE`,
          tracked: true,
          requiresShipping: false,
        },
      },
    ],
  };
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Product variant bulk-mutation conformance blocker',
    whatFailed: 'Attempted to capture live conformance for the staged bulk product variant mutation family.',
    operations: ['productVariantsBulkCreate', 'productVariantsBulkUpdate', 'productVariantsBulkDelete'],
    blocker,
    whyBlocked:
      'Without a write-capable token, the repo cannot capture successful live mutation payload shape, userErrors behavior, or immediate downstream variant/search/count parity for the bulk variant mutation family.',
    completedSteps: [
      'added a reusable live-write capture harness for the bulk variant mutation family',
      "aligned the proxy request scaffolds with Shopify's current 2025-01 input shape (`inventoryItem.sku`, `inventoryQuantities`, and `optionValues`) so later reruns compare the real payloads directly",
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with `write_products`, then rerun `corepack pnpm conformance:capture-product-variant-mutations`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const skuPrefix = `HERMES-BULK-${runId.slice(-6)}`;
const productVariables = buildProductVariables(`Hermes Variant Bulk Conformance ${runId}`);
let productId = null;

try {
  const createProductResponse = await runGraphql(createProductMutation, productVariables);
  expectNoUserErrors('productCreate (bulk seed)', createProductResponse.data?.productCreate?.userErrors);
  productId = createProductResponse.data?.productCreate?.product?.id ?? null;
  if (!productId) {
    throw new Error('Bulk seed product creation did not return a product id.');
  }

  const setupOptionsResponse = await runGraphql(setupOptionsMutation, {
    productId,
    options: [{ name: 'Color', values: [{ name: 'Red' }, { name: 'Blue' }] }],
  });
  expectNoUserErrors(
    'productOptionsCreate (bulk seed setup)',
    setupOptionsResponse.data?.productOptionsCreate?.userErrors,
  );

  const initialState = await runGraphql(initialVariantQuery, { id: productId });
  const defaultVariantId = initialState.data?.product?.variants?.nodes?.[0]?.id ?? null;
  if (!defaultVariantId) {
    throw new Error('Bulk seed product did not expose a default variant id after option setup.');
  }

  const bulkUpdateVariables = buildBulkUpdateVariables(productId, defaultVariantId, skuPrefix);
  const bulkUpdateResponse = await runGraphql(bulkUpdateMutation, bulkUpdateVariables);
  expectNoUserErrors('productVariantsBulkUpdate', bulkUpdateResponse.data?.productVariantsBulkUpdate?.userErrors);
  const bulkUpdateRead = await runGraphql(bulkDownstreamQuery, {
    id: productId,
    query: `sku:${bulkUpdateVariables.variants[0].inventoryItem.sku}`,
  });

  const bulkCreateVariables = buildBulkCreateVariables(productId, skuPrefix);
  const bulkCreateResponse = await runGraphql(bulkCreateMutation, bulkCreateVariables);
  expectNoUserErrors('productVariantsBulkCreate', bulkCreateResponse.data?.productVariantsBulkCreate?.userErrors);
  const bulkCreateRead = await runGraphql(bulkDownstreamQuery, {
    id: productId,
    query: `sku:${bulkCreateVariables.variants[0].inventoryItem.sku}`,
  });

  const bulkDeleteVariables = {
    productId,
    variantsIds: [defaultVariantId],
  };
  const bulkDeleteResponse = await runGraphql(bulkDeleteMutation, bulkDeleteVariables);
  expectNoUserErrors('productVariantsBulkDelete', bulkDeleteResponse.data?.productVariantsBulkDelete?.userErrors);
  const bulkDeleteRead = await runGraphql(bulkDownstreamQuery, {
    id: productId,
    query: `sku:${bulkUpdateVariables.variants[0].inventoryItem.sku}`,
  });

  const captures = {
    'product-variants-bulk-update-parity.json': {
      mutation: {
        variables: bulkUpdateVariables,
        response: bulkUpdateResponse,
      },
      downstreamRead: bulkUpdateRead,
    },
    'product-variants-bulk-create-parity.json': {
      mutation: {
        variables: bulkCreateVariables,
        response: bulkCreateResponse,
      },
      downstreamRead: bulkCreateRead,
    },
    'product-variants-bulk-delete-parity.json': {
      mutation: {
        variables: bulkDeleteVariables,
        response: bulkDeleteResponse,
      },
      downstreamRead: bulkDeleteRead,
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
        productId,
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
  if (productId) {
    try {
      await runGraphql(
        `#graphql
          mutation ProductVariantConformanceCleanup($input: ProductDeleteInput!) {
            productDelete(input: $input) {
              deletedProductId
              userErrors {
                field
                message
              }
            }
          }
        `,
        { input: { id: productId } },
      );
    } catch {
      // Best-effort cleanup only. Surface the original failure instead.
    }
  }
}
