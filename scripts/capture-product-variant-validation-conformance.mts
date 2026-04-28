// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-variants-bulk-validation-atomicity.json');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createProductMutation = `#graphql
  mutation ProductVariantValidationCreateProduct($product: ProductCreateInput!) {
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

const deleteProductMutation = `#graphql
  mutation ProductVariantValidationDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const setupOptionsMutation = `#graphql
  mutation ProductVariantValidationSetupOptions($productId: ID!, $options: [OptionCreateInput!]!) {
    productOptionsCreate(productId: $productId, options: $options) {
      product {
        id
        options {
          id
          name
          values
          optionValues {
            id
            name
            hasVariants
          }
        }
        variants(first: 10) {
          nodes {
            id
            title
          }
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productStateQuery = `#graphql
  query ProductVariantValidationState($id: ID!) {
    product(id: $id) {
      id
      totalInventory
      tracksInventory
      options {
        name
        values
        optionValues {
          name
          hasVariants
        }
      }
      variants(first: 20) {
        nodes {
          id
          title
          sku
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
  }
`;

const bulkCreateMutation = `#graphql
  mutation ProductVariantValidationBulkCreate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkCreate(productId: $productId, variants: $variants) {
      product {
        id
        totalInventory
        tracksInventory
      }
      productVariants {
        id
        title
        sku
        inventoryQuantity
        selectedOptions {
          name
          value
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const bulkUpdateMutation = `#graphql
  mutation ProductVariantValidationBulkUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        totalInventory
        tracksInventory
      }
      productVariants {
        id
        title
        sku
        inventoryQuantity
        selectedOptions {
          name
          value
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const bulkDeleteMutation = `#graphql
  mutation ProductVariantValidationBulkDelete($productId: ID!, $variantsIds: [ID!]!) {
    productVariantsBulkDelete(productId: $productId, variantsIds: $variantsIds) {
      product {
        id
        totalInventory
        tracksInventory
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function stableJson(value) {
  return JSON.stringify(value);
}

async function readProductState(productId) {
  return await runGraphql(productStateQuery, { id: productId });
}

async function captureCase({ name, query, variables, productId }) {
  const before = await readProductState(productId);
  const response = await runGraphqlRaw(query, variables);
  const after = await readProductState(productId);

  return {
    name,
    request: {
      variables,
    },
    response,
    atomicNoWrite: stableJson(before.data?.product ?? null) === stableJson(after.data?.product ?? null),
    before: before.data,
    after: after.data,
  };
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const skuPrefix = `HAR290-${runId.slice(-6)}`;
const createProductResponse = await runGraphql(createProductMutation, {
  product: {
    title: `HAR-290 Bulk Variant Validation ${runId}`,
    status: 'DRAFT',
  },
});
const productId = createProductResponse.data?.productCreate?.product?.id ?? null;
if (!productId) {
  throw new Error(
    `Could not create product variant validation seed: ${JSON.stringify(createProductResponse, null, 2)}`,
  );
}

try {
  const setupOptionsResponse = await runGraphql(setupOptionsMutation, {
    productId,
    options: [
      { name: 'Color', values: [{ name: 'Red' }, { name: 'Blue' }] },
      { name: 'Size', values: [{ name: 'Small' }, { name: 'Large' }] },
    ],
  });
  const setupUserErrors = setupOptionsResponse.data?.productOptionsCreate?.userErrors ?? [];
  if (setupUserErrors.length > 0) {
    throw new Error(`Option setup returned userErrors: ${JSON.stringify(setupUserErrors, null, 2)}`);
  }

  const defaultVariantId = setupOptionsResponse.data?.productOptionsCreate?.product?.variants?.nodes?.[0]?.id ?? null;
  if (!defaultVariantId) {
    throw new Error(
      `Option setup did not expose a default variant id: ${JSON.stringify(setupOptionsResponse, null, 2)}`,
    );
  }

  const unknownProductId = 'gid://shopify/Product/999999999999999999';
  const unknownVariantId = 'gid://shopify/ProductVariant/999999999999999999';
  const unknownLocationId = 'gid://shopify/Location/999999999999999999';
  const cases = [
    {
      name: 'create-unknown-product',
      query: bulkCreateMutation,
      variables: {
        productId: unknownProductId,
        variants: [{ optionValues: [{ optionName: 'Color', name: 'Blue' }] }],
      },
    },
    {
      name: 'create-empty-batch',
      query: bulkCreateMutation,
      variables: { productId, variants: [] },
    },
    {
      name: 'create-duplicate-option-values',
      query: bulkCreateMutation,
      variables: {
        productId,
        variants: [
          {
            optionValues: [
              { optionName: 'Color', name: 'Blue' },
              { optionName: 'Color', name: 'Red' },
            ],
            inventoryItem: { sku: `${skuPrefix}-DUP` },
          },
        ],
      },
    },
    {
      name: 'create-option-does-not-exist',
      query: bulkCreateMutation,
      variables: {
        productId,
        variants: [
          {
            optionValues: [{ optionName: 'Material', name: 'Cotton' }],
            inventoryItem: { sku: `${skuPrefix}-NOOPT` },
          },
        ],
      },
    },
    {
      name: 'create-missing-required-option',
      query: bulkCreateMutation,
      variables: {
        productId,
        variants: [
          {
            optionValues: [{ optionName: 'Color', name: 'Blue' }],
            inventoryItem: { sku: `${skuPrefix}-MISS` },
          },
        ],
      },
    },
    {
      name: 'create-invalid-inventory-location',
      query: bulkCreateMutation,
      variables: {
        productId,
        variants: [
          {
            optionValues: [
              { optionName: 'Color', name: 'Blue' },
              { optionName: 'Size', name: 'Large' },
            ],
            inventoryQuantities: [{ availableQuantity: 5, locationId: unknownLocationId }],
            inventoryItem: { sku: `${skuPrefix}-INVLOC` },
          },
        ],
      },
    },
    {
      name: 'create-mixed-valid-invalid',
      query: bulkCreateMutation,
      variables: {
        productId,
        variants: [
          {
            optionValues: [
              { optionName: 'Color', name: 'Blue' },
              { optionName: 'Size', name: 'Large' },
            ],
            inventoryItem: { sku: `${skuPrefix}-VALID` },
          },
          {
            optionValues: [{ optionName: 'Material', name: 'Cotton' }],
            inventoryItem: { sku: `${skuPrefix}-BAD` },
          },
        ],
      },
    },
    {
      name: 'update-unknown-product',
      query: bulkUpdateMutation,
      variables: { productId: unknownProductId, variants: [] },
    },
    {
      name: 'update-empty-batch',
      query: bulkUpdateMutation,
      variables: { productId, variants: [] },
    },
    {
      name: 'update-missing-variant-id',
      query: bulkUpdateMutation,
      variables: { productId, variants: [{ inventoryItem: { sku: `${skuPrefix}-NOID` } }] },
    },
    {
      name: 'update-unknown-variant-id',
      query: bulkUpdateMutation,
      variables: { productId, variants: [{ id: unknownVariantId, inventoryItem: { sku: `${skuPrefix}-UNKVAR` } }] },
    },
    {
      name: 'update-invalid-inventory-quantities',
      query: bulkUpdateMutation,
      variables: {
        productId,
        variants: [
          {
            id: defaultVariantId,
            inventoryQuantities: [{ availableQuantity: 4, locationId: 'gid://shopify/Location/1' }],
          },
        ],
      },
    },
    {
      name: 'update-option-does-not-exist',
      query: bulkUpdateMutation,
      variables: {
        productId,
        variants: [{ id: defaultVariantId, optionValues: [{ optionName: 'Material', name: 'Cotton' }] }],
      },
    },
    {
      name: 'update-mixed-valid-invalid',
      query: bulkUpdateMutation,
      variables: {
        productId,
        variants: [
          { id: defaultVariantId, inventoryItem: { sku: `${skuPrefix}-UPVALID` } },
          { id: unknownVariantId, inventoryItem: { sku: `${skuPrefix}-UPBAD` } },
        ],
      },
    },
    {
      name: 'delete-unknown-product',
      query: bulkDeleteMutation,
      variables: { productId: unknownProductId, variantsIds: [] },
    },
    {
      name: 'delete-empty-batch',
      query: bulkDeleteMutation,
      variables: { productId, variantsIds: [] },
    },
    {
      name: 'delete-unknown-variant-id',
      query: bulkDeleteMutation,
      variables: { productId, variantsIds: [unknownVariantId] },
    },
    {
      name: 'delete-mixed-valid-invalid',
      query: bulkDeleteMutation,
      variables: { productId, variantsIds: [defaultVariantId, unknownVariantId] },
    },
  ];

  const capturedCases = [];
  for (const entry of cases) {
    capturedCases.push(await captureCase({ ...entry, productId }));
  }

  const payload = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    seed: {
      productId,
      defaultVariantId,
      createProductResponse,
      setupOptionsResponse,
    },
    notes:
      'HAR-290 live validation and atomicity capture for productVariantsBulkCreate, productVariantsBulkUpdate, and productVariantsBulkDelete. All rejected branches preserve before/after product state.',
    cases: capturedCases,
  };

  await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        cases: capturedCases.length,
        productId,
        allAtomic: capturedCases.every((entry) => entry.atomicNoWrite),
      },
      null,
      2,
    ),
  );
} finally {
  await runGraphql(deleteProductMutation, { input: { id: productId } }).catch((error) => {
    console.error(`Cleanup failed for ${productId}: ${String(error)}`);
  });
}
