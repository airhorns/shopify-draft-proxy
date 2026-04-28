// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventory-item-update-parity.json');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createMutation = `#graphql
  mutation InventoryItemUpdateConformanceCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            inventoryQuantity
            inventoryItem {
              id
              tracked
              requiresShipping
              countryCodeOfOrigin
              provinceCodeOfOrigin
              harmonizedSystemCode
              measurement {
                weight {
                  unit
                  value
                }
              }
              variant {
                id
                inventoryQuantity
                product {
                  id
                  title
                  tracksInventory
                }
              }
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

const inventoryItemUpdateMutation = `#graphql
  mutation InventoryItemUpdateParityPlan($id: ID!, $input: InventoryItemInput!) {
    inventoryItemUpdate(id: $id, input: $input) {
      inventoryItem {
        id
        tracked
        requiresShipping
        countryCodeOfOrigin
        provinceCodeOfOrigin
        harmonizedSystemCode
        measurement {
          weight {
            unit
            value
          }
        }
        variant {
          id
          inventoryQuantity
          product {
            id
            title
            tracksInventory
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

const downstreamReadQuery = `#graphql
  query InventoryItemUpdateDownstream($variantId: ID!, $inventoryItemId: ID!) {
    productVariant(id: $variantId) {
      id
      inventoryQuantity
      inventoryItem {
        id
        tracked
        requiresShipping
        countryCodeOfOrigin
        provinceCodeOfOrigin
        harmonizedSystemCode
        measurement {
          weight {
            unit
            value
          }
        }
      }
    }
    inventoryItem(id: $inventoryItemId) {
      id
      tracked
      requiresShipping
      countryCodeOfOrigin
      provinceCodeOfOrigin
      harmonizedSystemCode
      measurement {
        weight {
          unit
          value
        }
      }
      variant {
        id
        inventoryQuantity
        product {
          id
          title
          tracksInventory
        }
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation InventoryItemUpdateConformanceDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  mutation: null,
  validation: null,
};

let productId = null;

try {
  const createVariables = {
    product: {
      title: `hermes-inventory-item-update-${Date.now()}`,
      status: 'DRAFT',
    },
  };
  const createResult = await runGraphql(createMutation, createVariables);
  const createdProduct = createResult.data?.productCreate?.product ?? null;
  const createdVariant = createdProduct?.variants?.nodes?.[0] ?? null;
  const inventoryItemId = createdVariant?.inventoryItem?.id ?? null;
  const variantId = createdVariant?.id ?? null;
  productId = createdProduct?.id ?? null;

  if (!productId || !inventoryItemId || !variantId) {
    throw new Error(
      `Inventory item update capture failed to create a usable temporary product: ${JSON.stringify(createResult, null, 2)}`,
    );
  }

  const mutationVariables = {
    id: inventoryItemId,
    input: {
      tracked: true,
      requiresShipping: false,
      countryCodeOfOrigin: 'CA',
      provinceCodeOfOrigin: 'ON',
      harmonizedSystemCode: '620343',
      measurement: {
        weight: {
          unit: 'KILOGRAMS',
          value: 2.5,
        },
      },
    },
  };
  const mutationResponse = await runGraphql(inventoryItemUpdateMutation, mutationVariables);
  const downstreamRead = await runGraphql(downstreamReadQuery, {
    variantId,
    inventoryItemId,
  });
  const validationVariables = {
    id: 'gid://shopify/InventoryItem/99999999999999',
    input: {
      tracked: true,
    },
  };
  const validationResponse = await runGraphql(inventoryItemUpdateMutation, validationVariables);

  fixture.mutation = {
    create: {
      variables: createVariables,
      response: createResult,
    },
    variables: mutationVariables,
    response: mutationResponse,
    downstreamRead,
  };
  fixture.validation = {
    variables: validationVariables,
    response: validationResponse,
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (productId) {
    await runGraphql(deleteMutation, { input: { id: productId } }).catch(() => null);
  }
  await rm(path.join('pending', 'inventory-item-update-conformance-scope-blocker.md'), { force: true }).catch(
    () => null,
  );
}
