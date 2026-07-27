/* oxlint-disable no-console -- CLI capture scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type GraphqlPayload = JsonRecord;
type GraphqlVariables = Record<string, unknown>;

type ProductSetup = {
  productId: string;
  variantId: string;
  inventoryItemId: string;
  create: GraphqlPayload;
  track: GraphqlPayload;
  beforeInventoryRead: GraphqlPayload;
};

type ShipmentSetup = ProductSetup & {
  originLocation: { id: string; name: string };
  destinationLocation: { id: string; name: string };
  originLocationCreate: GraphqlPayload;
  destinationLocationCreate: GraphqlPayload;
  originActivation: GraphqlPayload;
  destinationActivation: GraphqlPayload;
  inventorySet: GraphqlPayload;
};

type UpstreamCall = {
  operationName: string;
  variables: { ids: string[] };
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
if (apiVersion !== '2025-01') {
  throw new Error(
    `inventory default-location/carrier capture requires SHOPIFY_CONFORMANCE_API_VERSION=2025-01, got ${apiVersion}`,
  );
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'inventory-default-location-carrier-connection.json');

const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlAllowGraphqlErrors(query: string, variables: GraphqlVariables = {}): Promise<GraphqlPayload> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(JSON.stringify(result, null, 2));
  }
  return result.payload as GraphqlPayload;
}

const createProductMutation = `#graphql
  mutation InventoryDefaultCarrierProductCreate($product: ProductCreateInput!) {
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
            sku
            inventoryQuantity
            selectedOptions { name value }
            inventoryItem {
              id
              tracked
              requiresShipping
            }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const trackInventoryMutation = `#graphql
  mutation InventoryDefaultCarrierTrackInventory($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        totalInventory
        tracksInventory
      }
      productVariants {
        id
        sku
        inventoryQuantity
        inventoryItem {
          id
          tracked
          requiresShipping
        }
      }
      userErrors { field message }
    }
  }
`;

const inventoryItemUpdateMutation = `#graphql
  mutation InventoryDefaultLocationItemUpdate($id: ID!, $input: InventoryItemInput!) {
    inventoryItemUpdate(id: $id, input: $input) {
      inventoryItem {
        id
        tracked
        requiresShipping
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

const inventoryItemsReadQuery = `#graphql
  query InventoryItemsByQuery($query: String!) {
    inventoryItems(first: 5, query: $query) {
      nodes {
        id
        tracked
        requiresShipping
        variant {
          id
          inventoryQuantity
          product {
            id
            title
            tracksInventory
          }
        }
        inventoryLevels(first: 10) {
          nodes {
            location {
              id
              name
            }
            quantities(names: ["available", "on_hand"]) {
              name
              quantity
            }
          }
        }
      }
    }
  }
`;

const inventoryItemReadQuery = `#graphql
  query InventoryDefaultLocationItemRead($item: ID!) {
    inventoryItem(id: $item) {
      id
      variant {
        id
        inventoryQuantity
      }
      inventoryLevels(first: 10) {
        nodes {
          location {
            id
            name
          }
          quantities(names: ["available", "on_hand"]) {
            name
            quantity
          }
        }
      }
    }
  }
`;

const productHydrateNodesQuery = `#graphql
  query ProductsHydrateNodes($ids: [ID!]!) {
    nodes(ids: $ids) {
      __typename
      id
      ... on InventoryItem {
        tracked
        requiresShipping
        measurement { weight { unit value } }
        variant {
          id
          title
          inventoryQuantity
          selectedOptions { name value }
          product {
            id
            title
            handle
            status
            totalInventory
            tracksInventory
          }
        }
        inventoryLevels(first: 50) {
          nodes {
            id
            location { id name }
            quantities(names: ["available", "on_hand", "committed", "incoming", "reserved", "damaged", "quality_control", "safety_stock"]) {
              name
              quantity
              updatedAt
            }
          }
        }
      }
      ... on Location {
        id
        name
        isActive
      }
    }
  }
`;

const parityRunnerInventoryHydrateNodesQuery =
  'query ProductsHydrateNodes($ids: [ID!]!) { nodes(ids: $ids) { ... on InventoryItem { id tracked requiresShipping countryCodeOfOrigin provinceCodeOfOrigin harmonizedSystemCode measurement { weight { value unit } } variant { id title inventoryQuantity selectedOptions { name value } product { id title handle status totalInventory tracksInventory } } inventoryLevels(first: 10, includeInactive: true) { nodes { id isActive location { id name } quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) { name quantity updatedAt } } } } ... on InventoryLevel { id isActive location { id name } quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) { name quantity updatedAt } item { id tracked requiresShipping variant { id title inventoryQuantity selectedOptions { name value } product { id title handle status totalInventory tracksInventory } } inventoryLevels(first: 10, includeInactive: true) { nodes { id isActive location { id name } quantities(names: ["available", "on_hand", "committed", "incoming", "reserved"]) { name quantity updatedAt } } } } } } }';

const orderCreateInventoryPreflightQuery = (
  await readFile('config/parity-requests/orders/order-create-inventory-preflight.graphql', 'utf8')
).trimEnd();
const coldVariantProductReadQuery = (
  await readFile('config/parity-requests/products/inventory-cold-variant-product-read.graphql', 'utf8')
).trimEnd();

const orderCreateMutation = `#graphql
  mutation InventoryDefaultLocationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        lineItems(first: 5) {
          nodes {
            id
            variant {
              id
            }
            quantity
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

const orderReadQuery = `#graphql
  query InventoryColdVariantOrderRead($id: ID!) {
    order(id: $id) {
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
  }
`;

const orderCancelMutation = `#graphql
  mutation InventoryDefaultLocationOrderCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job { id done }
      userErrors { field message }
    }
  }
`;

const locationAddMutation = `#graphql
  mutation InventoryDefaultCarrierLocationAdd($input: LocationAddInput!) {
    locationAdd(input: $input) {
      location {
        id
        name
        isActive
      }
      userErrors { field message }
    }
  }
`;

const locationDeactivateMutation = `#graphql
  mutation InventoryDefaultCarrierLocationDeactivate($locationId: ID!, $destinationLocationId: ID) {
    locationDeactivate(locationId: $locationId, destinationLocationId: $destinationLocationId) {
      location {
        id
        isActive
      }
      locationDeactivateUserErrors { field message }
    }
  }
`;

const locationDeleteMutation = `#graphql
  mutation InventoryDefaultCarrierLocationDelete($locationId: ID!) {
    locationDelete(locationId: $locationId) {
      deletedLocationId
      locationDeleteUserErrors { field message }
    }
  }
`;

const inventoryActivateMutation = `#graphql
  mutation InventoryDefaultCarrierInventoryActivate(
    $inventoryItemId: ID!
    $locationId: ID!
    $available: Int
  ) {
    inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: $available) {
      inventoryLevel {
        id
        location { id name }
        quantities(names: ["available", "on_hand"]) { name quantity }
      }
      userErrors { field message }
    }
  }
`;

const inventorySetQuantitiesMutation = `#graphql
  mutation InventoryDefaultCarrierInventorySet($input: InventorySetQuantitiesInput!) {
    inventorySetQuantities(input: $input) {
      inventoryAdjustmentGroup {
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

const inventoryTransferCreateMutation = `#graphql
  mutation InventoryTransferCreateParity($input: InventoryTransferCreateInput!) {
    inventoryTransferCreate(input: $input) {
      inventoryTransfer {
        id
        name
        status
        totalQuantity
        lineItems(first: 10) {
          nodes {
            totalQuantity
            shippableQuantity
            shippedQuantity
            processableQuantity
            pickedForShipmentQuantity
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

const inventoryTransferMarkReadyMutation = `#graphql
  mutation InventoryTransferMarkReadyParity($id: ID!) {
    inventoryTransferMarkAsReadyToShip(id: $id) {
      inventoryTransfer {
        status
        totalQuantity
        lineItems(first: 10) {
          nodes {
            totalQuantity
            shippableQuantity
            shippedQuantity
            processableQuantity
            pickedForShipmentQuantity
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

const inventoryShipmentCreateMutation = `#graphql
  mutation InventoryShipmentCreateCarrier($input: InventoryShipmentCreateInput!) {
    inventoryShipmentCreate(input: $input) {
      inventoryShipment {
        id
        name
        status
        lineItemTotalQuantity
        totalAcceptedQuantity
        totalReceivedQuantity
        totalRejectedQuantity
        tracking {
          trackingNumber
          company
          trackingUrl
          arrivesAt
        }
        lineItems(first: 10) {
          nodes {
            id
            quantity
            acceptedQuantity
            rejectedQuantity
            unreceivedQuantity
            inventoryItem {
              id
              sku
              tracked
            }
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

const inventoryShipmentCreateInTransitMutation = `#graphql
  mutation InventoryShipmentCreateInTransitParity($input: InventoryShipmentCreateInput!) {
    inventoryShipmentCreateInTransit(input: $input) {
      inventoryShipment {
        id
        name
        status
        lineItemTotalQuantity
        totalAcceptedQuantity
        totalReceivedQuantity
        totalRejectedQuantity
        tracking {
          trackingNumber
          company
          trackingUrl
          arrivesAt
        }
        lineItems(first: 10) {
          nodes {
            id
            quantity
            acceptedQuantity
            rejectedQuantity
            unreceivedQuantity
            inventoryItem {
              id
              sku
              tracked
            }
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

const inventoryTransferReadAfterShipmentQuery = `#graphql
  query InventoryTransferReadAfterShipmentParity($id: ID!) {
    inventoryTransfer(id: $id) {
      status
      totalQuantity
      lineItems(first: 10) {
        nodes {
          totalQuantity
          shippableQuantity
          shippedQuantity
          processableQuantity
          pickedForShipmentQuantity
        }
      }
    }
  }
`;

const inventoryShipmentDeleteMutation = `#graphql
  mutation InventoryDefaultCarrierShipmentDelete($id: ID!) {
    inventoryShipmentDelete(id: $id) {
      id
      userErrors { field message code }
    }
  }
`;

const inventoryTransferCancelMutation = `#graphql
  mutation InventoryDefaultCarrierTransferCancel($id: ID!) {
    inventoryTransferCancel(id: $id) {
      inventoryTransfer { id status }
      userErrors { field message code }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation InventoryDefaultCarrierProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

function readRecord(value: unknown, label: string): JsonRecord {
  if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
    return value as JsonRecord;
  }
  throw new Error(`${label} was not an object: ${JSON.stringify(value)}`);
}

function readPath(value: unknown, pathSegments: Array<string | number>, label: string): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      current = current[Number(segment)];
      continue;
    }
    current = readRecord(current, label)[String(segment)];
  }
  return current;
}

function readStringPath(value: unknown, pathSegments: Array<string | number>, label: string): string {
  const candidate = readPath(value, pathSegments, label);
  if (typeof candidate === 'string' && candidate.length > 0) {
    return candidate;
  }
  throw new Error(`${label} was missing string path ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
}

function readOptionalStringPath(value: unknown, pathSegments: Array<string | number>): string | null {
  try {
    const candidate = readPath(value, pathSegments, 'optional string path');
    return typeof candidate === 'string' && candidate.length > 0 ? candidate : null;
  } catch {
    return null;
  }
}

function readArrayPath(value: unknown, pathSegments: Array<string | number>, label: string): unknown[] {
  const candidate = readPath(value, pathSegments, label);
  return Array.isArray(candidate) ? candidate : [];
}

function expectNoUserErrors(payload: unknown, pathSegments: Array<string | number>, label: string): void {
  const errors = readArrayPath(payload, pathSegments, label);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function resourceTail(gid: string): string {
  return gid.split('/').at(-1) ?? gid;
}

function readCreatedProduct(payload: GraphqlPayload): Omit<ProductSetup, 'create' | 'track' | 'beforeInventoryRead'> {
  return {
    productId: readStringPath(payload, ['data', 'productCreate', 'product', 'id'], 'productCreate'),
    variantId: readStringPath(
      payload,
      ['data', 'productCreate', 'product', 'variants', 'nodes', 0, 'id'],
      'productCreate',
    ),
    inventoryItemId: readStringPath(
      payload,
      ['data', 'productCreate', 'product', 'variants', 'nodes', 0, 'inventoryItem', 'id'],
      'productCreate',
    ),
  };
}

function readCreatedLocation(payload: GraphqlPayload): { id: string; name: string } {
  return {
    id: readStringPath(payload, ['data', 'locationAdd', 'location', 'id'], 'locationAdd'),
    name: readStringPath(payload, ['data', 'locationAdd', 'location', 'name'], 'locationAdd'),
  };
}

async function deleteProduct(productId: string | null): Promise<GraphqlPayload | null> {
  if (!productId) return null;
  try {
    return await runGraphqlAllowGraphqlErrors(productDeleteMutation, { input: { id: productId } });
  } catch (error) {
    console.warn(`Product cleanup failed for ${productId}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

async function cancelOrder(orderId: string | null): Promise<GraphqlPayload | null> {
  if (!orderId) return null;
  try {
    return await runGraphqlAllowGraphqlErrors(orderCancelMutation, {
      orderId,
      reason: 'OTHER',
      notifyCustomer: false,
      restock: true,
    });
  } catch (error) {
    console.warn(`Order cleanup failed for ${orderId}: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
}

async function cleanupLocation(locationId: string, destinationLocationId: string | null): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};
  try {
    cleanup['deactivate'] = await runGraphqlAllowGraphqlErrors(locationDeactivateMutation, {
      locationId,
      destinationLocationId,
    });
  } catch (error) {
    cleanup['deactivateError'] = error instanceof Error ? error.message : String(error);
  }
  try {
    cleanup['delete'] = await runGraphqlAllowGraphqlErrors(locationDeleteMutation, { locationId });
  } catch (error) {
    cleanup['deleteError'] = error instanceof Error ? error.message : String(error);
  }
  return cleanup;
}

async function createTrackedProduct(runId: string, role: string): Promise<ProductSetup> {
  const create = (await runGraphql(createProductMutation, {
    product: {
      title: `Inventory default carrier ${role} ${runId}`,
      status: 'ACTIVE',
    },
  })) as GraphqlPayload;
  expectNoUserErrors(create, ['data', 'productCreate', 'userErrors'], 'productCreate');
  const product = readCreatedProduct(create);
  const track = (await runGraphql(trackInventoryMutation, {
    productId: product.productId,
    variants: [
      {
        id: product.variantId,
        inventoryItem: {
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  })) as GraphqlPayload;
  expectNoUserErrors(track, ['data', 'productVariantsBulkUpdate', 'userErrors'], 'productVariantsBulkUpdate');
  const beforeInventoryRead = (await runGraphql(inventoryItemReadQuery, {
    item: product.inventoryItemId,
  })) as GraphqlPayload;
  return {
    ...product,
    create,
    track,
    beforeInventoryRead,
  };
}

async function createLocation(
  runId: string,
  role: 'origin' | 'destination',
): Promise<{
  payload: GraphqlPayload;
  location: { id: string; name: string };
}> {
  const payload = await runGraphqlAllowGraphqlErrors(locationAddMutation, {
    input: {
      name: `Inventory default carrier ${role} ${runId}`,
      fulfillsOnlineOrders: true,
      address: {
        address1: role === 'origin' ? '10 Origin Test St' : '20 Destination Test St',
        city: 'Boston',
        provinceCode: 'MA',
        countryCode: 'US',
        zip: role === 'origin' ? '02110' : '02111',
      },
    },
  });
  expectNoUserErrors(payload, ['data', 'locationAdd', 'userErrors'], 'locationAdd');
  return {
    payload,
    location: readCreatedLocation(payload),
  };
}

async function createShipmentSetup(runId: string): Promise<ShipmentSetup> {
  const product = await createTrackedProduct(runId, 'shipment');
  const origin = await createLocation(runId, 'origin');
  const destination = await createLocation(runId, 'destination');

  const originActivation = await runGraphqlAllowGraphqlErrors(inventoryActivateMutation, {
    inventoryItemId: product.inventoryItemId,
    locationId: origin.location.id,
    available: 5,
  });
  expectNoUserErrors(originActivation, ['data', 'inventoryActivate', 'userErrors'], 'origin inventoryActivate');
  const destinationActivation = await runGraphqlAllowGraphqlErrors(inventoryActivateMutation, {
    inventoryItemId: product.inventoryItemId,
    locationId: destination.location.id,
    available: 0,
  });
  expectNoUserErrors(
    destinationActivation,
    ['data', 'inventoryActivate', 'userErrors'],
    'destination inventoryActivate',
  );

  const inventorySet = await runGraphqlAllowGraphqlErrors(inventorySetQuantitiesMutation, {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://inventory-default-carrier/${apiVersion}/${runId}`,
      ignoreCompareQuantity: true,
      quantities: [
        {
          inventoryItemId: product.inventoryItemId,
          locationId: origin.location.id,
          quantity: 5,
        },
        {
          inventoryItemId: product.inventoryItemId,
          locationId: destination.location.id,
          quantity: 0,
        },
      ],
    },
  });
  expectNoUserErrors(inventorySet, ['data', 'inventorySetQuantities', 'userErrors'], 'inventorySetQuantities');

  return {
    ...product,
    originLocation: origin.location,
    destinationLocation: destination.location,
    originLocationCreate: origin.payload,
    destinationLocationCreate: destination.payload,
    originActivation,
    destinationActivation,
    inventorySet,
  };
}

async function hydrateCall(
  query: string,
  ids: string[],
  operationName = 'ProductsHydrateNodes',
): Promise<UpstreamCall> {
  const response = await runGraphqlRequest(query, { ids });
  if (response.status < 200 || response.status >= 300) {
    throw new Error(`Hydration call failed: ${JSON.stringify(response, null, 2)}`);
  }
  return {
    operationName,
    variables: { ids },
    query,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

function firstInventoryLocationId(setup: ProductSetup): string | null {
  const nodes = readArrayPath(
    setup.beforeInventoryRead,
    ['data', 'inventoryItem', 'inventoryLevels', 'nodes'],
    'inventoryItem read',
  );
  for (const node of nodes) {
    const id = (node as JsonRecord | undefined)?.['location'];
    if (typeof id === 'object' && id !== null && typeof (id as JsonRecord)['id'] === 'string') {
      const locationId = (id as JsonRecord)['id'];
      if (typeof locationId === 'string') return locationId;
    }
  }
  return null;
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
let defaultProductIdForCleanup: string | null = null;
let shipmentProductIdForCleanup: string | null = null;
let orderIdForCleanup: string | null = null;
let coldVariantOrderIdForCleanup: string | null = null;
let unresolvedVariantOrderIdForCleanup: string | null = null;
let shipmentIdForCleanup: string | null = null;
let inTransitShipmentIdForCleanup: string | null = null;
let transferIdForCleanup: string | null = null;
let inTransitTransferIdForCleanup: string | null = null;
let shipmentLocationIdsForCleanup: string[] = [];
let cleanupDestinationLocationId: string | null = null;

const defaultProduct = await createTrackedProduct(runId, 'default');
defaultProductIdForCleanup = defaultProduct.productId;
cleanupDestinationLocationId = firstInventoryLocationId(defaultProduct);
const shipmentProduct = await createShipmentSetup(runId);
shipmentProductIdForCleanup = shipmentProduct.productId;
shipmentLocationIdsForCleanup = [shipmentProduct.originLocation.id, shipmentProduct.destinationLocation.id];

try {
  const upstreamCalls: UpstreamCall[] = [];
  const inventoryItemUpdateVariables = {
    id: defaultProduct.inventoryItemId,
    input: {
      tracked: true,
      requiresShipping: true,
    },
  };
  upstreamCalls.push(
    await hydrateCall(parityRunnerInventoryHydrateNodesQuery, [defaultProduct.inventoryItemId]),
    await hydrateCall(productHydrateNodesQuery, [defaultProduct.inventoryItemId]),
  );
  const inventoryItemUpdate = await runGraphqlAllowGraphqlErrors(
    inventoryItemUpdateMutation,
    inventoryItemUpdateVariables,
  );
  expectNoUserErrors(inventoryItemUpdate, ['data', 'inventoryItemUpdate', 'userErrors'], 'inventoryItemUpdate');

  const inventoryItemsReadVariables = {
    query: `id:${resourceTail(defaultProduct.inventoryItemId)}`,
  };
  const inventoryItemsRead = await runGraphqlAllowGraphqlErrors(inventoryItemsReadQuery, inventoryItemsReadVariables);

  const orderCreateVariables = {
    order: {
      email: `inventory-default-location-${runId}@example.com`,
      test: true,
      currency: 'USD',
      lineItems: [
        {
          variantId: defaultProduct.variantId,
          quantity: 2,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'USD',
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
  const orderCreateDefaultLocation = await runGraphqlAllowGraphqlErrors(orderCreateMutation, orderCreateVariables);
  expectNoUserErrors(orderCreateDefaultLocation, ['data', 'orderCreate', 'userErrors'], 'orderCreate');
  orderIdForCleanup = readStringPath(orderCreateDefaultLocation, ['data', 'orderCreate', 'order', 'id'], 'orderCreate');

  const afterOrderInventoryReadVariables = {
    item: defaultProduct.inventoryItemId,
  };
  const afterOrderInventoryRead = await runGraphqlAllowGraphqlErrors(
    inventoryItemReadQuery,
    afterOrderInventoryReadVariables,
  );

  upstreamCalls.push(
    await hydrateCall(
      orderCreateInventoryPreflightQuery,
      [shipmentProduct.variantId],
      'OrdersOrderCreateInventoryPreflight',
    ),
  );
  const coldVariantOrderCreateVariables = {
    order: {
      email: `inventory-cold-variant-${runId}@example.com`,
      test: true,
      currency: 'USD',
      lineItems: [
        {
          variantId: shipmentProduct.variantId,
          quantity: 2,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'USD',
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
  const coldVariantOrderCreate = await runGraphqlAllowGraphqlErrors(
    orderCreateMutation,
    coldVariantOrderCreateVariables,
  );
  expectNoUserErrors(coldVariantOrderCreate, ['data', 'orderCreate', 'userErrors'], 'cold variant orderCreate');
  coldVariantOrderIdForCleanup = readStringPath(
    coldVariantOrderCreate,
    ['data', 'orderCreate', 'order', 'id'],
    'cold variant orderCreate',
  );

  const afterColdVariantOrderInventoryReadVariables = {
    item: shipmentProduct.inventoryItemId,
  };
  const afterColdVariantOrderInventoryRead = await runGraphqlAllowGraphqlErrors(
    inventoryItemReadQuery,
    afterColdVariantOrderInventoryReadVariables,
  );
  const afterColdVariantOrderProductReadVariables = {
    id: shipmentProduct.productId,
  };
  const afterColdVariantOrderProductRead = await runGraphqlAllowGraphqlErrors(
    coldVariantProductReadQuery,
    afterColdVariantOrderProductReadVariables,
  );
  const coldVariantOrderReadVariables = { id: coldVariantOrderIdForCleanup };
  const coldVariantOrderRead = await runGraphqlAllowGraphqlErrors(orderReadQuery, coldVariantOrderReadVariables);

  const unresolvedVariantId = `gid://shopify/ProductVariant/${resourceTail(shipmentProduct.inventoryItemId)}`;
  upstreamCalls.push(
    await hydrateCall(orderCreateInventoryPreflightQuery, [unresolvedVariantId], 'OrdersOrderCreateInventoryPreflight'),
  );
  const unresolvedVariantOrderCreateVariables = {
    order: {
      email: `inventory-unresolved-variant-${runId}@example.com`,
      test: true,
      currency: 'USD',
      lineItems: [
        {
          variantId: unresolvedVariantId,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'USD',
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
  const unresolvedVariantOrderCreate = await runGraphqlAllowGraphqlErrors(
    orderCreateMutation,
    unresolvedVariantOrderCreateVariables,
  );
  unresolvedVariantOrderIdForCleanup = readOptionalStringPath(unresolvedVariantOrderCreate, [
    'data',
    'orderCreate',
    'order',
    'id',
  ]);
  const afterUnresolvedVariantOrderInventoryReadVariables = {
    item: shipmentProduct.inventoryItemId,
  };
  const afterUnresolvedVariantOrderInventoryRead = await runGraphqlAllowGraphqlErrors(
    inventoryItemReadQuery,
    afterUnresolvedVariantOrderInventoryReadVariables,
  );

  const transferCreateVariables = {
    input: {
      originLocationId: shipmentProduct.originLocation.id,
      destinationLocationId: shipmentProduct.destinationLocation.id,
      referenceName: `inventory-default-carrier-${runId}`,
      note: 'inventory default carrier conformance',
      tags: ['inventory-default-carrier'],
      lineItems: [
        {
          inventoryItemId: shipmentProduct.inventoryItemId,
          quantity: 2,
        },
      ],
    },
  };
  upstreamCalls.push(
    await hydrateCall(parityRunnerInventoryHydrateNodesQuery, [shipmentProduct.inventoryItemId]),
    await hydrateCall(productHydrateNodesQuery, [
      shipmentProduct.originLocation.id,
      shipmentProduct.destinationLocation.id,
      shipmentProduct.inventoryItemId,
    ]),
  );
  const transferCreate = await runGraphqlAllowGraphqlErrors(inventoryTransferCreateMutation, transferCreateVariables);
  expectNoUserErrors(transferCreate, ['data', 'inventoryTransferCreate', 'userErrors'], 'inventoryTransferCreate');
  transferIdForCleanup = readStringPath(
    transferCreate,
    ['data', 'inventoryTransferCreate', 'inventoryTransfer', 'id'],
    'inventoryTransferCreate',
  );

  const transferReadyVariables = {
    id: transferIdForCleanup,
  };
  const transferReady = await runGraphqlAllowGraphqlErrors(inventoryTransferMarkReadyMutation, transferReadyVariables);
  expectNoUserErrors(
    transferReady,
    ['data', 'inventoryTransferMarkAsReadyToShip', 'userErrors'],
    'inventoryTransferMarkAsReadyToShip',
  );
  upstreamCalls.push(await hydrateCall(parityRunnerInventoryHydrateNodesQuery, [shipmentProduct.inventoryItemId]));

  const shipmentCreateVariables = {
    input: {
      movementId: transferIdForCleanup,
      trackingInput: {
        trackingNumber: `JP-${runId}`,
        company: 'Japan Post',
        trackingUrl: 'https://www.post.japanpost.jp/',
        arrivesAt: '2026-07-01T12:00:00Z',
      },
      lineItems: [
        {
          inventoryItemId: shipmentProduct.inventoryItemId,
          quantity: 2,
        },
      ],
    },
  };
  const shipmentCreate = await runGraphqlAllowGraphqlErrors(inventoryShipmentCreateMutation, shipmentCreateVariables);
  expectNoUserErrors(shipmentCreate, ['data', 'inventoryShipmentCreate', 'userErrors'], 'inventoryShipmentCreate');
  shipmentIdForCleanup = readStringPath(
    shipmentCreate,
    ['data', 'inventoryShipmentCreate', 'inventoryShipment', 'id'],
    'inventoryShipmentCreate',
  );

  const transferReadAfterDraftShipmentVariables = { id: transferIdForCleanup };
  const transferReadAfterDraftShipment = await runGraphqlAllowGraphqlErrors(
    inventoryTransferReadAfterShipmentQuery,
    transferReadAfterDraftShipmentVariables,
  );

  const inTransitTransferCreateVariables = {
    input: {
      originLocationId: shipmentProduct.originLocation.id,
      destinationLocationId: shipmentProduct.destinationLocation.id,
      referenceName: `inventory-default-carrier-in-transit-${runId}`,
      note: 'inventory default carrier in-transit conformance',
      tags: ['inventory-default-carrier', 'in-transit'],
      lineItems: [
        {
          inventoryItemId: shipmentProduct.inventoryItemId,
          quantity: 1,
        },
      ],
    },
  };
  upstreamCalls.push(
    await hydrateCall(parityRunnerInventoryHydrateNodesQuery, [shipmentProduct.inventoryItemId]),
    await hydrateCall(productHydrateNodesQuery, [
      shipmentProduct.originLocation.id,
      shipmentProduct.destinationLocation.id,
      shipmentProduct.inventoryItemId,
    ]),
  );
  const inTransitTransferCreate = await runGraphqlAllowGraphqlErrors(
    inventoryTransferCreateMutation,
    inTransitTransferCreateVariables,
  );
  expectNoUserErrors(
    inTransitTransferCreate,
    ['data', 'inventoryTransferCreate', 'userErrors'],
    'in-transit inventoryTransferCreate',
  );
  inTransitTransferIdForCleanup = readStringPath(
    inTransitTransferCreate,
    ['data', 'inventoryTransferCreate', 'inventoryTransfer', 'id'],
    'in-transit inventoryTransferCreate',
  );

  const inTransitTransferReadyVariables = {
    id: inTransitTransferIdForCleanup,
  };
  const inTransitTransferReady = await runGraphqlAllowGraphqlErrors(
    inventoryTransferMarkReadyMutation,
    inTransitTransferReadyVariables,
  );
  expectNoUserErrors(
    inTransitTransferReady,
    ['data', 'inventoryTransferMarkAsReadyToShip', 'userErrors'],
    'in-transit inventoryTransferMarkAsReadyToShip',
  );
  upstreamCalls.push(await hydrateCall(parityRunnerInventoryHydrateNodesQuery, [shipmentProduct.inventoryItemId]));

  const shipmentCreateInTransitVariables = {
    input: {
      movementId: inTransitTransferIdForCleanup,
      trackingInput: {
        trackingNumber: `JP-IN-TRANSIT-${runId}`,
        company: 'Japan Post',
        trackingUrl: 'https://www.post.japanpost.jp/',
        arrivesAt: '2026-07-01T12:00:00Z',
      },
      lineItems: [
        {
          inventoryItemId: shipmentProduct.inventoryItemId,
          quantity: 1,
        },
      ],
    },
  };
  const shipmentCreateInTransit = await runGraphqlAllowGraphqlErrors(
    inventoryShipmentCreateInTransitMutation,
    shipmentCreateInTransitVariables,
  );
  expectNoUserErrors(
    shipmentCreateInTransit,
    ['data', 'inventoryShipmentCreateInTransit', 'userErrors'],
    'inventoryShipmentCreateInTransit',
  );
  inTransitShipmentIdForCleanup = readStringPath(
    shipmentCreateInTransit,
    ['data', 'inventoryShipmentCreateInTransit', 'inventoryShipment', 'id'],
    'inventoryShipmentCreateInTransit',
  );

  const transferReadAfterShipmentVariables = { id: inTransitTransferIdForCleanup };
  const transferReadAfterShipment = await runGraphqlAllowGraphqlErrors(
    inventoryTransferReadAfterShipmentQuery,
    transferReadAfterShipmentVariables,
  );

  const cleanup: JsonRecord = {};
  cleanup['orderCancel'] = await cancelOrder(orderIdForCleanup);
  orderIdForCleanup = null;
  cleanup['coldVariantOrderCancel'] = await cancelOrder(coldVariantOrderIdForCleanup);
  coldVariantOrderIdForCleanup = null;
  cleanup['unresolvedVariantOrderCancel'] = await cancelOrder(unresolvedVariantOrderIdForCleanup);
  unresolvedVariantOrderIdForCleanup = null;
  cleanup['inTransitShipmentDelete'] = await runGraphqlAllowGraphqlErrors(inventoryShipmentDeleteMutation, {
    id: inTransitShipmentIdForCleanup,
  });
  inTransitShipmentIdForCleanup = null;
  cleanup['shipmentDelete'] = await runGraphqlAllowGraphqlErrors(inventoryShipmentDeleteMutation, {
    id: shipmentIdForCleanup,
  });
  shipmentIdForCleanup = null;
  cleanup['inTransitTransferCancel'] = await runGraphqlAllowGraphqlErrors(inventoryTransferCancelMutation, {
    id: inTransitTransferIdForCleanup,
  });
  inTransitTransferIdForCleanup = null;
  cleanup['transferCancel'] = await runGraphqlAllowGraphqlErrors(inventoryTransferCancelMutation, {
    id: transferIdForCleanup,
  });
  transferIdForCleanup = null;
  cleanup['defaultProductDelete'] = await deleteProduct(defaultProduct.productId);
  defaultProductIdForCleanup = null;
  cleanup['shipmentProductDelete'] = await deleteProduct(shipmentProduct.productId);
  shipmentProductIdForCleanup = null;
  cleanup['shipmentLocations'] = {};
  for (const locationId of [...shipmentLocationIdsForCleanup].reverse()) {
    (cleanup['shipmentLocations'] as JsonRecord)[locationId] = await cleanupLocation(
      locationId,
      cleanupDestinationLocationId,
    );
  }
  shipmentLocationIdsForCleanup = [];

  const fixture = {
    scenarioId: 'inventory-default-location-carrier-connection',
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    setup: {
      defaultProduct,
      shipmentProduct,
      defaultLocationId: cleanupDestinationLocationId,
    },
    workflow: {
      inventoryItemUpdate: {
        variables: inventoryItemUpdateVariables,
        response: inventoryItemUpdate,
      },
      inventoryItemsRead: {
        variables: inventoryItemsReadVariables,
        response: inventoryItemsRead,
      },
      orderCreateDefaultLocation: {
        variables: orderCreateVariables,
        response: orderCreateDefaultLocation,
      },
      afterOrderInventoryRead: {
        variables: afterOrderInventoryReadVariables,
        response: afterOrderInventoryRead,
      },
      coldVariantOrderCreate: {
        variables: coldVariantOrderCreateVariables,
        response: coldVariantOrderCreate,
      },
      afterColdVariantOrderInventoryRead: {
        variables: afterColdVariantOrderInventoryReadVariables,
        response: afterColdVariantOrderInventoryRead,
      },
      afterColdVariantOrderProductRead: {
        variables: afterColdVariantOrderProductReadVariables,
        response: afterColdVariantOrderProductRead,
      },
      coldVariantOrderRead: {
        variables: coldVariantOrderReadVariables,
        response: coldVariantOrderRead,
      },
      unresolvedVariantOrderCreate: {
        variables: unresolvedVariantOrderCreateVariables,
        response: unresolvedVariantOrderCreate,
      },
      afterUnresolvedVariantOrderInventoryRead: {
        variables: afterUnresolvedVariantOrderInventoryReadVariables,
        response: afterUnresolvedVariantOrderInventoryRead,
      },
      transferCreate: {
        variables: transferCreateVariables,
        response: transferCreate,
      },
      transferReady: {
        variables: transferReadyVariables,
        response: transferReady,
      },
      shipmentCreate: {
        variables: shipmentCreateVariables,
        response: shipmentCreate,
      },
      transferReadAfterDraftShipment: {
        variables: transferReadAfterDraftShipmentVariables,
        response: transferReadAfterDraftShipment,
      },
      inTransitTransferCreate: {
        variables: inTransitTransferCreateVariables,
        response: inTransitTransferCreate,
      },
      inTransitTransferReady: {
        variables: inTransitTransferReadyVariables,
        response: inTransitTransferReady,
      },
      shipmentCreateInTransit: {
        variables: shipmentCreateInTransitVariables,
        response: shipmentCreateInTransit,
      },
      transferReadAfterShipment: {
        variables: transferReadAfterShipmentVariables,
        response: transferReadAfterShipment,
      },
    },
    cleanup,
    upstreamCalls,
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        storeDomain,
        apiVersion,
        output: outputPath,
      },
      null,
      2,
    ),
  );
} finally {
  if (inTransitShipmentIdForCleanup) {
    try {
      await runGraphqlAllowGraphqlErrors(inventoryShipmentDeleteMutation, { id: inTransitShipmentIdForCleanup });
    } catch (error) {
      console.warn(
        `In-transit shipment cleanup failed for ${inTransitShipmentIdForCleanup}: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    }
  }
  if (shipmentIdForCleanup) {
    try {
      await runGraphqlAllowGraphqlErrors(inventoryShipmentDeleteMutation, { id: shipmentIdForCleanup });
    } catch (error) {
      console.warn(
        `Shipment cleanup failed for ${shipmentIdForCleanup}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
  if (inTransitTransferIdForCleanup) {
    try {
      await runGraphqlAllowGraphqlErrors(inventoryTransferCancelMutation, { id: inTransitTransferIdForCleanup });
    } catch (error) {
      console.warn(
        `In-transit transfer cleanup failed for ${inTransitTransferIdForCleanup}: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    }
  }
  if (transferIdForCleanup) {
    try {
      await runGraphqlAllowGraphqlErrors(inventoryTransferCancelMutation, { id: transferIdForCleanup });
    } catch (error) {
      console.warn(
        `Transfer cleanup failed for ${transferIdForCleanup}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
  await cancelOrder(unresolvedVariantOrderIdForCleanup);
  await cancelOrder(coldVariantOrderIdForCleanup);
  await cancelOrder(orderIdForCleanup);
  await deleteProduct(defaultProductIdForCleanup);
  await deleteProduct(shipmentProductIdForCleanup);
  for (const locationId of [...shipmentLocationIdsForCleanup].reverse()) {
    await cleanupLocation(locationId, cleanupDestinationLocationId);
  }
}
