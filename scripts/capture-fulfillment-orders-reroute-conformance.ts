/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

type LocationSummary = {
  id: string;
  name: string | null;
};

type ProductSetup = {
  productId: string;
  variantId: string;
  inventoryItemId: string;
  originLocation: LocationSummary;
  destinationLocation: LocationSummary;
};

type CreatedOrder = {
  id: string;
  name: string | null;
  fulfillmentOrder: JsonRecord;
  fulfillmentOrderId: string;
  assignedLocationId: string | null;
};

const scenarioId = 'fulfillment-orders-reroute-local-staging';
const unknownFulfillmentOrderId = 'gid://shopify/FulfillmentOrder/999999999999999';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const startedAt = new Date().toISOString();
const runId = startedAt.replace(/[-:.TZ]/gu, '').slice(0, 14);
const createdOrders: CreatedOrder[] = [];
const cleanup: Record<string, unknown> = {};
let productId: string | null = null;

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrdersRerouteFields on FulfillmentOrder {
    id
    status
    requestStatus
    fulfillAt
    fulfillBy
    updatedAt
    supportedActions {
      action
    }
    assignedLocation {
      name
      location {
        id
        name
      }
    }
    lineItems(first: 5) {
      nodes {
        id
        totalQuantity
        remainingQuantity
        lineItem {
          id
          title
          quantity
          fulfillableQuantity
        }
      }
    }
    order {
      id
      name
      displayFulfillmentStatus
    }
  }
`;

const locationsQuery = `#graphql
  query FulfillmentOrdersRerouteLocations($first: Int!) {
    locations(first: $first) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
        fulfillsOnlineOrders
        shipsInventory
      }
    }
    locationsAvailableForDeliveryProfilesConnection(first: $first) {
      nodes {
        id
        name
      }
    }
  }
`;

const productCreateMutation = `#graphql
  mutation FulfillmentOrdersRerouteProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        variants(first: 1) {
          nodes {
            id
            title
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

const productVariantsBulkUpdateMutation = `#graphql
  mutation FulfillmentOrdersRerouteVariantTrack(
    $productId: ID!
    $variants: [ProductVariantsBulkInput!]!
  ) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      product {
        id
        tracksInventory
        totalInventory
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

const inventoryActivateMutation = `#graphql
  mutation FulfillmentOrdersRerouteInventoryActivate(
    $inventoryItemId: ID!
    $locationId: ID!
    $available: Int
    $idempotencyKey: String!
  ) {
    inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: $available) @idempotent(key: $idempotencyKey) {
      inventoryLevel {
        id
        location {
          id
          name
        }
        item {
          id
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
  mutation FulfillmentOrdersRerouteInventorySet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
    inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
      inventoryAdjustmentGroup {
        id
        reason
        referenceDocumentUri
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
        code
      }
    }
  }
`;

const inventoryItemReadQuery = `#graphql
  query FulfillmentOrdersRerouteInventoryItem($id: ID!) {
    inventoryItem(id: $id) {
      id
      inventoryLevels(first: 50) {
        nodes {
          id
          location {
            id
            name
          }
          quantities(names: ["available"]) {
            name
            quantity
          }
        }
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrdersRerouteOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 5) {
          nodes {
            ...FulfillmentOrdersRerouteFields
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

const orderCancelMutation = `#graphql
  mutation FulfillmentOrdersRerouteOrderCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation FulfillmentOrdersRerouteProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const rerouteMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrdersReroute(
    $fulfillmentOrderIds: [ID!]!
    $includedLocationIds: [ID!]
    $excludedLocationIds: [ID!]
  ) {
    fulfillmentOrdersReroute(
      fulfillmentOrderIds: $fulfillmentOrderIds
      includedLocationIds: $includedLocationIds
      excludedLocationIds: $excludedLocationIds
    ) {
      movedFulfillmentOrders {
        ...FulfillmentOrdersRerouteFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const reportProgressMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrdersRerouteReportProgress(
    $id: ID!
    $progressReport: FulfillmentOrderReportProgressInput
  ) {
    fulfillmentOrderReportProgress(id: $id, progressReport: $progressReport) {
      fulfillmentOrder {
        ...FulfillmentOrdersRerouteFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderReadQuery = `#graphql
  ${fulfillmentOrderFields}
  query FulfillmentOrdersRerouteOrderRead($id: ID!) {
    order(id: $id) {
      id
      name
      displayFulfillmentStatus
      fulfillmentOrders(first: 10) {
        nodes {
          ...FulfillmentOrdersRerouteFields
        }
      }
    }
  }
`;

const topLevelReadQuery = `#graphql
  ${fulfillmentOrderFields}
  query FulfillmentOrdersRerouteTopLevelRead($fulfillmentOrderId: ID!, $first: Int!) {
    fulfillmentOrder(id: $fulfillmentOrderId) {
      ...FulfillmentOrdersRerouteFields
    }
    fulfillmentOrders(first: $first, includeClosed: true, sortKey: ID) {
      nodes {
        ...FulfillmentOrdersRerouteFields
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null ? (value as JsonRecord) : null;
}

function readNodes(value: unknown): JsonRecord[] {
  const record = readObject(value);
  const nodes = record?.['nodes'];
  return Array.isArray(nodes) ? nodes.filter((node): node is JsonRecord => readObject(node) !== null) : [];
}

function data(captureResult: GraphqlCapture): JsonRecord {
  return readObject(captureResult.response.payload.data) ?? {};
}

function userErrors(captureResult: GraphqlCapture, root: string): unknown[] {
  const payload = readObject(data(captureResult)[root]);
  const errors = payload?.['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoTopLevelErrors(captureResult: GraphqlCapture, label: string): void {
  if (captureResult.response.status < 200 || captureResult.response.status >= 300 || captureResult.response.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function assertNoUserErrors(captureResult: GraphqlCapture, root: string, label: string): void {
  assertNoTopLevelErrors(captureResult, label);
  const errors = userErrors(captureResult, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function locationSummaries(locations: GraphqlCapture): LocationSummary[] {
  const availableConnection = readObject(data(locations)['locationsAvailableForDeliveryProfilesConnection']);
  const availableLocations = readNodes(availableConnection)
    .map((location) => ({
      id: typeof location['id'] === 'string' ? location['id'] : '',
      name: typeof location['name'] === 'string' ? location['name'] : null,
    }))
    .filter((location) => location.id !== '');
  if (availableLocations.length > 0) {
    return availableLocations;
  }

  const locationConnection = readObject(data(locations)['locations']);
  return readNodes(locationConnection)
    .filter((location) => location['isActive'] !== false)
    .filter((location) => location['isFulfillmentService'] !== true)
    .map((location) => ({
      id: typeof location['id'] === 'string' ? location['id'] : '',
      name: typeof location['name'] === 'string' ? location['name'] : null,
    }))
    .filter((location) => location.id !== '');
}

function firstNodeFromPath(captureResult: GraphqlCapture, pathParts: string[]): JsonRecord | null {
  let current: unknown = data(captureResult);
  for (const part of pathParts) {
    current = readObject(current)?.[part];
  }
  return readNodes(current)[0] ?? null;
}

function createdProduct(captureResult: GraphqlCapture): { productId: string; variantId: string; inventoryItemId: string } {
  const productCreate = readObject(data(captureResult)['productCreate']);
  const product = readObject(productCreate?.['product']);
  const variant = firstNodeFromPath(captureResult, ['productCreate', 'product', 'variants']);
  const inventoryItem = readObject(variant?.['inventoryItem']);
  const productId = product?.['id'];
  const variantId = variant?.['id'];
  const inventoryItemId = inventoryItem?.['id'];
  if (typeof productId !== 'string' || typeof variantId !== 'string' || typeof inventoryItemId !== 'string') {
    throw new Error(`Unable to read created product ids: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return { productId, variantId, inventoryItemId };
}

function assignedLocationId(fulfillmentOrder: JsonRecord): string | null {
  const assignedLocation = readObject(fulfillmentOrder['assignedLocation']);
  const location = readObject(assignedLocation?.['location']);
  const id = location?.['id'];
  return typeof id === 'string' ? id : null;
}

function firstFulfillmentOrderFromOrderCreate(captureResult: GraphqlCapture): JsonRecord {
  const orderCreate = readObject(data(captureResult)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  const fulfillmentOrders = readObject(order?.['fulfillmentOrders']);
  const fulfillmentOrder = readNodes(fulfillmentOrders)[0];
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return fulfillmentOrder;
}

function createdOrderFromCapture(captureResult: GraphqlCapture): CreatedOrder {
  const orderCreate = readObject(data(captureResult)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  if (!order || typeof order['id'] !== 'string') {
    throw new Error(`Unable to read created order: ${JSON.stringify(captureResult.response.payload)}`);
  }
  const fulfillmentOrder = firstFulfillmentOrderFromOrderCreate(captureResult);
  return {
    id: order['id'],
    name: typeof order['name'] === 'string' ? order['name'] : null,
    fulfillmentOrder,
    fulfillmentOrderId: fulfillmentOrder['id'] as string,
    assignedLocationId: assignedLocationId(fulfillmentOrder),
  };
}

function alternateLocation(locationIds: LocationSummary[], currentLocationId: string | null): LocationSummary {
  const alternate = locationIds.find((location) => location.id !== currentLocationId);
  if (!alternate) {
    throw new Error(`Need at least two active stock locations for ${scenarioId}: ${JSON.stringify(locationIds)}`);
  }
  return alternate;
}

function movedFulfillmentOrder(captureResult: GraphqlCapture, label: string): JsonRecord {
  assertNoUserErrors(captureResult, 'fulfillmentOrdersReroute', label);
  const payload = readObject(data(captureResult)['fulfillmentOrdersReroute']);
  const moved = readNodes({ nodes: payload?.['movedFulfillmentOrders'] })[0];
  if (!moved) {
    throw new Error(`${label} returned no movedFulfillmentOrders: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return moved;
}

function expectMovedAway(captureResult: GraphqlCapture, originalLocationId: string | null, label: string): string {
  const moved = movedFulfillmentOrder(captureResult, label);
  const movedLocationId = assignedLocationId(moved);
  if (!movedLocationId || movedLocationId === originalLocationId) {
    throw new Error(`${label} did not change location: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return movedLocationId;
}

function availableQuantity(inventoryRead: GraphqlCapture, locationId: string): number {
  const inventoryItem = readObject(data(inventoryRead)['inventoryItem']);
  const levels = readObject(inventoryItem?.['inventoryLevels']);
  const level = readNodes(levels).find((node) => {
    const location = readObject(node['location']);
    return location?.['id'] === locationId;
  });
  const quantities = Array.isArray(level?.['quantities']) ? level['quantities'] : [];
  const available = quantities.find((quantity) => {
    const record = readObject(quantity);
    return record?.['name'] === 'available';
  });
  const quantity = readObject(available)?.['quantity'];
  return typeof quantity === 'number' ? quantity : 0;
}

async function createProductSetup(locations: LocationSummary[], runId: string): Promise<{
  setup: ProductSetup;
  create: GraphqlCapture;
  track: GraphqlCapture;
  originActivation: GraphqlCapture;
  destinationActivation: GraphqlCapture;
  inventoryReadBeforeSet: GraphqlCapture;
  inventorySet: GraphqlCapture;
}> {
  const originLocation = locations[0];
  const destinationLocation = locations[1];
  if (!originLocation || !destinationLocation) {
    throw new Error(`Need at least two active stock locations for ${scenarioId}: ${JSON.stringify(locations)}`);
  }

  const create = await capture(productCreateMutation, {
    product: {
      title: `Fulfillment orders reroute conformance ${runId}`,
      status: 'ACTIVE',
    },
  });
  assertNoUserErrors(create, 'productCreate', 'productCreate');
  const product = createdProduct(create);
  productId = product.productId;

  const track = await capture(productVariantsBulkUpdateMutation, {
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
  });
  assertNoUserErrors(track, 'productVariantsBulkUpdate', 'productVariantsBulkUpdate');

  const originActivation = await capture(inventoryActivateMutation, {
    inventoryItemId: product.inventoryItemId,
    locationId: originLocation.id,
    available: null,
    idempotencyKey: `fulfillment-orders-reroute-origin-${runId}`,
  });
  assertNoUserErrors(originActivation, 'inventoryActivate', 'origin inventoryActivate');

  const destinationActivation = await capture(inventoryActivateMutation, {
    inventoryItemId: product.inventoryItemId,
    locationId: destinationLocation.id,
    available: null,
    idempotencyKey: `fulfillment-orders-reroute-destination-${runId}`,
  });
  assertNoUserErrors(destinationActivation, 'inventoryActivate', 'destination inventoryActivate');

  const inventoryReadBeforeSet = await capture(inventoryItemReadQuery, {
    id: product.inventoryItemId,
  });
  assertNoTopLevelErrors(inventoryReadBeforeSet, 'inventory item before set');

  const inventorySet = await capture(inventorySetQuantitiesMutation, {
    input: {
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: `logistics://fulfillment-orders-reroute-conformance/${apiVersion}/${runId}`,
      quantities: [
        {
          inventoryItemId: product.inventoryItemId,
          locationId: originLocation.id,
          quantity: 5,
          changeFromQuantity: availableQuantity(inventoryReadBeforeSet, originLocation.id),
        },
        {
          inventoryItemId: product.inventoryItemId,
          locationId: destinationLocation.id,
          quantity: 5,
          changeFromQuantity: availableQuantity(inventoryReadBeforeSet, destinationLocation.id),
        },
      ],
    },
    idempotencyKey: `fulfillment-orders-reroute-set-${runId}`,
  });
  assertNoUserErrors(inventorySet, 'inventorySetQuantities', 'inventorySetQuantities');

  return {
    setup: {
      ...product,
      originLocation,
      destinationLocation,
    },
    create,
    track,
    originActivation,
    destinationActivation,
    inventoryReadBeforeSet,
    inventorySet,
  };
}

async function createRerouteOrder(
  setup: ProductSetup,
  label: string,
  runId: string,
): Promise<{ order: CreatedOrder; create: GraphqlCapture }> {
  const variables = {
    order: {
      email: `fulfillment-orders-reroute-${label}-${runId}@example.com`,
      note: `Fulfillment orders reroute conformance ${label} ${runId}`,
      tags: ['fulfillment-orders-reroute-conformance', label],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'Reroute',
        lastName: 'Conformance',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: setup.variantId,
          title: `Fulfillment orders reroute ${label}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '20.00',
              currencyCode: 'USD',
            },
          },
          requiresShipping: true,
          taxable: true,
        },
      ],
      shippingLines: [
        {
          title: 'Reroute Ground',
          code: 'REROUTE_GROUND',
          source: 'fulfillment-orders-reroute-conformance',
          priceSet: {
            shopMoney: {
              amount: '5.00',
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
  const create = await capture(orderCreateMutation, variables);
  assertNoUserErrors(create, 'orderCreate', `orderCreate ${label}`);
  return {
    order: createdOrderFromCapture(create),
    create,
  };
}

async function readAfter(order: CreatedOrder): Promise<{ orderRead: GraphqlCapture; topLevelRead: GraphqlCapture }> {
  return {
    orderRead: await capture(orderReadQuery, { id: order.id }),
    topLevelRead: await capture(topLevelReadQuery, {
      fulfillmentOrderId: order.fulfillmentOrderId,
      first: 10,
    }),
  };
}

async function cleanupOrder(order: CreatedOrder): Promise<GraphqlCapture> {
  return capture(orderCancelMutation, {
    orderId: order.id,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });
}

async function cleanupProduct(productId: string): Promise<GraphqlCapture> {
  return capture(productDeleteMutation, { input: { id: productId } });
}

try {
  const locations = await capture(locationsQuery, { first: 20 });
  assertNoTopLevelErrors(locations, 'locations');
  const stockLocations = locationSummaries(locations);
  if (stockLocations.length < 2) {
    throw new Error(`Need at least two active stock locations for ${scenarioId}: ${JSON.stringify(stockLocations)}`);
  }

  const productSetup = await createProductSetup(stockLocations, runId);
  productId = productSetup.setup.productId;

  const successOrder = await createRerouteOrder(productSetup.setup, 'success', runId);
  createdOrders.push(successOrder.order);
  const successReroute = await capture(rerouteMutation, {
    fulfillmentOrderIds: [successOrder.order.fulfillmentOrderId],
    includedLocationIds: null,
    excludedLocationIds: null,
  });
  const successDestinationLocationId = expectMovedAway(
    successReroute,
    successOrder.order.assignedLocationId,
    'success fulfillmentOrdersReroute',
  );
  const afterSuccessReroute = await readAfter(successOrder.order);

  const progressOrder = await createRerouteOrder(productSetup.setup, 'progress', runId);
  createdOrders.push(progressOrder.order);
  const progressDestination = alternateLocation(stockLocations, progressOrder.order.assignedLocationId);
  const progressReport = await capture(reportProgressMutation, {
    id: progressOrder.order.fulfillmentOrderId,
    progressReport: {
      reasonNotes: 'Report progress before reroute validation',
    },
  });
  assertNoUserErrors(progressReport, 'fulfillmentOrderReportProgress', 'fulfillmentOrderReportProgress');
  const rerouteAfterProgress = await capture(rerouteMutation, {
    fulfillmentOrderIds: [progressOrder.order.fulfillmentOrderId],
    includedLocationIds: [progressDestination.id],
    excludedLocationIds: null,
  });
  assertNoTopLevelErrors(rerouteAfterProgress, 'reroute after progress');

  const emptyIds = await capture(rerouteMutation, {
    fulfillmentOrderIds: [],
    includedLocationIds: null,
    excludedLocationIds: null,
  });
  assertNoTopLevelErrors(emptyIds, 'empty fulfillmentOrderIds');

  const unknownHighId = await capture(rerouteMutation, {
    fulfillmentOrderIds: [unknownFulfillmentOrderId],
    includedLocationIds: null,
    excludedLocationIds: null,
  });
  assertNoTopLevelErrors(unknownHighId, 'unknown high fulfillmentOrderId');

  for (const order of createdOrders) {
    cleanup[`cancelOrder:${order.id}`] = await cleanupOrder(order);
  }
  if (productId) {
    cleanup['productDelete'] = await cleanupProduct(productId);
  }

  const output = {
    metadata: {
      scenarioId,
      capturedAt: new Date().toISOString(),
      startedAt,
      storeDomain,
      apiVersion,
      stockLocations,
      unknownFulfillmentOrderId,
      scopedRoots: ['fulfillmentOrdersReroute', 'fulfillmentOrderReportProgress'],
      createdOrders,
    },
    setup: {
      locations,
      productCreate: productSetup.create,
      productVariantsBulkUpdate: productSetup.track,
      originInventoryActivate: productSetup.originActivation,
      destinationInventoryActivate: productSetup.destinationActivation,
      inventoryReadBeforeSet: productSetup.inventoryReadBeforeSet,
      inventorySetQuantities: productSetup.inventorySet,
    },
    success: {
      create: successOrder.create,
      expectedDestinationLocationId: successDestinationLocationId,
      reroute: successReroute,
      afterReroute: afterSuccessReroute,
    },
    validation: {
      progress: {
        create: progressOrder.create,
        expectedDestinationLocationId: progressDestination.id,
        reportProgress: progressReport,
        rerouteAfterProgress,
      },
      emptyIds,
      unknownHighId,
    },
    cleanup,
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  console.log(`Captured fulfillment-orders reroute fixture: ${outputPath}`);
} catch (error) {
  console.error((error as Error).message);
  for (const order of createdOrders) {
    try {
      cleanup[`cancelOrderAfterError:${order.id}`] = await cleanupOrder(order);
    } catch (cleanupError) {
      console.error(`Cleanup order ${order.id} failed: ${(cleanupError as Error).message}`);
    }
  }
  if (productId) {
    try {
      cleanup['productDeleteAfterError'] = await cleanupProduct(productId);
    } catch (cleanupError) {
      console.error(`Cleanup product ${productId} failed: ${(cleanupError as Error).message}`);
    }
  }
  process.exitCode = 1;
}
