// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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
const outputPath = path.join(outputDir, 'inventory-deactivate-validation-2026-04.json');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createProductMutation = `#graphql
  mutation InventoryDeactivateValidationProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
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
            inventoryQuantity
            selectedOptions { name value }
            inventoryItem {
              id
              tracked
              requiresShipping
              inventoryLevels(first: 10) {
                nodes {
                  id
                  isActive
                  location { id name }
                  quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) {
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
      userErrors { field message }
    }
  }
`;

const locationsQuery = `#graphql
  query InventoryDeactivateValidationLocations {
    locations(first: 10) {
      nodes { id name isActive }
    }
  }
`;

const trackInventoryMutation = `#graphql
  mutation InventoryDeactivateValidationTrack($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        totalInventory
        tracksInventory
      }
      productVariants {
        id
        inventoryQuantity
        inventoryItem { id tracked requiresShipping }
      }
      userErrors { field message }
    }
  }
`;

const inventorySetQuantitiesMutation = `#graphql
  mutation InventoryDeactivateValidationSet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
    inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        id
        changes {
          name
          delta
          quantityAfterChange
          item { id }
          location { id name }
        }
      }
      userErrors { field message code }
    }
  }
`;

const inventoryActivateMutation = `#graphql
  mutation InventoryDeactivateValidationActivate($inventoryItemId: ID!, $locationId: ID!, $available: Int, $idempotencyKey: String!) {
    inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: $available) @idempotent(key: $idempotencyKey) {
      inventoryLevel {
        id
        isActive
        location { id name }
        item { id }
      }
      userErrors { field message }
    }
  }
`;

const inventoryDeactivateMutation = `#graphql
  mutation InventoryDeactivateValidationDeactivate($inventoryLevelId: ID!, $idempotencyKey: String!) {
    inventoryDeactivate(inventoryLevelId: $inventoryLevelId) @idempotent(key: $idempotencyKey) {
      userErrors { field message }
    }
  }
`;

const inventoryTransferCreateReadyMutation = `#graphql
  mutation InventoryDeactivateValidationTransferReady($input: InventoryTransferCreateAsReadyToShipInput!, $idempotencyKey: String!) {
    inventoryTransferCreateAsReadyToShip(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryTransfer {
        id
        status
        totalQuantity
        lineItems(first: 10) {
          nodes {
            id
            totalQuantity
            shippableQuantity
            shippedQuantity
            processableQuantity
            pickedForShipmentQuantity
          }
        }
      }
      userErrors { field message code }
    }
  }
`;

const inventoryShipmentCreateInTransitMutation = `#graphql
  mutation InventoryDeactivateValidationShipmentInTransit($input: InventoryShipmentCreateInput!, $idempotencyKey: String!) {
    inventoryShipmentCreateInTransit(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryShipment {
        id
        name
        status
        lineItemTotalQuantity
        totalAcceptedQuantity
        totalReceivedQuantity
        totalRejectedQuantity
        lineItems(first: 10) {
          nodes {
            id
            quantity
            unreceivedQuantity
            inventoryItem { id tracked }
          }
        }
      }
      userErrors { field message code }
    }
  }
`;

const orderCreateMutation = `#graphql
  mutation InventoryDeactivateValidationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        lineItems(first: 5) {
          nodes {
            id
            quantity
            variant { id }
          }
        }
      }
      userErrors { field message code }
    }
  }
`;

const productReadQuery = `#graphql
  query InventoryDeactivateValidationProduct($productId: ID!) {
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
          inventoryQuantity
          selectedOptions { name value }
          inventoryItem {
            id
            tracked
            requiresShipping
            inventoryLevels(first: 10, includeInactive: true) {
              nodes {
                id
                isActive
                location { id name }
                quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) {
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
  }
`;

const deleteProductMutation = `#graphql
  mutation InventoryDeactivateValidationDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation InventoryDeactivateValidationCancelOrder($orderId: ID!) {
    orderCancel(orderId: $orderId, reason: OTHER, notifyCustomer: false, restock: true) {
      job { id done }
      orderCancelUserErrors { field message code }
      userErrors { field message }
    }
  }
`;

function firstVariant(product) {
  return product?.variants?.nodes?.[0] ?? null;
}

function firstInventoryItem(product) {
  return firstVariant(product)?.inventoryItem ?? null;
}

function levelForLocation(product, locationId) {
  return (
    firstInventoryItem(product)?.inventoryLevels?.nodes?.find((level) => level?.location?.id === locationId) ?? null
  );
}

function inventoryItemLegacyId(itemId) {
  return itemId?.split('/').pop() ?? null;
}

function hydrateLevelCall(level, product) {
  return {
    operationName: 'ProductsHydrateNodes',
    variables: { ids: [level.id] },
    query: 'hand-synthesized from HAR-591 live inventoryDeactivate validation capture',
    response: {
      status: 200,
      body: {
        data: {
          nodes: [inventoryLevelNode(level, product)],
        },
      },
    },
  };
}

function hydrateItemCall(item, product) {
  return {
    operationName: 'ProductsHydrateNodes',
    variables: { ids: [item.id] },
    query: 'hand-synthesized from HAR-591 live inventoryActivate validation capture',
    response: {
      status: 200,
      body: {
        data: {
          nodes: [inventoryItemNode(item, product)],
        },
      },
    },
  };
}

function hydrateMissingLevelCall(inventoryLevelId) {
  return {
    operationName: 'ProductsHydrateNodes',
    variables: { ids: [inventoryLevelId] },
    query: 'hand-synthesized from HAR-591 fabricated inventoryLevelId capture',
    response: {
      status: 200,
      body: {
        data: {
          nodes: [null],
        },
      },
    },
  };
}

function inventoryLevelNode(level, product) {
  const variant = firstVariant(product);
  const item = firstInventoryItem(product);
  return {
    id: level.id,
    isActive: level.isActive,
    location: level.location,
    quantities: level.quantities,
    item: {
      id: item.id,
      tracked: item.tracked,
      requiresShipping: item.requiresShipping,
      inventoryLevels: item.inventoryLevels,
      variant: variantNode(variant, product),
    },
  };
}

function inventoryItemNode(item, product) {
  const variant = firstVariant(product);
  return {
    id: item.id,
    tracked: item.tracked,
    requiresShipping: item.requiresShipping,
    inventoryLevels: item.inventoryLevels,
    variant: variantNode(variant, product),
  };
}

function variantNode(variant, product) {
  return {
    id: variant.id,
    title: variant.title,
    inventoryQuantity: variant.inventoryQuantity,
    selectedOptions: variant.selectedOptions,
    product: {
      id: product.id,
      title: product.title,
      handle: product.handle,
      status: product.status,
      totalInventory: product.totalInventory,
      tracksInventory: product.tracksInventory,
    },
  };
}

async function createTemporaryProduct(title) {
  const response = await runGraphql(createProductMutation, {
    product: {
      title,
      status: 'ACTIVE',
    },
  });
  const product = response.data?.productCreate?.product ?? null;
  const errors = response.data?.productCreate?.userErrors ?? [];
  if (!product?.id || errors.length > 0) {
    throw new Error(`productCreate failed: ${JSON.stringify(response, null, 2)}`);
  }
  return product;
}

async function trackedProductWithLocations(label, primaryLocation, secondaryLocation, runId) {
  const created = await createTemporaryProduct(`hermes-har-591-${label}-${runId}`);
  const variant = firstVariant(created);
  const item = firstInventoryItem(created);
  if (!variant?.id || !item?.id) {
    throw new Error(`Unexpected productCreate payload: ${JSON.stringify(created, null, 2)}`);
  }

  const trackVariables = {
    productId: created.id,
    variants: [
      {
        id: variant.id,
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
      referenceDocumentUri: `logistics://har-591/${label}/primary/${runId}`,
      quantities: [
        {
          inventoryItemId: item.id,
          locationId: primaryLocation.id,
          quantity: 5,
          changeFromQuantity: 0,
        },
      ],
    },
    idempotencyKey: `har-591-${label}-primary-${runId}`,
  };
  const setPrimaryQuantity = await runGraphql(inventorySetQuantitiesMutation, setPrimaryVariables);

  let activateSecondaryVariables = null;
  let activateSecondary = null;
  let setSecondaryVariables = null;
  let setSecondaryQuantity = null;
  if (secondaryLocation) {
    activateSecondaryVariables = {
      inventoryItemId: item.id,
      locationId: secondaryLocation.id,
      available: 0,
      idempotencyKey: `har-591-${label}-activate-secondary-${runId}`,
    };
    activateSecondary = await runGraphql(inventoryActivateMutation, activateSecondaryVariables);

    setSecondaryVariables = {
      input: {
        name: 'available',
        reason: 'correction',
        referenceDocumentUri: `logistics://har-591/${label}/secondary/${runId}`,
        quantities: [
          {
            inventoryItemId: item.id,
            locationId: secondaryLocation.id,
            quantity: 3,
            changeFromQuantity: 0,
          },
        ],
      },
      idempotencyKey: `har-591-${label}-secondary-${runId}`,
    };
    setSecondaryQuantity = await runGraphql(inventorySetQuantitiesMutation, setSecondaryVariables);
  }

  const productRead = await runGraphql(productReadQuery, { productId: created.id });
  const product = productRead.data?.product ?? null;
  const refreshedVariant = firstVariant(product);
  const refreshedItem = firstInventoryItem(product);
  const primaryLevel = levelForLocation(product, primaryLocation.id);
  const secondaryLevel = secondaryLocation ? levelForLocation(product, secondaryLocation.id) : null;
  if (!product?.id || !refreshedVariant?.id || !refreshedItem?.id || !primaryLevel?.id) {
    throw new Error(`Unexpected setup product payload: ${JSON.stringify(productRead, null, 2)}`);
  }
  if (secondaryLocation && !secondaryLevel?.id) {
    throw new Error(`Secondary level did not activate: ${JSON.stringify(productRead, null, 2)}`);
  }

  return {
    product,
    variant: refreshedVariant,
    inventoryItem: refreshedItem,
    primaryLevel,
    secondaryLevel,
    setup: {
      productCreate: created,
      trackInventory: { variables: trackVariables, response: trackInventory },
      setPrimaryQuantity: { variables: setPrimaryVariables, response: setPrimaryQuantity },
      inventoryActivateSecondaryLocation: activateSecondaryVariables
        ? { variables: activateSecondaryVariables, response: activateSecondary }
        : null,
      setSecondaryQuantity: setSecondaryVariables
        ? { variables: setSecondaryVariables, response: setSecondaryQuantity }
        : null,
      productRead,
    },
  };
}

async function createReadyTransfer({
  inventoryItemId,
  originLocationId,
  destinationLocationId,
  quantity,
  label,
  runId,
}) {
  const variables = {
    input: {
      originLocationId,
      destinationLocationId,
      referenceName: `HAR-591-${label}-${runId}`,
      note: `HAR-591 ${label}`,
      tags: ['har-591', label],
      lineItems: [
        {
          inventoryItemId,
          quantity,
        },
      ],
    },
    idempotencyKey: `har-591-transfer-${label}-${runId}`,
  };
  const response = await runGraphql(inventoryTransferCreateReadyMutation, variables);
  return { variables, response };
}

async function createInTransitShipment({ movementId, inventoryItemId, quantity, label, runId }) {
  const variables = {
    input: {
      movementId,
      trackingInput: {
        trackingNumber: `HAR591${runId}`,
        company: 'Hermes',
        trackingUrl: 'https://example.test/har-591',
        arrivesAt: '2026-06-01T00:00:00.000Z',
      },
      lineItems: [
        {
          inventoryItemId,
          quantity,
        },
      ],
    },
    idempotencyKey: `har-591-shipment-${label}-${runId}`,
  };
  const response = await runGraphql(inventoryShipmentCreateInTransitMutation, variables);
  return { variables, response };
}

async function createCommittedOrder({ variantId, quantity, label, runId }) {
  const variables = {
    order: {
      email: `har-591-${label}-${runId}@example.com`,
      test: true,
      currency: 'CAD',
      lineItems: [
        {
          variantId,
          quantity,
          title: `HAR-591 ${label}`,
          priceSet: {
            shopMoney: {
              amount: '1.00',
              currencyCode: 'CAD',
            },
          },
        },
      ],
      transactions: [
        {
          kind: 'SALE',
          status: 'SUCCESS',
          gateway: 'manual',
          test: true,
          amountSet: {
            shopMoney: {
              amount: `${quantity}.00`,
              currencyCode: 'CAD',
            },
          },
        },
      ],
    },
    options: {
      inventoryBehaviour: 'DECREMENT_IGNORING_POLICY',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
  const response = await runGraphql(orderCreateMutation, variables);
  return { variables, response };
}

async function deleteProduct(product) {
  if (!product?.id) return null;
  try {
    return await runGraphql(deleteProductMutation, { input: { id: product.id } });
  } catch (error) {
    console.error(`Failed to delete ${product.id}:`, error);
    return null;
  }
}

async function cancelOrder(order) {
  const orderId = order?.response?.data?.orderCreate?.order?.id ?? null;
  if (!orderId) return null;
  try {
    return await runGraphql(orderCancelMutation, { orderId });
  } catch (error) {
    console.error(`Failed to cancel ${orderId}:`, error);
    return null;
  }
}

async function main() {
  await mkdir(outputDir, { recursive: true });
  const runId = `${Date.now()}`;
  const locations = await runGraphql(locationsQuery);
  const locationNodes = Array.isArray(locations.data?.locations?.nodes) ? locations.data.locations.nodes : [];
  const activeLocationNodes = locationNodes.filter((location) => location?.isActive !== false);
  const preferredPrimary = activeLocationNodes.find((location) => location?.name === 'Shop location') ?? null;
  const preferredSecondary = activeLocationNodes.find((location) => location?.name === 'My Custom Location') ?? null;
  const primaryLocation = preferredPrimary ?? activeLocationNodes[0] ?? null;
  const secondaryLocation =
    preferredSecondary ?? activeLocationNodes.find((location) => location?.id !== primaryLocation?.id) ?? null;
  if (!primaryLocation?.id || !secondaryLocation?.id) {
    throw new Error(
      `HAR-591 capture requires at least two active locations: ${JSON.stringify(locationNodes, null, 2)}`,
    );
  }

  const cleanupProducts = [];
  const cleanupOrders = [];
  try {
    const committed = await trackedProductWithLocations('committed', primaryLocation, secondaryLocation, runId);
    cleanupProducts.push(committed.product);
    const committedOrder = await createCommittedOrder({
      variantId: committed.variant.id,
      quantity: 2,
      label: 'committed',
      runId,
    });
    cleanupOrders.push(committedOrder);
    const committedProductRead = await runGraphql(productReadQuery, { productId: committed.product.id });
    const committedProduct = committedProductRead.data?.product;
    const committedLevel = levelForLocation(committedProduct, primaryLocation.id);
    const deactivateCommittedVariables = {
      inventoryLevelId: committedLevel.id,
      idempotencyKey: `har-591-deactivate-committed-${runId}`,
    };
    const deactivateCommitted = await runGraphql(inventoryDeactivateMutation, deactivateCommittedVariables);

    const incomingReserved = await trackedProductWithLocations(
      'incoming-reserved',
      primaryLocation,
      secondaryLocation,
      runId,
    );
    cleanupProducts.push(incomingReserved.product);
    const reservedTransfer = await createReadyTransfer({
      inventoryItemId: incomingReserved.inventoryItem.id,
      originLocationId: primaryLocation.id,
      destinationLocationId: secondaryLocation.id,
      quantity: 2,
      label: 'incoming-reserved',
      runId,
    });
    const incomingTransfer = await createReadyTransfer({
      inventoryItemId: incomingReserved.inventoryItem.id,
      originLocationId: secondaryLocation.id,
      destinationLocationId: primaryLocation.id,
      quantity: 1,
      label: 'incoming-reserved-reverse',
      runId,
    });
    const incomingTransferId =
      incomingTransfer.response.data?.inventoryTransferCreateAsReadyToShip?.inventoryTransfer?.id ?? null;
    const incomingShipment = incomingTransferId
      ? await createInTransitShipment({
          movementId: incomingTransferId,
          inventoryItemId: incomingReserved.inventoryItem.id,
          quantity: 1,
          label: 'incoming-reserved',
          runId,
        })
      : null;
    const incomingReservedProductRead = await runGraphql(productReadQuery, { productId: incomingReserved.product.id });
    const incomingReservedProduct = incomingReservedProductRead.data?.product;
    const incomingReservedLevel = levelForLocation(incomingReservedProduct, primaryLocation.id);
    const deactivateIncomingReservedVariables = {
      inventoryLevelId: incomingReservedLevel.id,
      idempotencyKey: `har-591-deactivate-incoming-reserved-${runId}`,
    };
    const deactivateIncomingReserved = await runGraphql(
      inventoryDeactivateMutation,
      deactivateIncomingReservedVariables,
    );

    const onlyLocation = await trackedProductWithLocations('only-location', primaryLocation, null, runId);
    cleanupProducts.push(onlyLocation.product);
    const deactivateOnlyLocationVariables = {
      inventoryLevelId: onlyLocation.primaryLevel.id,
      idempotencyKey: `har-591-deactivate-only-location-${runId}`,
    };
    const deactivateOnlyLocation = await runGraphql(inventoryDeactivateMutation, deactivateOnlyLocationVariables);

    const fabricatedInventoryLevelId = 'gid://shopify/InventoryLevel/999999999999?inventory_item_id=999999999998';
    const deactivateMissingVariables = {
      inventoryLevelId: fabricatedInventoryLevelId,
      idempotencyKey: `har-591-deactivate-missing-${runId}`,
    };
    const deactivateMissing = await runGraphql(inventoryDeactivateMutation, deactivateMissingVariables);

    const committedItemLegacyId = inventoryItemLegacyId(committed.inventoryItem.id);
    const deletedLocationInventoryLevelId = `gid://shopify/InventoryLevel/999999999999?inventory_item_id=${committedItemLegacyId}`;
    const deactivateDeletedLocationVariables = {
      inventoryLevelId: deletedLocationInventoryLevelId,
      idempotencyKey: `har-591-deactivate-deleted-location-${runId}`,
    };
    const deactivateDeletedLocation = await runGraphql(inventoryDeactivateMutation, deactivateDeletedLocationVariables);

    const activateActiveVariables = {
      inventoryItemId: onlyLocation.inventoryItem.id,
      locationId: primaryLocation.id,
      available: 7,
      idempotencyKey: `har-591-activate-active-available-${runId}`,
    };
    const activateActiveAvailable = await runGraphql(inventoryActivateMutation, activateActiveVariables);

    const reactivate = await trackedProductWithLocations('reactivate', primaryLocation, secondaryLocation, runId);
    cleanupProducts.push(reactivate.product);
    const deactivateForReactivateVariables = {
      inventoryLevelId: reactivate.secondaryLevel.id,
      idempotencyKey: `har-591-deactivate-before-reactivate-${runId}`,
    };
    const deactivateForReactivate = await runGraphql(inventoryDeactivateMutation, deactivateForReactivateVariables);
    const reactivateVariables = {
      inventoryItemId: reactivate.inventoryItem.id,
      locationId: secondaryLocation.id,
      available: 7,
      idempotencyKey: `har-591-reactivate-inactive-available-${runId}`,
    };
    const reactivateInactiveAvailable = await runGraphql(inventoryActivateMutation, reactivateVariables);

    const fixture = {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      locations: locationNodes,
      setup: {
        committed: committed.setup,
        incomingReserved: incomingReserved.setup,
        onlyLocation: onlyLocation.setup,
        reactivate: reactivate.setup,
      },
      adjustments: {
        committedOrder,
        reservedTransfer,
        incomingTransfer,
        incomingShipment,
      },
      productReads: {
        committedAfterAdjust: committedProductRead,
        incomingReservedAfterAdjust: incomingReservedProductRead,
      },
      inventoryDeactivateCommitted: {
        variables: deactivateCommittedVariables,
        response: deactivateCommitted,
      },
      inventoryDeactivateIncomingReserved: {
        variables: deactivateIncomingReservedVariables,
        response: deactivateIncomingReserved,
      },
      inventoryDeactivateOnlyLocation: {
        variables: deactivateOnlyLocationVariables,
        response: deactivateOnlyLocation,
      },
      inventoryDeactivateMissingLevel: {
        variables: deactivateMissingVariables,
        response: deactivateMissing,
      },
      inventoryDeactivateDeletedLocation: {
        variables: deactivateDeletedLocationVariables,
        response: deactivateDeletedLocation,
      },
      inventoryActivateActiveAvailable: {
        variables: activateActiveVariables,
        response: activateActiveAvailable,
      },
      inventoryDeactivateBeforeReactivate: {
        variables: deactivateForReactivateVariables,
        response: deactivateForReactivate,
      },
      inventoryActivateInactiveAvailable: {
        variables: reactivateVariables,
        response: reactivateInactiveAvailable,
      },
      cleanup: {},
      upstreamCalls: [
        hydrateLevelCall(committedLevel, committedProduct),
        hydrateLevelCall(incomingReservedLevel, incomingReservedProduct),
        hydrateLevelCall(onlyLocation.primaryLevel, onlyLocation.product),
        hydrateMissingLevelCall(fabricatedInventoryLevelId),
        hydrateMissingLevelCall(deletedLocationInventoryLevelId),
        hydrateItemCall(onlyLocation.inventoryItem, onlyLocation.product),
        hydrateLevelCall(reactivate.secondaryLevel, reactivate.product),
      ],
    };

    await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
    console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
  } finally {
    const orderCleanup = [];
    for (const order of cleanupOrders.reverse()) {
      orderCleanup.push({
        orderId: order.response?.data?.orderCreate?.order?.id ?? null,
        response: await cancelOrder(order),
      });
    }
    const cleanup = [];
    for (const product of cleanupProducts.reverse()) {
      cleanup.push({ productId: product.id, response: await deleteProduct(product) });
    }
    console.log(JSON.stringify({ orderCleanup, cleanup }, null, 2));
  }
}

await main();
