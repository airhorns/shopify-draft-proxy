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
const validationOutputPath = path.join(outputDir, 'inventory-item-update-validation.json');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlPayload(query, variables) {
  const { payload } = await runGraphqlRaw(query, variables);
  return payload;
}

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

const validationCreateMutation = `#graphql
  mutation InventoryItemUpdateValidationCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        tracksInventory
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

const inventoryItemUpdateValidationMutation = `#graphql
  mutation InventoryItemUpdateValidation($id: ID!, $input: InventoryItemInput!) {
    inventoryItemUpdate(id: $id, input: $input) {
      inventoryItem {
        id
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

const validationFixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  captures: {},
  upstreamCalls: [],
};

let productIds = [];

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
  const productId = createdProduct?.id ?? null;
  if (productId) {
    productIds.push(productId);
  }

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

  const validationCreateVariables = {
    product: {
      title: `hermes-inventory-item-update-validation-${Date.now()}`,
      status: 'DRAFT',
    },
  };
  const validationCreateResponse = await runGraphql(validationCreateMutation, validationCreateVariables);
  const validationProduct = validationCreateResponse.data?.productCreate?.product ?? null;
  const validationVariant = validationProduct?.variants?.nodes?.[0] ?? null;
  const validationInventoryItem = validationVariant?.inventoryItem ?? null;
  const validationInventoryItemId = validationInventoryItem?.id ?? null;
  const validationProductId = validationProduct?.id ?? null;
  if (validationProductId) {
    productIds.push(validationProductId);
  }
  if (!validationProductId || !validationInventoryItemId) {
    throw new Error(
      `Inventory item update validation capture failed to create a usable temporary product: ${JSON.stringify(validationCreateResponse, null, 2)}`,
    );
  }

  const validationCases = {
    negativeCost: {
      id: validationInventoryItemId,
      input: {
        cost: '-5.00',
      },
    },
    negativeWeightValue: {
      id: validationInventoryItemId,
      input: {
        measurement: {
          weight: {
            unit: 'KILOGRAMS',
            value: -1,
          },
        },
      },
    },
    invalidWeightUnit: {
      id: validationInventoryItemId,
      input: {
        measurement: {
          weight: {
            unit: 'STONES',
            value: 1,
          },
        },
      },
    },
    invalidCountry: {
      id: validationInventoryItemId,
      input: {
        countryCodeOfOrigin: 'ZZ',
      },
    },
    invalidHsCode: {
      id: validationInventoryItemId,
      input: {
        harmonizedSystemCode: '12',
      },
    },
  };

  const validationCaptures = {
    productCreate: {
      variables: validationCreateVariables,
      response: validationCreateResponse,
    },
  };
  for (const [name, variables] of Object.entries(validationCases)) {
    validationCaptures[name] = {
      variables,
      response: await runGraphqlPayload(inventoryItemUpdateValidationMutation, variables),
    };
  }

  validationFixture.captures = validationCaptures;
  validationFixture.upstreamCalls = [
    {
      operationName: 'ProductsHydrateNodes',
      variables: {
        ids: [validationInventoryItemId],
      },
      query: 'hand-synthesized from checked-in inventory item validation capture evidence',
      response: {
        status: 200,
        body: {
          data: {
            nodes: [
              {
                ...validationInventoryItem,
                variant: {
                  id: validationVariant.id,
                  inventoryQuantity: validationVariant.inventoryQuantity,
                  product: {
                    id: validationProduct.id,
                    title: validationProduct.title,
                    tracksInventory: validationProduct.tracksInventory,
                  },
                },
              },
            ],
          },
        },
      },
    },
  ];
  await writeFile(validationOutputPath, `${JSON.stringify(validationFixture, null, 2)}\n`);
  console.log(JSON.stringify({ ok: true, outputPath, validationOutputPath }, null, 2));
} finally {
  for (const productId of productIds.reverse()) {
    await runGraphql(deleteMutation, { input: { id: productId } }).catch(() => null);
  }
  await rm(path.join('pending', 'inventory-item-update-conformance-scope-blocker.md'), { force: true }).catch(
    () => null,
  );
}
