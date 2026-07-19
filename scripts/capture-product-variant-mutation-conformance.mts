// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-variant-mutation-conformance-scope-blocker.md');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

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

const productMutationPreflightHydrateQuery = await readFile(
  'config/parity-requests/products/product-mutation-preflight-hydrate.graphql',
  'utf8',
);

const bulkUpdateMutation = await readFile(
  'config/parity-requests/products/productVariantsBulkUpdate-parity-plan.graphql',
  'utf8',
);

const bulkCreateMutation = await readFile(
  'config/parity-requests/products/productVariantsBulkCreate-parity-plan.graphql',
  'utf8',
);

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

const bulkUpdateDownstreamQuery = await readFile(
  'config/parity-requests/products/productVariantsBulkUpdate-downstream-read.graphql',
  'utf8',
);

const bulkCreateDownstreamQuery = await readFile(
  'config/parity-requests/products/productVariantsBulkCreate-downstream-read.graphql',
  'utf8',
);

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
        taxable: false,
        taxCode: 'P0000000',
        requiresComponents: true,
        showUnitPrice: true,
        unitPriceMeasurement: {
          quantityValue: 1.5,
          quantityUnit: 'L',
          referenceValue: 1,
          referenceUnit: 'L',
        },
        inventoryPolicy: 'DENY',
        inventoryItem: {
          sku: `${skuPrefix}-RED`,
          tracked: true,
          requiresShipping: false,
          countryCodeOfOrigin: 'US',
          provinceCodeOfOrigin: 'CA',
          harmonizedSystemCode: '1234.56',
          measurement: {
            weight: {
              value: 0.5,
              unit: 'KILOGRAMS',
            },
          },
        },
        metafields: [
          {
            namespace: 'specs',
            key: 'bulkUpdateTier',
            type: 'single_line_text_field',
            value: 'premium',
          },
        ],
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
        compareAtPrice: '30.00',
        taxable: false,
        taxCode: 'P0000000',
        requiresComponents: true,
        showUnitPrice: true,
        unitPriceMeasurement: {
          quantityValue: 2.5,
          quantityUnit: 'L',
          referenceValue: 1,
          referenceUnit: 'L',
        },
        inventoryItem: {
          sku: `${skuPrefix}-BLUE`,
          tracked: true,
          requiresShipping: false,
          countryCodeOfOrigin: 'US',
          provinceCodeOfOrigin: 'CA',
          harmonizedSystemCode: '1234.56',
          measurement: {
            weight: {
              value: 0.25,
              unit: 'KILOGRAMS',
            },
          },
        },
        metafields: [
          {
            namespace: 'specs',
            key: 'bulkCreateTier',
            type: 'single_line_text_field',
            value: 'standard',
          },
        ],
      },
    ],
  };
}

async function recordProductHydrationCall(productId) {
  const variables = {
    id: productId,
    variantsAfter: null,
    mediaAfter: null,
    collectionsAfter: null,
  };
  const response = await runGraphqlRaw(productMutationPreflightHydrateQuery, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`ProductMutationPreflightHydrate failed: ${JSON.stringify(response, null, 2)}`);
  }
  return {
    operationName: 'ProductMutationPreflightHydrate',
    variables,
    query: productMutationPreflightHydrateQuery,
    response: {
      status: response.status,
      body: response.payload,
    },
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
  const bulkUpdateHydrationCall = await recordProductHydrationCall(productId);
  const bulkUpdateResponse = await runGraphql(bulkUpdateMutation, bulkUpdateVariables);
  expectNoUserErrors('productVariantsBulkUpdate', bulkUpdateResponse.data?.productVariantsBulkUpdate?.userErrors);
  const bulkUpdateReadVariables = {
    id: productId,
    query: `sku:${bulkUpdateVariables.variants[0].inventoryItem.sku}`,
  };
  const bulkUpdateRead = await runGraphql(bulkUpdateDownstreamQuery, bulkUpdateReadVariables);

  const bulkCreateVariables = buildBulkCreateVariables(productId, skuPrefix);
  const bulkCreateHydrationCall = await recordProductHydrationCall(productId);
  const bulkCreateResponse = await runGraphql(bulkCreateMutation, bulkCreateVariables);
  expectNoUserErrors('productVariantsBulkCreate', bulkCreateResponse.data?.productVariantsBulkCreate?.userErrors);
  const bulkCreateReadVariables = {
    id: productId,
    query: `sku:${bulkCreateVariables.variants[0].inventoryItem.sku}`,
  };
  const bulkCreateRead = await runGraphql(bulkCreateDownstreamQuery, bulkCreateReadVariables);

  const bulkDeleteVariables = {
    productId,
    variantsIds: [defaultVariantId],
  };
  const bulkDeleteHydrationCall = await recordProductHydrationCall(productId);
  const bulkDeleteResponse = await runGraphql(bulkDeleteMutation, bulkDeleteVariables);
  expectNoUserErrors('productVariantsBulkDelete', bulkDeleteResponse.data?.productVariantsBulkDelete?.userErrors);
  const bulkDeleteReadVariables = {
    id: productId,
    query: `sku:${bulkUpdateVariables.variants[0].inventoryItem.sku}`,
  };
  const bulkDeleteRead = await runGraphql(bulkUpdateDownstreamQuery, bulkDeleteReadVariables);

  const captures = {
    'product-variants-bulk-update-parity.json': {
      mutation: {
        variables: bulkUpdateVariables,
        response: bulkUpdateResponse,
      },
      downstreamRead: {
        requestVariables: bulkUpdateReadVariables,
        ...bulkUpdateRead,
      },
      upstreamCalls: [bulkUpdateHydrationCall],
    },
    'product-variants-bulk-create-parity.json': {
      mutation: {
        variables: bulkCreateVariables,
        response: bulkCreateResponse,
      },
      downstreamRead: {
        requestVariables: bulkCreateReadVariables,
        ...bulkCreateRead,
      },
      upstreamCalls: [bulkCreateHydrationCall],
    },
    'product-variants-bulk-delete-parity.json': {
      mutation: {
        variables: bulkDeleteVariables,
        response: bulkDeleteResponse,
      },
      downstreamRead: {
        requestVariables: bulkDeleteReadVariables,
        response: bulkDeleteRead,
      },
      upstreamCalls: [bulkDeleteHydrationCall],
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
