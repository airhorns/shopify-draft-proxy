// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

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
const outputPath = path.join(outputDir, 'inventory-linkage-parity.json');
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'inventory-linkage-single-location-blocker.md');

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
    throw new Error(JSON.stringify({ status: response.status, payload }, null, 2));
  }

  return payload;
}

const createProductMutation = `#graphql
  mutation InventoryLinkageProductCreate($input: ProductCreateInput!) {
    productCreate(product: $input) {
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
              inventoryLevels(first: 5) {
                nodes {
                  id
                  location {
                    id
                    name
                  }
                  quantities(names: ["available", "on_hand", "incoming"]) {
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
      userErrors {
        field
        message
      }
    }
  }
`;

const locationsQuery = `#graphql
  query InventoryLinkageLocations {
    locations(first: 10) {
      nodes {
        id
        name
      }
    }
  }
`;

const inventoryActivateMutation = `#graphql
  mutation InventoryActivateParityPlan($inventoryItemId: ID!, $locationId: ID!, $available: Int) {
    inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: $available) {
      inventoryLevel {
        id
        location {
          id
          name
        }
        quantities(names: ["available", "on_hand", "incoming"]) {
          name
          quantity
          updatedAt
        }
        item {
          id
          tracked
          variant {
            id
            inventoryQuantity
            product {
              id
              totalInventory
              tracksInventory
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

const inventoryDeactivateMutation = `#graphql
  mutation InventoryDeactivateParityPlan($inventoryLevelId: ID!) {
    inventoryDeactivate(inventoryLevelId: $inventoryLevelId) {
      userErrors {
        field
        message
      }
    }
  }
`;

const inventoryBulkToggleMutation = `#graphql
  mutation InventoryBulkToggleActivationParityPlan($inventoryItemId: ID!, $inventoryItemUpdates: [InventoryBulkToggleActivationInput!]!) {
    inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $inventoryItemUpdates) {
      inventoryItem {
        id
        tracked
        inventoryLevels(first: 5) {
          nodes {
            id
            location {
              id
            }
            quantities(names: ["available", "on_hand", "incoming"]) {
              name
              quantity
            }
          }
        }
      }
      inventoryLevels {
        id
        location {
          id
        }
        quantities(names: ["available", "on_hand", "incoming"]) {
          name
          quantity
        }
        item {
          id
          tracked
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

const downstreamReadQuery = `#graphql
  query InventoryLinkageDownstreamRead($inventoryItemId: ID!, $variantId: ID!) {
    inventoryItem(id: $inventoryItemId) {
      id
      tracked
      variant {
        id
        inventoryQuantity
        product {
          id
          totalInventory
          tracksInventory
        }
      }
      inventoryLevels(first: 5) {
        nodes {
          id
          location {
            id
            name
          }
          quantities(names: ["available", "on_hand", "incoming"]) {
            name
            quantity
            updatedAt
          }
        }
      }
    }
    productVariant(id: $variantId) {
      id
      inventoryQuantity
      inventoryItem {
        id
        tracked
        inventoryLevels(first: 5) {
          nodes {
            id
            location {
              id
              name
            }
            quantities(names: ["available", "on_hand", "incoming"]) {
              name
              quantity
              updatedAt
            }
          }
        }
      }
    }
  }
`;

const deleteProductMutation = `#graphql
  mutation InventoryLinkageProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

async function createTemporaryProduct(title) {
  const created = await runGraphql(createProductMutation, {
    input: {
      title,
    },
  });

  const product = created.data?.productCreate?.product ?? null;
  const variant = product?.variants?.nodes?.[0] ?? null;
  const inventoryItem = variant?.inventoryItem ?? null;
  const inventoryLevel = inventoryItem?.inventoryLevels?.nodes?.[0] ?? null;

  if (!product?.id || !variant?.id || !inventoryItem?.id || !inventoryLevel?.id) {
    throw new Error(
      `Unexpected productCreate payload for inventory linkage capture: ${JSON.stringify(created, null, 2)}`,
    );
  }

  return { product, variant, inventoryItem, inventoryLevel };
}

async function cleanupTemporaryProducts(products) {
  for (const product of products) {
    if (!product?.id) {
      continue;
    }

    await runGraphql(deleteProductMutation, { input: { id: product.id } }).catch((error) => {
      console.error(`Cleanup failed for ${product.id}: ${error.message}`);
    });
  }
}

async function main() {
  await mkdir(outputDir, { recursive: true });
  await mkdir(pendingDir, { recursive: true });

  const locations = await runGraphql(locationsQuery);
  const locationNodes = Array.isArray(locations.data?.locations?.nodes) ? locations.data.locations.nodes : [];
  const primaryLocation = locationNodes[0] ?? null;
  const secondaryLocation = locationNodes[1] ?? null;
  if (!primaryLocation?.id) {
    throw new Error(`Expected at least one location on ${storeDomain}`);
  }

  if (!secondaryLocation?.id) {
    const blockerNote = `# Inventory linkage mutation blocker

Date: ${new Date().toISOString().slice(0, 10)}

## What was refreshed

I refreshed the live conformance evidence for the next inventory-linkage family after \`inventoryItemUpdate\`:

- \`inventoryActivate\`
- \`inventoryDeactivate\`
- \`inventoryBulkToggleActivation\`

The current host/store/token still exposes only one location:

- \`${primaryLocation.id}\` (\`${primaryLocation.name}\`)

## Why the broader success path is still blocked

The current store shape still prevents capturing the multi-location success path faithfully. With only one real location, every safe deactivation probe hits the minimum-one-location rule instead of the ordinary success payloads. That means the remaining success path for this family still needs at least one second safe location before it can be captured without guessing.

## Recommended next step

Before trying to close the broader success path for \`inventoryActivate\` / \`inventoryDeactivate\` / \`inventoryBulkToggleActivation\`, add a second safe location to the conformance store (or switch to a dev store that already has one), then rerun \`corepack pnpm conformance:capture-inventory-linkage-mutations\`.
`;
    await writeFile(blockerPath, blockerNote, 'utf8');
    console.log(
      JSON.stringify(
        {
          ok: true,
          storeDomain,
          apiVersion,
          locations: locationNodes,
          files: ['inventory-linkage-single-location-blocker.md'],
        },
        null,
        2,
      ),
    );
    return;
  }

  const singleLocationProduct = await createTemporaryProduct(`hermes-inventory-linkage-single-${Date.now()}`);
  const directSuccessProduct = await createTemporaryProduct(`hermes-inventory-linkage-direct-${Date.now()}`);
  const bulkSuccessProduct = await createTemporaryProduct(`hermes-inventory-linkage-bulk-${Date.now()}`);

  try {
    const activateNoOpVariables = {
      inventoryItemId: singleLocationProduct.inventoryItem.id,
      locationId: primaryLocation.id,
    };
    const activateAvailableErrorVariables = {
      ...activateNoOpVariables,
      available: 7,
    };
    const activateUnknownLocationVariables = {
      inventoryItemId: singleLocationProduct.inventoryItem.id,
      locationId: 'gid://shopify/Location/999999999999',
    };
    const deactivateOnlyLocationVariables = {
      inventoryLevelId: singleLocationProduct.inventoryLevel.id,
    };
    const bulkActivateNoOpVariables = {
      inventoryItemId: singleLocationProduct.inventoryItem.id,
      inventoryItemUpdates: [{ locationId: primaryLocation.id, activate: true }],
    };
    const bulkUnknownLocationVariables = {
      inventoryItemId: singleLocationProduct.inventoryItem.id,
      inventoryItemUpdates: [{ locationId: 'gid://shopify/Location/999999999999', activate: true }],
    };
    const bulkDeactivateOnlyLocationVariables = {
      inventoryItemId: singleLocationProduct.inventoryItem.id,
      inventoryItemUpdates: [{ locationId: primaryLocation.id, activate: false }],
    };

    const activateSecondLocationVariables = {
      inventoryItemId: directSuccessProduct.inventoryItem.id,
      locationId: secondaryLocation.id,
      available: 9,
    };
    const bulkActivateSecondLocationVariables = {
      inventoryItemId: bulkSuccessProduct.inventoryItem.id,
      inventoryItemUpdates: [{ locationId: secondaryLocation.id, activate: true }],
    };

    const activateNoOp = await runGraphql(inventoryActivateMutation, activateNoOpVariables);
    const activateAvailableError = await runGraphql(inventoryActivateMutation, activateAvailableErrorVariables);
    const activateUnknownLocation = await runGraphql(inventoryActivateMutation, activateUnknownLocationVariables);
    const deactivateOnlyLocationError = await runGraphql(inventoryDeactivateMutation, deactivateOnlyLocationVariables);
    const bulkActivateNoOp = await runGraphql(inventoryBulkToggleMutation, bulkActivateNoOpVariables);
    const bulkUnknownLocation = await runGraphql(inventoryBulkToggleMutation, bulkUnknownLocationVariables);
    const bulkDeactivateOnlyLocationError = await runGraphql(
      inventoryBulkToggleMutation,
      bulkDeactivateOnlyLocationVariables,
    );

    const activateSecondLocation = await runGraphql(inventoryActivateMutation, activateSecondLocationVariables);
    const downstreamReadAfterActivateSecondLocation = await runGraphql(downstreamReadQuery, {
      inventoryItemId: directSuccessProduct.inventoryItem.id,
      variantId: directSuccessProduct.variant.id,
    });
    const deactivateAlternateLocationVariables = {
      inventoryLevelId: directSuccessProduct.inventoryLevel.id,
    };
    const deactivateWithAlternateLocation = await runGraphql(
      inventoryDeactivateMutation,
      deactivateAlternateLocationVariables,
    );
    const downstreamReadAfterDeactivateWithAlternateLocation = await runGraphql(downstreamReadQuery, {
      inventoryItemId: directSuccessProduct.inventoryItem.id,
      variantId: directSuccessProduct.variant.id,
    });

    const bulkActivateSecondLocation = await runGraphql(
      inventoryBulkToggleMutation,
      bulkActivateSecondLocationVariables,
    );
    const downstreamReadAfterBulkActivateSecondLocation = await runGraphql(downstreamReadQuery, {
      inventoryItemId: bulkSuccessProduct.inventoryItem.id,
      variantId: bulkSuccessProduct.variant.id,
    });
    const bulkDeactivateSecondLocationVariables = {
      inventoryItemId: bulkSuccessProduct.inventoryItem.id,
      inventoryItemUpdates: [{ locationId: secondaryLocation.id, activate: false }],
    };
    const bulkDeactivateSecondLocation = await runGraphql(
      inventoryBulkToggleMutation,
      bulkDeactivateSecondLocationVariables,
    );
    const downstreamReadAfterBulkDeactivateSecondLocation = await runGraphql(downstreamReadQuery, {
      inventoryItemId: bulkSuccessProduct.inventoryItem.id,
      variantId: bulkSuccessProduct.variant.id,
    });

    const fixture = {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      locations: locationNodes,
      createdProduct: singleLocationProduct.product,
      inventoryActivateNoOp: {
        variables: activateNoOpVariables,
        response: activateNoOp,
      },
      inventoryActivateAvailableError: {
        variables: activateAvailableErrorVariables,
        response: activateAvailableError,
      },
      inventoryActivateUnknownLocation: {
        variables: activateUnknownLocationVariables,
        response: activateUnknownLocation,
      },
      inventoryDeactivateOnlyLocationError: {
        variables: deactivateOnlyLocationVariables,
        response: deactivateOnlyLocationError,
      },
      inventoryBulkToggleActivateNoOp: {
        variables: bulkActivateNoOpVariables,
        response: bulkActivateNoOp,
      },
      inventoryBulkToggleUnknownLocation: {
        variables: bulkUnknownLocationVariables,
        response: bulkUnknownLocation,
      },
      inventoryBulkToggleDeactivateOnlyLocationError: {
        variables: bulkDeactivateOnlyLocationVariables,
        response: bulkDeactivateOnlyLocationError,
      },
      inventoryActivateSecondLocation: {
        variables: activateSecondLocationVariables,
        response: activateSecondLocation,
      },
      downstreamReadAfterInventoryActivateSecondLocation: downstreamReadAfterActivateSecondLocation,
      inventoryDeactivateWithAlternateLocation: {
        variables: deactivateAlternateLocationVariables,
        response: deactivateWithAlternateLocation,
      },
      downstreamReadAfterInventoryDeactivateWithAlternateLocation: downstreamReadAfterDeactivateWithAlternateLocation,
      inventoryBulkToggleActivateSecondLocation: {
        variables: bulkActivateSecondLocationVariables,
        response: bulkActivateSecondLocation,
      },
      downstreamReadAfterInventoryBulkToggleActivateSecondLocation: downstreamReadAfterBulkActivateSecondLocation,
      inventoryBulkToggleDeactivateSecondLocation: {
        variables: bulkDeactivateSecondLocationVariables,
        response: bulkDeactivateSecondLocation,
      },
      downstreamReadAfterInventoryBulkToggleDeactivateSecondLocation: downstreamReadAfterBulkDeactivateSecondLocation,
    };

    await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
    await rm(blockerPath, { force: true });

    console.log(
      JSON.stringify(
        {
          ok: true,
          storeDomain,
          apiVersion,
          locations: locationNodes,
          files: ['inventory-linkage-parity.json'],
        },
        null,
        2,
      ),
    );
  } finally {
    await cleanupTemporaryProducts([
      singleLocationProduct.product,
      directSuccessProduct.product,
      bulkSuccessProduct.product,
    ]);
  }
}

await main();
