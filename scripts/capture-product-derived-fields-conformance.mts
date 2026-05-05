/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;
type UserError = { field: string[] | null; message: string; code?: string | null };
type GraphqlPayload<TData = unknown> = { data?: TData; errors?: unknown; extensions?: unknown };

type ProductNode = {
  id?: string | null;
  variants?: {
    nodes?: Array<{
      id?: string | null;
      inventoryItem?: {
        id?: string | null;
        inventoryLevels?: { nodes?: Array<{ location?: { id?: string | null } | null }> | null } | null;
      } | null;
    }> | null;
  } | null;
};

type ProductCreateData = {
  productCreate?: {
    product?: ProductNode | null;
    userErrors?: UserError[] | null;
  } | null;
};

type ProductSetData = {
  productSet?: {
    product?: ProductNode | null;
    userErrors?: UserError[] | null;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload<TData>>;
};

function expectNoUserErrors(label: string, userErrors: UserError[] | null | undefined): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function requireString(value: string | null | undefined, label: string): string {
  if (typeof value === 'string' && value.length > 0) return value;
  throw new Error(`Missing ${label}.`);
}

const productCreateMutation = `#graphql
  mutation ProductDerivedFieldsCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        variants(first: 10) {
          nodes {
            id
            title
            price
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

const priceUpdateMutation = `#graphql
  mutation ProductDerivedFieldsPriceUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        priceRangeV2 {
          minVariantPrice { amount currencyCode }
          maxVariantPrice { amount currencyCode }
        }
        totalVariants
        hasOnlyDefaultVariant
        hasOutOfStockVariants
        tracksInventory
        totalInventory
      }
      productVariants {
        id
        price
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const bulkCreateMutation = `#graphql
  mutation ProductDerivedFieldsBulkCreate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkCreate(productId: $productId, variants: $variants) {
      product {
        id
        priceRangeV2 {
          minVariantPrice { amount currencyCode }
          maxVariantPrice { amount currencyCode }
        }
        priceRange {
          minVariantPrice { amount currencyCode }
          maxVariantPrice { amount currencyCode }
        }
        totalVariants
        hasOnlyDefaultVariant
        hasOutOfStockVariants
        tracksInventory
        totalInventory
      }
      productVariants {
        id
        title
        price
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const priceRangeDownstreamQuery = `#graphql
  query ProductDerivedFieldsDownstream($id: ID!) {
    product(id: $id) {
      id
      priceRangeV2 {
        minVariantPrice { amount currencyCode }
        maxVariantPrice { amount currencyCode }
      }
      priceRange {
        minVariantPrice { amount currencyCode }
        maxVariantPrice { amount currencyCode }
      }
      totalVariants
      hasOnlyDefaultVariant
      hasOutOfStockVariants
      tracksInventory
      totalInventory
    }
  }
`;

const locationQuery = `#graphql
  query ProductDerivedFieldsLocation {
    locations(first: 1) {
      nodes {
        id
        name
      }
    }
  }
`;

const productSetSetupMutation = `#graphql
  mutation InventoryAdjustDerivedFieldsSetup($input: ProductSetInput!, $synchronous: Boolean!) {
    productSet(input: $input, synchronous: $synchronous) {
      product {
        id
        totalVariants
        hasOnlyDefaultVariant
        hasOutOfStockVariants
        tracksInventory
        totalInventory
        variants(first: 5) {
          nodes {
            id
            inventoryQuantity
            inventoryItem {
              id
              tracked
              inventoryLevels(first: 5) {
                nodes {
                  id
                  location { id name }
                  quantities(names: ["available", "on_hand"]) {
                    name
                    quantity
                  }
                }
              }
            }
          }
        }
      }
      productSetOperation {
        id
        status
        userErrors {
          field
          message
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const inventoryAdjustMutation = `#graphql
  mutation InventoryAdjustDerivedFieldsAdjust($input: InventoryAdjustQuantitiesInput!) {
    inventoryAdjustQuantities(input: $input) {
      inventoryAdjustmentGroup {
        reason
        referenceDocumentUri
        changes {
          name
          delta
          item { id }
          location { id name }
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

const inventoryDownstreamQuery = `#graphql
  query InventoryAdjustDerivedFieldsDownstream($id: ID!) {
    product(id: $id) {
      id
      totalVariants
      hasOnlyDefaultVariant
      hasOutOfStockVariants
      tracksInventory
      totalInventory
      variants(first: 5) {
        nodes {
          inventoryQuantity
          inventoryItem {
            tracked
            inventoryLevels(first: 5) {
              nodes {
                location { id name }
                quantities(names: ["available", "on_hand"]) {
                  name
                  quantity
                }
              }
            }
          }
        }
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation ProductDerivedFieldsCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const productIdsToDelete = new Set<string>();

try {
  const createVariables = {
    product: {
      title: `HAR592 Price Range Hat ${runId}`,
      status: 'DRAFT',
      productOptions: [{ name: 'Color', values: [{ name: 'Red' }] }],
    },
  };
  const create = await runGraphql<ProductCreateData>(productCreateMutation, createVariables);
  expectNoUserErrors('productCreate price-range setup', create.data?.productCreate?.userErrors);
  const priceProductId = requireString(create.data?.productCreate?.product?.id, 'price-range product id');
  productIdsToDelete.add(priceProductId);
  const redVariantId = requireString(
    create.data?.productCreate?.product?.variants?.nodes?.[0]?.id,
    'price-range default variant id',
  );

  const priceUpdateVariables = {
    productId: priceProductId,
    variants: [{ id: redVariantId, price: '10.00' }],
  };
  const priceUpdate = await runGraphql(priceUpdateMutation, priceUpdateVariables);
  expectNoUserErrors(
    'productVariantsBulkUpdate price setup',
    (priceUpdate.data as { productVariantsBulkUpdate?: { userErrors?: UserError[] | null } } | undefined)
      ?.productVariantsBulkUpdate?.userErrors,
  );

  const bulkCreateVariables = {
    productId: priceProductId,
    variants: [
      { optionValues: [{ optionName: 'Color', name: 'Blue' }], price: '5.00' },
      { optionValues: [{ optionName: 'Color', name: 'Green' }], price: '20.00' },
    ],
  };
  const bulkCreate = await runGraphql(bulkCreateMutation, bulkCreateVariables);
  expectNoUserErrors(
    'productVariantsBulkCreate derived fields',
    (bulkCreate.data as { productVariantsBulkCreate?: { userErrors?: UserError[] | null } } | undefined)
      ?.productVariantsBulkCreate?.userErrors,
  );
  const priceRangeDownstream = await runGraphql(priceRangeDownstreamQuery, { id: priceProductId });

  await writeFile(
    path.join(outputDir, 'product-create-then-bulk-create-price-range-parity.json'),
    `${JSON.stringify(
      {
        create: { variables: createVariables, response: create },
        priceUpdate: { variables: priceUpdateVariables, response: priceUpdate },
        bulkCreate: { variables: bulkCreateVariables, response: bulkCreate },
        downstreamRead: priceRangeDownstream,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  const locations = await runGraphql<{ locations?: { nodes?: Array<{ id?: string | null }> | null } }>(locationQuery);
  const locationId = requireString(locations.data?.locations?.nodes?.[0]?.id, 'setup location id');
  const inventorySetupVariables = {
    synchronous: true,
    input: {
      title: `HAR592 Inventory Aggregate ${runId}`,
      status: 'DRAFT',
      productOptions: [{ name: 'Title', position: 1, values: [{ name: 'Default Title' }] }],
      variants: [
        {
          optionValues: [{ optionName: 'Title', name: 'Default Title' }],
          price: '10.00',
          inventoryItem: { tracked: true, requiresShipping: true },
          inventoryQuantities: [{ locationId, name: 'available', quantity: 1 }],
        },
      ],
    },
  };
  const setup = await runGraphql<ProductSetData>(productSetSetupMutation, inventorySetupVariables);
  expectNoUserErrors('productSet inventory setup', setup.data?.productSet?.userErrors);
  const inventoryProductId = requireString(setup.data?.productSet?.product?.id, 'inventory product id');
  productIdsToDelete.add(inventoryProductId);
  const inventoryVariant = setup.data?.productSet?.product?.variants?.nodes?.[0] ?? null;
  const inventoryItemId = requireString(inventoryVariant?.inventoryItem?.id, 'inventory item id');
  const adjustmentLocationId = requireString(
    inventoryVariant?.inventoryItem?.inventoryLevels?.nodes?.[0]?.location?.id,
    'inventory adjustment location id',
  );
  const adjustVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://har-592/inventory-adjust/${runId}`,
      changes: [{ inventoryItemId, locationId: adjustmentLocationId, delta: -1 }],
    },
  };
  const adjust = await runGraphql(inventoryAdjustMutation, adjustVariables);
  expectNoUserErrors(
    'inventoryAdjustQuantities derived fields',
    (adjust.data as { inventoryAdjustQuantities?: { userErrors?: UserError[] | null } } | undefined)
      ?.inventoryAdjustQuantities?.userErrors,
  );
  const inventoryDownstream = await runGraphql(inventoryDownstreamQuery, { id: inventoryProductId });

  await writeFile(
    path.join(outputDir, 'inventory-adjust-then-has-out-of-stock-variants-parity.json'),
    `${JSON.stringify(
      {
        setup: { variables: inventorySetupVariables, response: setup },
        adjust: { variables: adjustVariables, response: adjust },
        downstreamRead: inventoryDownstream,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: [
          'product-create-then-bulk-create-price-range-parity.json',
          'inventory-adjust-then-has-out-of-stock-variants-parity.json',
        ],
      },
      null,
      2,
    ),
  );
} finally {
  for (const productId of productIdsToDelete) {
    try {
      await runGraphql(deleteMutation, { input: { id: productId } });
    } catch (error) {
      console.error(`Cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}
