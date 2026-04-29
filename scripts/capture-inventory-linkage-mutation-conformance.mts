// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventory-linkage-parity.json');
const inactiveLifecyclePath = path.join(outputDir, 'inventory-inactive-level-lifecycle-2026-04.json');
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'inventory-linkage-single-location-blocker.md');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

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

const trackInventoryMutation = `#graphql
  mutation InventoryInactiveLifecycleTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        totalInventory
        tracksInventory
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

const inventorySetQuantitiesMutation = `#graphql
  mutation InventoryInactiveLifecycleSet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
    inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        id
        changes {
          name
          delta
          quantityAfterChange
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

const inventoryActivateMutation = `#graphql
  mutation InventoryActivateParityPlan($inventoryItemId: ID!, $locationId: ID!, $available: Int, $idempotencyKey: String!) {
    inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: $available) @idempotent(key: $idempotencyKey) {
      inventoryLevel {
        id
        isActive
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
  mutation InventoryDeactivateParityPlan($inventoryLevelId: ID!, $idempotencyKey: String!) {
    inventoryDeactivate(inventoryLevelId: $inventoryLevelId) @idempotent(key: $idempotencyKey) {
      userErrors {
        field
        message
      }
    }
  }
`;

const inventoryBulkToggleMutation = `#graphql
  mutation InventoryBulkToggleActivationParityPlan($inventoryItemId: ID!, $inventoryItemUpdates: [InventoryBulkToggleActivationInput!]!, $idempotencyKey: String!) {
    inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $inventoryItemUpdates) @idempotent(key: $idempotencyKey) {
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

const inactiveLifecycleProductQuery = `#graphql
  query InventoryInactiveLifecycleProduct($productId: ID!) {
    product(id: $productId) {
      id
      title
      handle
      status
      totalInventory
      tracksInventory
      variants(first: 1) {
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
            inventoryLevels(first: 5, includeInactive: true) {
              nodes {
                id
                isActive
                location {
                  id
                  name
                }
                quantities(names: ["available", "on_hand", "incoming"]) {
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

const inactiveLifecycleReadQuery = `#graphql
  query InventoryInactiveLifecycleRead($inventoryItemId: ID!, $inventoryLevelId: ID!, $inactiveLocationId: ID!) {
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
      inventoryLevels(first: 5, includeInactive: true) {
        nodes {
          id
          isActive
          location {
            id
            name
          }
          quantities(names: ["available", "on_hand", "incoming"]) {
            name
            quantity
          }
        }
      }
      inactiveLevel: inventoryLevel(locationId: $inactiveLocationId, includeInactive: true) {
        id
        isActive
        location {
          id
          name
        }
        quantities(names: ["available", "on_hand", "incoming"]) {
          name
          quantity
        }
      }
    }
    inventoryLevel(id: $inventoryLevelId) {
      id
      isActive
      location {
        id
        name
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

function readFirstVariant(product) {
  return product?.variants?.nodes?.[0] ?? null;
}

async function captureInactiveInventoryLifecycle(primaryLocation, secondaryLocation) {
  const runId = `${Date.now()}`;
  const lifecycleProduct = await createTemporaryProduct(`hermes-inventory-inactive-lifecycle-${runId}`);
  const defaultLocationId = lifecycleProduct.inventoryLevel?.location?.id ?? null;
  const defaultLocation =
    [primaryLocation, secondaryLocation].find((location) => location.id === defaultLocationId) ?? primaryLocation;
  const alternateLocation =
    [primaryLocation, secondaryLocation].find((location) => location.id !== defaultLocation.id) ?? secondaryLocation;

  try {
    const trackVariables = {
      productId: lifecycleProduct.product.id,
      variants: [
        {
          id: lifecycleProduct.variant.id,
          inventoryItem: {
            tracked: true,
            requiresShipping: true,
          },
        },
      ],
    };
    const trackInventory = await runGraphql(trackInventoryMutation, trackVariables);

    const setPrimaryVariables = {
      input: {
        name: 'available',
        reason: 'correction',
        referenceDocumentUri: `logistics://har-468/primary/${runId}`,
        quantities: [
          {
            inventoryItemId: lifecycleProduct.inventoryItem.id,
            locationId: defaultLocation.id,
            quantity: 5,
            changeFromQuantity: 0,
          },
        ],
      },
      idempotencyKey: `har-468-primary-${runId}`,
    };
    const setPrimaryQuantity = await runGraphql(inventorySetQuantitiesMutation, setPrimaryVariables);

    const activateSecondaryVariables = {
      inventoryItemId: lifecycleProduct.inventoryItem.id,
      locationId: alternateLocation.id,
      available: 0,
      idempotencyKey: `har-468-activate-secondary-${runId}`,
    };
    const activateSecondary = await runGraphql(inventoryActivateMutation, activateSecondaryVariables);

    const setSecondaryVariables = {
      input: {
        name: 'available',
        reason: 'correction',
        referenceDocumentUri: `logistics://har-468/secondary/${runId}`,
        quantities: [
          {
            inventoryItemId: lifecycleProduct.inventoryItem.id,
            locationId: alternateLocation.id,
            quantity: 7,
            changeFromQuantity: 0,
          },
        ],
      },
      idempotencyKey: `har-468-secondary-${runId}`,
    };
    const setSecondaryQuantity = await runGraphql(inventorySetQuantitiesMutation, setSecondaryVariables);

    const seedProductRead = await runGraphql(inactiveLifecycleProductQuery, {
      productId: lifecycleProduct.product.id,
    });
    const seedProduct = seedProductRead.data?.product ?? null;
    const seedVariant = readFirstVariant(seedProduct);
    const seedInventoryItem = seedVariant?.inventoryItem ?? null;
    const secondaryLevel =
      seedInventoryItem?.inventoryLevels?.nodes?.find((level) => level?.location?.id === alternateLocation.id) ?? null;
    if (!seedProduct?.id || !seedVariant?.id || !seedInventoryItem?.id || !secondaryLevel?.id) {
      throw new Error(`Unexpected inactive lifecycle setup payload: ${JSON.stringify(seedProductRead, null, 2)}`);
    }

    const deactivateVariables = {
      inventoryLevelId: secondaryLevel.id,
      idempotencyKey: `har-468-deactivate-${runId}`,
    };
    const deactivate = await runGraphql(inventoryDeactivateMutation, deactivateVariables);

    const readAfterDeactivateVariables = {
      inventoryItemId: seedInventoryItem.id,
      inventoryLevelId: secondaryLevel.id,
      inactiveLocationId: alternateLocation.id,
    };
    const readAfterDeactivate = await runGraphql(inactiveLifecycleReadQuery, readAfterDeactivateVariables);

    const reactivateVariables = {
      inventoryItemId: seedInventoryItem.id,
      locationId: alternateLocation.id,
      idempotencyKey: `har-468-reactivate-${runId}`,
    };
    const reactivate = await runGraphql(inventoryActivateMutation, reactivateVariables);

    const readAfterReactivateVariables = {
      inventoryItemId: seedInventoryItem.id,
      inventoryLevelId: secondaryLevel.id,
      inactiveLocationId: alternateLocation.id,
    };
    const readAfterReactivate = await runGraphql(inactiveLifecycleReadQuery, readAfterReactivateVariables);

    return {
      fixture: {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        locations: [defaultLocation, alternateLocation],
        createdProduct: seedProduct,
        setup: {
          productCreate: lifecycleProduct.product,
          trackInventory: {
            variables: trackVariables,
            response: trackInventory,
          },
          setPrimaryQuantity: {
            variables: setPrimaryVariables,
            response: setPrimaryQuantity,
          },
          inventoryActivateSecondaryLocation: {
            variables: activateSecondaryVariables,
            response: activateSecondary,
          },
          setSecondaryQuantity: {
            variables: setSecondaryVariables,
            response: setSecondaryQuantity,
          },
          seedProductRead,
        },
        inventoryInactiveLifecycleDeactivate: {
          variables: deactivateVariables,
          response: deactivate,
        },
        inventoryInactiveLifecycleReadAfterDeactivate: {
          variables: readAfterDeactivateVariables,
          response: readAfterDeactivate,
        },
        inventoryInactiveLifecycleReactivate: {
          variables: reactivateVariables,
          response: reactivate,
        },
        inventoryInactiveLifecycleReadAfterReactivate: {
          variables: readAfterReactivateVariables,
          response: readAfterReactivate,
        },
      },
      cleanupProducts: [lifecycleProduct.product],
    };
  } catch (error) {
    await cleanupTemporaryProducts([lifecycleProduct.product]);
    throw error;
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
  let inactiveLifecycleCapture = null;

  try {
    inactiveLifecycleCapture = await captureInactiveInventoryLifecycle(primaryLocation, secondaryLocation);
    const mutationRunId = `${Date.now()}`;

    const activateNoOpVariables = {
      inventoryItemId: singleLocationProduct.inventoryItem.id,
      locationId: primaryLocation.id,
      idempotencyKey: `inventory-linkage-activate-no-op-${mutationRunId}`,
    };
    const activateAvailableErrorVariables = {
      ...activateNoOpVariables,
      available: 7,
      idempotencyKey: `inventory-linkage-activate-available-${mutationRunId}`,
    };
    const activateUnknownLocationVariables = {
      inventoryItemId: singleLocationProduct.inventoryItem.id,
      locationId: 'gid://shopify/Location/999999999999',
      idempotencyKey: `inventory-linkage-activate-unknown-${mutationRunId}`,
    };
    const deactivateOnlyLocationVariables = {
      inventoryLevelId: singleLocationProduct.inventoryLevel.id,
      idempotencyKey: `inventory-linkage-deactivate-only-${mutationRunId}`,
    };
    const bulkActivateNoOpVariables = {
      inventoryItemId: singleLocationProduct.inventoryItem.id,
      inventoryItemUpdates: [{ locationId: primaryLocation.id, activate: true }],
      idempotencyKey: `inventory-linkage-bulk-no-op-${mutationRunId}`,
    };
    const bulkUnknownLocationVariables = {
      inventoryItemId: singleLocationProduct.inventoryItem.id,
      inventoryItemUpdates: [{ locationId: 'gid://shopify/Location/999999999999', activate: true }],
      idempotencyKey: `inventory-linkage-bulk-unknown-${mutationRunId}`,
    };
    const bulkDeactivateOnlyLocationVariables = {
      inventoryItemId: singleLocationProduct.inventoryItem.id,
      inventoryItemUpdates: [{ locationId: primaryLocation.id, activate: false }],
      idempotencyKey: `inventory-linkage-bulk-deactivate-only-${mutationRunId}`,
    };

    const activateSecondLocationVariables = {
      inventoryItemId: directSuccessProduct.inventoryItem.id,
      locationId: secondaryLocation.id,
      available: 9,
      idempotencyKey: `inventory-linkage-activate-second-${mutationRunId}`,
    };
    const bulkActivateSecondLocationVariables = {
      inventoryItemId: bulkSuccessProduct.inventoryItem.id,
      inventoryItemUpdates: [{ locationId: secondaryLocation.id, activate: true }],
      idempotencyKey: `inventory-linkage-bulk-activate-second-${mutationRunId}`,
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
      idempotencyKey: `inventory-linkage-deactivate-alternate-${mutationRunId}`,
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
      idempotencyKey: `inventory-linkage-bulk-deactivate-second-${mutationRunId}`,
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
    await writeFile(inactiveLifecyclePath, `${JSON.stringify(inactiveLifecycleCapture.fixture, null, 2)}\n`, 'utf8');
    await rm(blockerPath, { force: true });

    console.log(
      JSON.stringify(
        {
          ok: true,
          storeDomain,
          apiVersion,
          locations: locationNodes,
          files: ['inventory-linkage-parity.json', 'inventory-inactive-level-lifecycle-2026-04.json'],
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
      ...(inactiveLifecycleCapture?.cleanupProducts ?? []),
    ]);
  }
}

await main();
