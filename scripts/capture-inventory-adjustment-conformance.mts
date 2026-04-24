// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile, rm } from 'node:fs/promises';
import path from 'node:path';

import { runAdminGraphql, runAdminGraphqlRequest } from './conformance-graphql-client.mjs';
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
const blockerPath = path.join(pendingDir, 'inventory-adjustment-conformance-scope-blocker.md');

async function runGraphql(query, variables = {}) {
  return runAdminGraphql(
    { adminOrigin, apiVersion, headers: buildAdminAuthHeaders(adminAccessToken) },
    query,
    variables,
  );
}

async function runGraphqlAllowGraphqlErrors(query, variables = {}) {
  const result = await runAdminGraphqlRequest(
    { adminOrigin, apiVersion, headers: buildAdminAuthHeaders(adminAccessToken) },
    query,
    variables,
  );
  if (result.status < 200 || result.status >= 300) {
    const error = new Error(JSON.stringify(result, null, 2));
    error.result = result;
    throw error;
  }

  return result.payload;
}

const createMutation = `#graphql
  mutation InventoryAdjustmentConformanceCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 10) {
          nodes {
            id
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

const locationsQuery = `#graphql
  query InventoryAdjustmentLocations {
    locations(first: 10) {
      nodes {
        id
      }
    }
  }
`;

const trackInventoryMutation = `#graphql
  mutation InventoryAdjustmentTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        totalInventory
        tracksInventory
        variants(first: 10) {
          nodes {
            id
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

const inventoryAdjustMutation = `#graphql
  mutation InventoryAdjustQuantitiesParityPlan($input: InventoryAdjustQuantitiesInput!) {
    inventoryAdjustQuantities(input: $input) {
      inventoryAdjustmentGroup {
        id
        createdAt
        reason
        referenceDocumentUri
        changes {
          name
          delta
          quantityAfterChange
          ledgerDocumentUri
          item {
            id
          }
          location {
            id
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

const inventoryAdjustMetadataMutation = `#graphql
  mutation InventoryAdjustQuantitiesMetadataProbe($input: InventoryAdjustQuantitiesInput!) {
    inventoryAdjustQuantities(input: $input) {
      inventoryAdjustmentGroup {
        id
        createdAt
        reason
        referenceDocumentUri
        app {
          id
          title
          apiKey
          handle
        }
        staffMember {
          id
          name
          email
          firstName
          lastName
          initials
          locale
        }
        changes {
          name
          delta
          quantityAfterChange
          ledgerDocumentUri
          item {
            id
          }
          location {
            id
            name
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
  query InventoryAdjustmentDownstream(
    $firstProductId: ID!
    $secondProductId: ID!
    $firstVariantId: ID!
    $secondVariantId: ID!
    $firstInventoryItemId: ID!
    $secondInventoryItemId: ID!
    $query: String!
  ) {
    firstProduct: product(id: $firstProductId) {
      id
      totalInventory
      tracksInventory
      variants(first: 10) {
        nodes {
          id
          inventoryQuantity
          inventoryItem {
            id
            tracked
          }
        }
      }
    }
    secondProduct: product(id: $secondProductId) {
      id
      totalInventory
      tracksInventory
      variants(first: 10) {
        nodes {
          id
          inventoryQuantity
          inventoryItem {
            id
            tracked
          }
        }
      }
    }
    firstVariant: productVariant(id: $firstVariantId) {
      id
      inventoryQuantity
      inventoryItem {
        id
        tracked
      }
    }
    secondVariant: productVariant(id: $secondVariantId) {
      id
      inventoryQuantity
      inventoryItem {
        id
        tracked
      }
    }
    firstInventoryItem: inventoryItem(id: $firstInventoryItemId) {
      id
      tracked
      variant {
        id
        inventoryQuantity
        product {
          id
          totalInventory
        }
      }
    }
    secondInventoryItem: inventoryItem(id: $secondInventoryItemId) {
      id
      tracked
      variant {
        id
        inventoryQuantity
        product {
          id
          totalInventory
        }
      }
    }
    matching: products(first: 10, query: $query) {
      nodes {
        id
        totalInventory
        tracksInventory
        variants(first: 10) {
          nodes {
            id
            inventoryQuantity
          }
        }
      }
    }
    matchingCount: productsCount(query: $query) {
      count
      precision
    }
  }
`;

const nonAvailableDownstreamReadQuery = `#graphql
  query InventoryAdjustmentNonAvailableDownstream(
    $firstProductId: ID!
    $firstVariantId: ID!
    $firstInventoryItemId: ID!
  ) {
    firstProduct: product(id: $firstProductId) {
      id
      totalInventory
      tracksInventory
      variants(first: 10) {
        nodes {
          id
          inventoryQuantity
          inventoryItem {
            id
            tracked
            inventoryLevels(first: 5) {
              nodes {
                id
                quantities(names: ["available", "incoming", "reserved", "damaged", "quality_control", "safety_stock", "committed", "on_hand"]) {
                  name
                  quantity
                  updatedAt
                }
              }
            }
          }
        }
      }
    }
    firstVariant: productVariant(id: $firstVariantId) {
      id
      inventoryQuantity
      inventoryItem {
        id
        tracked
        inventoryLevels(first: 5) {
          nodes {
            id
            quantities(names: ["available", "incoming", "reserved", "damaged", "quality_control", "safety_stock", "committed", "on_hand"]) {
              name
              quantity
              updatedAt
            }
          }
        }
      }
    }
    firstInventoryItem: inventoryItem(id: $firstInventoryItemId) {
      id
      tracked
      inventoryLevels(first: 5) {
        nodes {
          id
          quantities(names: ["available", "incoming", "reserved", "damaged", "quality_control", "safety_stock", "committed", "on_hand"]) {
            name
            quantity
            updatedAt
          }
        }
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation InventoryAdjustmentConformanceDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function buildCreateVariables(runId, label) {
  return {
    product: {
      title: `Hermes Inventory Adjustment ${label} ${runId}`,
      status: 'DRAFT',
    },
  };
}

function extractCreatedProduct(payload) {
  const product = payload?.data?.productCreate?.product;
  const variant = product?.variants?.nodes?.[0];
  const inventoryItem = variant?.inventoryItem;
  if (typeof product?.id !== 'string' || typeof variant?.id !== 'string' || typeof inventoryItem?.id !== 'string') {
    throw new Error('Inventory adjustment capture did not return a default variant inventory item.');
  }

  return {
    productId: product.id,
    variantId: variant.id,
    inventoryItemId: inventoryItem.id,
  };
}

async function deleteProduct(productId) {
  if (!productId) {
    return;
  }

  try {
    await runGraphql(deleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`Cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Inventory adjustment conformance blocker',
    whatFailed:
      'Attempted to capture live conformance for `inventoryAdjustQuantities`, including tracked inventory setup and immediate downstream inventory reads.',
    operations: ['inventoryAdjustQuantities'],
    blocker,
    whyBlocked:
      'Without a token that can create products, mark inventory tracked, and write inventory adjustments against a valid location, the repo cannot capture real payload shape or downstream read lag for inventory adjustments.',
    completedSteps: [
      'added a dedicated live inventory-adjustment capture harness with safe create/update/delete cleanup',
      'aligned the captured mutation slice with the checked-in parity request and downstream read probes for product, productVariant, inventoryItem, and products/productsCount inventory filters',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with product and inventory write scopes, then rerun `tsx ./scripts/capture-inventory-adjustment-conformance.mts`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
let firstProductId = null;
let secondProductId = null;

try {
  const locationsResponse = await runGraphql(locationsQuery);
  const locationId = locationsResponse.data?.locations?.nodes?.[0]?.id ?? null;
  if (typeof locationId !== 'string' || !locationId) {
    throw new Error('Could not resolve a writable location id for inventory adjustment capture.');
  }

  const firstCreate = await runGraphql(createMutation, buildCreateVariables(runId, 'A'));
  const secondCreate = await runGraphql(createMutation, buildCreateVariables(runId, 'B'));
  const first = extractCreatedProduct(firstCreate);
  const second = extractCreatedProduct(secondCreate);
  firstProductId = first.productId;
  secondProductId = second.productId;

  const trackFirstResponse = await runGraphql(trackInventoryMutation, {
    productId: first.productId,
    variants: [
      {
        id: first.variantId,
        inventoryItem: {
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  });
  const trackSecondResponse = await runGraphql(trackInventoryMutation, {
    productId: second.productId,
    variants: [
      {
        id: second.variantId,
        inventoryItem: {
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  });

  const seedResponse = await runGraphql(inventoryAdjustMutation, {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: 'logistics://cycle-count/2026-04-15/seed',
      changes: [
        {
          inventoryItemId: first.inventoryItemId,
          locationId,
          delta: 3,
        },
        {
          inventoryItemId: second.inventoryItemId,
          locationId,
          delta: 7,
        },
      ],
    },
  });

  const mutationVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: 'logistics://cycle-count/2026-04-15',
      changes: [
        {
          inventoryItemId: first.inventoryItemId,
          locationId,
          delta: -2,
        },
        {
          inventoryItemId: second.inventoryItemId,
          locationId,
          delta: 4,
        },
      ],
    },
  };

  const mutationResponse = await runGraphqlAllowGraphqlErrors(inventoryAdjustMetadataMutation, mutationVariables);
  const downstreamRead = await runGraphql(downstreamReadQuery, {
    firstProductId: first.productId,
    secondProductId: second.productId,
    firstVariantId: first.variantId,
    secondVariantId: second.variantId,
    firstInventoryItemId: first.inventoryItemId,
    secondInventoryItemId: second.inventoryItemId,
    query: 'inventory_total:>=4',
  });

  const nonAvailableMutationVariables = {
    input: {
      name: 'incoming',
      reason: 'correction',
      referenceDocumentUri: 'logistics://incoming/2026-04-17',
      changes: [
        {
          inventoryItemId: first.inventoryItemId,
          locationId,
          ledgerDocumentUri: 'ledger://incoming/first',
          delta: 2,
        },
      ],
    },
  };
  const nonAvailableMutationResponse = await runGraphql(inventoryAdjustMutation, nonAvailableMutationVariables);
  const nonAvailableDownstreamRead = await runGraphql(nonAvailableDownstreamReadQuery, {
    firstProductId: first.productId,
    firstVariantId: first.variantId,
    firstInventoryItemId: first.inventoryItemId,
  });

  const invalidNameProbeVariables = {
    input: {
      name: 'on_hand',
      reason: 'correction',
      referenceDocumentUri: 'logistics://invalid/on-hand',
      changes: [
        {
          inventoryItemId: first.inventoryItemId,
          locationId,
          delta: 1,
        },
      ],
    },
  };
  const invalidNameProbeResponse = await runGraphql(inventoryAdjustMutation, invalidNameProbeVariables);

  const missingInventoryItemIdVariables = {
    input: {
      name: 'incoming',
      reason: 'correction',
      referenceDocumentUri: 'logistics://invalid/missing-item',
      changes: [
        {
          locationId,
          ledgerDocumentUri: 'ledger://missing-item',
          delta: 2,
        },
      ],
    },
  };
  const missingInventoryItemIdResponse = await runGraphqlAllowGraphqlErrors(
    inventoryAdjustMutation,
    missingInventoryItemIdVariables,
  );

  const missingDeltaVariables = {
    input: {
      name: 'incoming',
      reason: 'correction',
      referenceDocumentUri: 'logistics://invalid/missing-delta',
      changes: [
        {
          inventoryItemId: first.inventoryItemId,
          locationId,
          ledgerDocumentUri: 'ledger://missing-delta',
        },
      ],
    },
  };
  const missingDeltaResponse = await runGraphqlAllowGraphqlErrors(inventoryAdjustMutation, missingDeltaVariables);

  const missingLocationIdVariables = {
    input: {
      name: 'incoming',
      reason: 'correction',
      referenceDocumentUri: 'logistics://invalid/missing-location',
      changes: [
        {
          inventoryItemId: first.inventoryItemId,
          ledgerDocumentUri: 'ledger://missing-location',
          delta: 2,
        },
      ],
    },
  };
  const missingLocationIdResponse = await runGraphqlAllowGraphqlErrors(
    inventoryAdjustMutation,
    missingLocationIdVariables,
  );

  const unknownInventoryItemIdVariables = {
    input: {
      name: 'incoming',
      reason: 'correction',
      referenceDocumentUri: 'logistics://invalid/unknown-item',
      changes: [
        {
          inventoryItemId: 'gid://shopify/InventoryItem/999999999999',
          locationId,
          ledgerDocumentUri: 'ledger://unknown-item',
          delta: 2,
        },
      ],
    },
  };
  const unknownInventoryItemIdResponse = await runGraphql(inventoryAdjustMutation, unknownInventoryItemIdVariables);

  const unknownLocationIdVariables = {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: 'logistics://invalid/unknown-location',
      changes: [
        {
          inventoryItemId: first.inventoryItemId,
          locationId: 'gid://shopify/Location/999999999999',
          delta: 1,
        },
      ],
    },
  };
  const unknownLocationIdResponse = await runGraphql(inventoryAdjustMutation, unknownLocationIdVariables);

  const capturePayload = {
    setup: {
      locationId,
      trackedInventory: {
        first: trackFirstResponse,
        second: trackSecondResponse,
      },
      seedAdjustment: seedResponse,
      products: [first, second],
    },
    mutation: {
      variables: mutationVariables,
      response: mutationResponse,
    },
    downstreamRead,
    nonAvailableMutation: {
      variables: nonAvailableMutationVariables,
      response: nonAvailableMutationResponse,
      downstreamRead: nonAvailableDownstreamRead,
    },
    invalidNameProbe: {
      variables: invalidNameProbeVariables,
      response: invalidNameProbeResponse,
    },
    missingRequiredFieldProbes: {
      missingInventoryItemId: {
        variables: missingInventoryItemIdVariables,
        response: missingInventoryItemIdResponse,
      },
      missingDelta: {
        variables: missingDeltaVariables,
        response: missingDeltaResponse,
      },
      missingLocationId: {
        variables: missingLocationIdVariables,
        response: missingLocationIdResponse,
      },
      unknownInventoryItemId: {
        variables: unknownInventoryItemIdVariables,
        response: unknownInventoryItemIdResponse,
      },
      unknownLocationId: {
        variables: unknownLocationIdVariables,
        response: unknownLocationIdResponse,
      },
    },
  };

  const outputPath = path.join(outputDir, 'inventory-adjust-quantities-parity.json');
  await writeFile(outputPath, `${JSON.stringify(capturePayload, null, 2)}\n`, 'utf8');
  await rm(blockerPath, { force: true });

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: ['inventory-adjust-quantities-parity.json'],
        locationId,
        productIds: [first.productId, second.productId],
      },
      null,
      2,
    ),
  );
} catch (error) {
  const blocker = parseWriteScopeBlocker(error);
  if (blocker) {
    await writeScopeBlocker(blocker);
  }

  throw error;
} finally {
  await deleteProduct(firstProductId);
  await deleteProduct(secondProductId);
}
