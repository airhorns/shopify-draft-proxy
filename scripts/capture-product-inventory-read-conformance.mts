/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

type GraphqlVariables = Record<string, unknown>;
type UserError = { field: string[] | null; message: string };
type InventoryItemSummary = { id?: string | null };
type VariantSummary = { id?: string | null; inventoryItem?: InventoryItemSummary | null };
type ProductSummary = {
  id?: string | null;
  variants?: {
    nodes?: VariantSummary[] | null;
  } | null;
};

type GraphqlPayload<TData> = {
  data?: TData;
  errors?: unknown;
};

type ProductCreateData = {
  productCreate?: {
    product?: ProductSummary | null;
    userErrors?: UserError[] | null;
  } | null;
};

type ProductOptionsCreateData = {
  productOptionsCreate?: {
    userErrors?: UserError[] | null;
  } | null;
};

type InitialVariantData = {
  product?: ProductSummary | null;
};

type ProductVariantsBulkCreateData = {
  productVariantsBulkCreate?: {
    productVariants?: VariantSummary[] | null;
    userErrors?: UserError[] | null;
  } | null;
};

type CapturePayload = {
  mutation: {
    variables: GraphqlVariables;
    response: GraphqlPayload<unknown>;
  };
  downstreamRead: GraphqlPayload<unknown>;
  setup?: Record<string, unknown>;
};

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`);
  }

  return value;
}

const storeDomain = requireEnv('SHOPIFY_CONFORMANCE_STORE_DOMAIN');
const adminOrigin = requireEnv('SHOPIFY_CONFORMANCE_ADMIN_ORIGIN');
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload<TData>>;
};

function expectNoUserErrors(pathLabel: string, userErrors: UserError[] | null | undefined): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${pathLabel} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

const productCreateMutation = `#graphql
  mutation ProductCreateInventoryReadParity($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        totalInventory
        tracksInventory
        variants(first: 10) {
          nodes {
            id
            title
            inventoryQuantity
            inventoryItem {
              id
              tracked
              requiresShipping
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

const createProductMutation = `#graphql
  mutation ProductInventoryReadSeedProduct($product: ProductCreateInput!) {
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
  mutation ProductInventoryReadSetupOptions($productId: ID!, $options: [OptionCreateInput!]!) {
    productOptionsCreate(productId: $productId, options: $options) {
      product {
        id
        options {
          name
          optionValues {
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
  query ProductInventoryReadInitialVariant($id: ID!) {
    product(id: $id) {
      id
      variants(first: 10) {
        nodes {
          id
          title
          inventoryQuantity
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

const productVariantsBulkCreateMutation = `#graphql
  mutation ProductVariantsBulkCreateInventoryReadParity($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkCreate(productId: $productId, variants: $variants) {
      product {
        id
        title
        handle
        status
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

const productCreateInventoryDownstreamQuery = `#graphql
  query ProductCreateInventoryReadDownstream($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) {
    product(id: $productId) {
      id
      title
      handle
      status
      totalInventory
      tracksInventory
      variants(first: 10) {
        nodes {
          id
          title
          inventoryQuantity
          inventoryItem {
            id
            tracked
            requiresShipping
          }
        }
      }
    }
    variant: productVariant(id: $variantId) {
      id
      title
      inventoryQuantity
      inventoryItem {
        id
        tracked
        requiresShipping
      }
      product {
        id
        title
        handle
        status
        totalInventory
        tracksInventory
      }
    }
    stock: inventoryItem(id: $inventoryItemId) {
      id
      tracked
      requiresShipping
      variant {
        id
        title
        sku
        inventoryQuantity
        product {
          id
          title
          handle
          status
          totalInventory
          tracksInventory
        }
      }
    }
  }
`;

const bulkCreateInventoryDownstreamQuery = `#graphql
  query ProductVariantsBulkCreateInventoryReadDownstream($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) {
    product(id: $productId) {
      id
      title
      handle
      status
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
    variant: productVariant(id: $variantId) {
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
      product {
        id
        title
        handle
        status
        totalInventory
        tracksInventory
      }
    }
    stock: inventoryItem(id: $inventoryItemId) {
      id
      tracked
      requiresShipping
      variant {
        id
        title
        sku
        inventoryQuantity
        product {
          id
          title
          handle
          status
          totalInventory
          tracksInventory
        }
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation ProductInventoryReadCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function buildProductVariables(title: string, runId: string): GraphqlVariables {
  return {
    product: {
      title,
      status: 'DRAFT',
      vendor: 'HERMES',
      productType: 'INVENTORY-CONFORMANCE',
      tags: ['conformance', 'inventory-read', runId],
    },
  };
}

function buildBulkCreateVariables(productId: string, runId: string): GraphqlVariables {
  return {
    productId,
    variants: [
      {
        optionValues: [{ optionName: 'Color', name: 'Blue' }],
        barcode: '3333333333333',
        price: '27.00',
        inventoryItem: {
          sku: `HERMES-INV-${runId.slice(-6)}-BLUE`,
          tracked: true,
          requiresShipping: false,
        },
      },
    ],
  };
}

function firstVariantWithInventoryItem(product: ProductSummary | null | undefined): VariantSummary | null {
  return product?.variants?.nodes?.find((variant) => variant?.inventoryItem?.id) ?? null;
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const productIdsToDelete = new Set<string>();

try {
  const productCreateVariables = buildProductVariables(`Hermes Product Inventory Read ${runId}`, runId);
  const productCreateResponse = await runGraphql<ProductCreateData>(productCreateMutation, productCreateVariables);
  expectNoUserErrors('productCreate inventory read', productCreateResponse.data?.productCreate?.userErrors);
  const createdProduct = productCreateResponse.data?.productCreate?.product ?? null;
  if (!createdProduct?.id) {
    throw new Error('Product create inventory read capture did not return a product id.');
  }
  productIdsToDelete.add(createdProduct.id);
  const createdVariant = firstVariantWithInventoryItem(createdProduct);
  if (!createdVariant?.id || !createdVariant?.inventoryItem?.id) {
    throw new Error('Product create inventory read capture did not return a variant inventory item.');
  }
  const productCreateDownstreamRead = await runGraphql<unknown>(productCreateInventoryDownstreamQuery, {
    productId: createdProduct.id,
    variantId: createdVariant.id,
    inventoryItemId: createdVariant.inventoryItem.id,
  });

  const seedProductVariables = buildProductVariables(`Hermes Product Inventory Variant Seed ${runId}`, runId);
  const seedProductResponse = await runGraphql<ProductCreateData>(createProductMutation, seedProductVariables);
  expectNoUserErrors('productCreate (bulk-create inventory seed)', seedProductResponse.data?.productCreate?.userErrors);
  const seedProductId = seedProductResponse.data?.productCreate?.product?.id ?? null;
  if (!seedProductId) {
    throw new Error('Bulk-create inventory read seed product did not return a product id.');
  }
  productIdsToDelete.add(seedProductId);

  const setupOptionsResponse = await runGraphql<ProductOptionsCreateData>(setupOptionsMutation, {
    productId: seedProductId,
    options: [{ name: 'Color', values: [{ name: 'Red' }, { name: 'Blue' }] }],
  });
  expectNoUserErrors(
    'productOptionsCreate (bulk-create inventory seed)',
    setupOptionsResponse.data?.productOptionsCreate?.userErrors,
  );

  const initialState = await runGraphql<InitialVariantData>(initialVariantQuery, { id: seedProductId });
  const defaultVariantId = initialState.data?.product?.variants?.nodes?.[0]?.id ?? null;
  if (!defaultVariantId) {
    throw new Error('Bulk-create inventory read seed product did not expose a default variant.');
  }

  const bulkCreateVariables = buildBulkCreateVariables(seedProductId, runId);
  const bulkCreateResponse = await runGraphql<ProductVariantsBulkCreateData>(
    productVariantsBulkCreateMutation,
    bulkCreateVariables,
  );
  expectNoUserErrors(
    'productVariantsBulkCreate inventory read',
    bulkCreateResponse.data?.productVariantsBulkCreate?.userErrors,
  );
  const createdBulkVariant = bulkCreateResponse.data?.productVariantsBulkCreate?.productVariants?.[0] ?? null;
  if (!createdBulkVariant?.id || !createdBulkVariant?.inventoryItem?.id) {
    throw new Error('Bulk-create inventory read capture did not return a created variant inventory item.');
  }
  const bulkCreateDownstreamRead = await runGraphql<unknown>(bulkCreateInventoryDownstreamQuery, {
    productId: seedProductId,
    variantId: createdBulkVariant.id,
    inventoryItemId: createdBulkVariant.inventoryItem.id,
  });

  const captures: Record<string, CapturePayload> = {
    'product-create-inventory-read-parity.json': {
      mutation: {
        variables: productCreateVariables,
        response: productCreateResponse,
      },
      downstreamRead: productCreateDownstreamRead,
    },
    'product-variants-bulk-create-inventory-read-parity.json': {
      setup: {
        product: {
          variables: seedProductVariables,
          response: seedProductResponse,
        },
        options: {
          variables: {
            productId: seedProductId,
            options: [{ name: 'Color', values: [{ name: 'Red' }, { name: 'Blue' }] }],
          },
          response: setupOptionsResponse,
        },
        initialState,
      },
      mutation: {
        variables: bulkCreateVariables,
        response: bulkCreateResponse,
      },
      downstreamRead: bulkCreateDownstreamRead,
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
