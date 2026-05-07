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

type RecordedUpstreamCall = {
  operationName: string;
  variables: JsonRecord;
  query: string;
  response: {
    status: number;
    body: JsonRecord;
  };
};

type CreatedOrder = {
  id: string;
  name: string | null;
  fulfillmentOrderId: string;
  fulfillmentOrderLineItemIds: string[];
  assignedLocationId: string | null;
};

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
const moveOutputPath = path.join(outputDir, 'fulfillment-order-move-multi-line.json');
const holdOutputPath = path.join(outputDir, 'fulfillment-order-hold-multi-line.json');
const physicalVariantA = 'gid://shopify/ProductVariant/48540157378793';

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderMoveHoldMultiLineFields on FulfillmentOrder {
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
    fulfillmentHolds {
      id
      handle
      reason
      reasonNotes
      displayReason
      heldByApp {
        id
        title
      }
      heldByRequestingApp
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
  }
`;

const locationsQuery = `#graphql
  query FulfillmentOrderMoveHoldMultiLineLocations($first: Int!) {
    locationsAvailableForDeliveryProfilesConnection(first: $first) {
      nodes {
        id
        name
        localPickupSettingsV2 {
          pickupTime
          instructions
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation CreateFulfillmentOrderMoveHoldMultiLineOrder($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 5) {
          nodes {
            ...FulfillmentOrderMoveHoldMultiLineFields
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

const moveMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderMoveMultiLine(
    $id: ID!
    $newLocationId: ID!
    $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]
  ) {
    fulfillmentOrderMove(
      id: $id
      newLocationId: $newLocationId
      fulfillmentOrderLineItems: $fulfillmentOrderLineItems
    ) {
      movedFulfillmentOrder {
        ...FulfillmentOrderMoveHoldMultiLineFields
      }
      originalFulfillmentOrder {
        ...FulfillmentOrderMoveHoldMultiLineFields
      }
      remainingFulfillmentOrder {
        ...FulfillmentOrderMoveHoldMultiLineFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const holdMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderHoldMultiLine($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
    fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
      fulfillmentHold {
        id
        handle
        reason
        reasonNotes
        displayReason
        heldByRequestingApp
      }
      fulfillmentOrder {
        ...FulfillmentOrderMoveHoldMultiLineFields
      }
      remainingFulfillmentOrder {
        ...FulfillmentOrderMoveHoldMultiLineFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const releaseHoldMutation = `#graphql
  mutation CleanupFulfillmentOrderHoldMultiLine($id: ID!, $holdIds: [ID!]) {
    fulfillmentOrderReleaseHold(id: $id, holdIds: $holdIds) {
      fulfillmentOrder {
        id
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const hydrateFulfillmentOrderQuery = `#graphql
  query ShippingFulfillmentOrderHydrate($id: ID!) {
    fulfillmentOrder(id: $id) {
      id status requestStatus assignmentStatus fulfillAt fulfillBy updatedAt
      supportedActions { action }
      assignedLocation { name location { id name } }
      fulfillmentHolds { id handle reason reasonNotes displayReason heldByApp { id title } heldByRequestingApp }
      merchantRequests(first: 10) { nodes { kind message requestOptions } }
      lineItems(first: 20) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
      order { id name displayFulfillmentStatus }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation CleanupFulfillmentOrderMoveHoldMultiLineOrder(
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

const taggedOrdersQuery = `#graphql
  query CleanupFulfillmentOrderMoveHoldMultiLineTaggedOrders($query: String!) {
    orders(first: 20, query: $query, sortKey: CREATED_AT, reverse: true) {
      nodes {
        id
        name
        cancelledAt
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
    response: scrubTicketIdentifiers(await runGraphqlRequest(query, variables)) as ConformanceGraphqlResult,
  };
}

function scrubTicketIdentifiers(value: unknown): unknown {
  if (typeof value === 'string') {
    return value.replace(/\bHAR-\d+\b/gu, 'historical-setup');
  }
  if (Array.isArray(value)) {
    return value.map((entry) => scrubTicketIdentifiers(entry));
  }
  const record = readObject(value);
  if (record) {
    return Object.fromEntries(Object.entries(record).map(([key, entry]) => [key, scrubTicketIdentifiers(entry)]));
  }
  return value;
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null ? (value as JsonRecord) : null;
}

function data(captureResult: GraphqlCapture): JsonRecord {
  return readObject(captureResult.response.payload.data) ?? {};
}

function readNodes(value: unknown): JsonRecord[] {
  const record = readObject(value);
  const nodes = record?.['nodes'];
  return Array.isArray(nodes) ? nodes.filter((node): node is JsonRecord => readObject(node) !== null) : [];
}

function getFulfillmentOrder(order: JsonRecord): JsonRecord {
  const fulfillmentOrder = readNodes(readObject(order['fulfillmentOrders']))[0];
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(order)}`);
  }
  return fulfillmentOrder;
}

function asCreatedOrder(captureResult: GraphqlCapture): CreatedOrder {
  const orderCreate = readObject(data(captureResult)['orderCreate']);
  const userErrors = orderCreate?.['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`Unable to create disposable order: ${JSON.stringify(userErrors)}`);
  }
  const order = readObject(orderCreate?.['order']);
  if (!order || typeof order['id'] !== 'string') {
    throw new Error(`Unable to create disposable order: ${JSON.stringify(captureResult.response.payload)}`);
  }

  const fulfillmentOrder = getFulfillmentOrder(order);
  const lineItemIds = readNodes(readObject(fulfillmentOrder['lineItems']))
    .map((lineItem) => lineItem['id'])
    .filter((id): id is string => typeof id === 'string');
  if (lineItemIds.length < 2) {
    throw new Error(`Created fulfillment order does not have two line items: ${JSON.stringify(order)}`);
  }

  const assignedLocation = readObject(fulfillmentOrder['assignedLocation']);
  const location = readObject(assignedLocation?.['location']);
  const assignedLocationId = typeof location?.['id'] === 'string' ? location['id'] : null;

  return {
    id: order['id'],
    name: typeof order['name'] === 'string' ? order['name'] : null,
    fulfillmentOrderId: fulfillmentOrder['id'] as string,
    fulfillmentOrderLineItemIds: lineItemIds,
    assignedLocationId,
  };
}

function hydrationCallFromOrderCreate(captureResult: GraphqlCapture): RecordedUpstreamCall {
  const orderCreate = readObject(data(captureResult)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  if (!order) {
    throw new Error(
      `Unable to build fulfillment order hydration cassette: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }
  const fulfillmentOrder = getFulfillmentOrder(order);
  const fulfillmentOrderId = fulfillmentOrder['id'];
  if (typeof fulfillmentOrderId !== 'string') {
    throw new Error(`Hydration fulfillment order has no id: ${JSON.stringify(fulfillmentOrder)}`);
  }
  return {
    operationName: 'ShippingFulfillmentOrderHydrate',
    variables: { id: fulfillmentOrderId },
    query: trimGraphql(hydrateFulfillmentOrderQuery),
    response: {
      status: captureResult.response.status,
      body: {
        data: {
          fulfillmentOrder,
        },
      },
    },
  };
}

function upstreamCallFromCapture(operationName: string, captureResult: GraphqlCapture): RecordedUpstreamCall {
  return {
    operationName,
    variables: captureResult.variables,
    query: captureResult.query,
    response: {
      status: captureResult.response.status,
      body: captureResult.response.payload,
    },
  };
}

function readActiveLocationIds(captureResult: GraphqlCapture): string[] {
  return readNodes(readObject(data(captureResult)['locationsAvailableForDeliveryProfilesConnection']))
    .map((location) => location['id'])
    .filter((id): id is string => typeof id === 'string');
}

async function createTrackedOrder(
  label: string,
  lineItems: Array<{ variantId: string; quantity: number; title: string }>,
): Promise<{ order: CreatedOrder; capture: GraphqlCapture }> {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const captureResult = await capture(orderCreateMutation, {
    order: {
      email: `fo-${label}-${stamp}@example.com`,
      note: `fulfillment-order move hold multi-line ${label} ${stamp}`,
      tags: ['fulfillment-order-move-hold-multi-line', label],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'MoveHold',
        lastName: 'MultiLine',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: lineItems.map((lineItem) => ({
        variantId: lineItem.variantId,
        title: `${lineItem.title} ${stamp}`,
        quantity: lineItem.quantity,
        properties: [{ name: 'capture-line', value: lineItem.title }],
        priceSet: {
          shopMoney: {
            amount: '20.00',
            currencyCode: 'USD',
          },
        },
        requiresShipping: true,
        taxable: true,
      })),
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  });
  return { order: asCreatedOrder(captureResult), capture: captureResult };
}

async function cleanupOrder(order: CreatedOrder): Promise<GraphqlCapture> {
  return capture(orderCancelMutation, {
    orderId: order.id,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
}

async function cleanupPriorTaggedOrders(): Promise<GraphqlCapture[]> {
  const search = await capture(taggedOrdersQuery, {
    query: 'tag:fulfillment-order-move-hold-multi-line',
  });
  const orders = readNodes(readObject(data(search)['orders']));
  const cancels: GraphqlCapture[] = [];
  for (const order of orders) {
    if (typeof order['id'] === 'string' && order['cancelledAt'] === null) {
      cancels.push(
        await cleanupOrder({
          id: order['id'],
          name: typeof order['name'] === 'string' ? order['name'] : null,
          fulfillmentOrderId: '',
          fulfillmentOrderLineItemIds: [],
          assignedLocationId: null,
        }),
      );
    }
  }
  return [search, ...cancels];
}

function assertNoUserErrors(captureResult: GraphqlCapture, root: string, label: string): void {
  const payload = readObject(data(captureResult)[root]);
  const userErrors = payload?.['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function readHoldId(captureResult: GraphqlCapture): string {
  const payload = readObject(data(captureResult)['fulfillmentOrderHold']);
  const hold = readObject(payload?.['fulfillmentHold']);
  const id = hold?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`Hold response did not include a hold id: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return id;
}

const startedAt = new Date().toISOString();
const preflightCleanup = await cleanupPriorTaggedOrders();
const locations = await capture(locationsQuery, { first: 10 });
const activeLocationIds = readActiveLocationIds(locations);

const moveOrder = await createTrackedOrder('move', [
  { variantId: physicalVariantA, quantity: 2, title: 'FO move multi item 1' },
  { variantId: physicalVariantA, quantity: 3, title: 'FO move multi item 2' },
]);
const moveDestinationId = activeLocationIds.find((id) => id !== moveOrder.order.assignedLocationId);
if (!moveDestinationId) {
  throw new Error(
    `No alternate active merchant-managed location found for move capture: ${JSON.stringify(activeLocationIds)}`,
  );
}
const move = await capture(moveMutation, {
  id: moveOrder.order.fulfillmentOrderId,
  newLocationId: moveDestinationId,
  fulfillmentOrderLineItems: [
    { id: moveOrder.order.fulfillmentOrderLineItemIds[0], quantity: 2 },
    { id: moveOrder.order.fulfillmentOrderLineItemIds[1], quantity: 1 },
  ],
});
assertNoUserErrors(move, 'fulfillmentOrderMove', 'multi-line move');
const moveCleanup = await cleanupOrder(moveOrder.order);

const holdOrder = await createTrackedOrder('hold', [
  { variantId: physicalVariantA, quantity: 2, title: 'FO hold multi item 1' },
  { variantId: physicalVariantA, quantity: 3, title: 'FO hold multi item 2' },
]);
const hold = await capture(holdMutation, {
  id: holdOrder.order.fulfillmentOrderId,
  fulfillmentHold: {
    reason: 'OTHER',
    reasonNotes: 'fulfillment-order hold multi-line capture',
    handle: `multi-line-hold-${Date.now()}`,
    notifyMerchant: false,
    fulfillmentOrderLineItems: [
      { id: holdOrder.order.fulfillmentOrderLineItemIds[0], quantity: 2 },
      { id: holdOrder.order.fulfillmentOrderLineItemIds[1], quantity: 1 },
    ],
  },
});
assertNoUserErrors(hold, 'fulfillmentOrderHold', 'multi-line hold');
const releaseHold = await capture(releaseHoldMutation, {
  id: holdOrder.order.fulfillmentOrderId,
  holdIds: [readHoldId(hold)],
});
const holdCleanup = await cleanupOrder(holdOrder.order);

const moveOutput = {
  metadata: {
    capturedAt: new Date().toISOString(),
    startedAt,
    storeDomain,
    apiVersion,
    scopedRoots: ['fulfillmentOrderMove'],
    createdOrder: moveOrder.order,
    destinationLocationId: moveDestinationId,
  },
  setup: {
    preflightCleanup,
    locations,
    orderCreate: moveOrder.capture,
  },
  workflow: {
    move,
  },
  cleanup: {
    orderCancel: moveCleanup,
  },
  upstreamCalls: [
    upstreamCallFromCapture('FulfillmentOrderMoveHoldMultiLineLocations', locations),
    hydrationCallFromOrderCreate(moveOrder.capture),
  ],
};

const holdOutput = {
  metadata: {
    capturedAt: new Date().toISOString(),
    startedAt,
    storeDomain,
    apiVersion,
    scopedRoots: ['fulfillmentOrderHold'],
    createdOrder: holdOrder.order,
  },
  setup: {
    preflightCleanup,
    orderCreate: holdOrder.capture,
  },
  workflow: {
    hold,
  },
  cleanup: {
    releaseHold,
    orderCancel: holdCleanup,
  },
  upstreamCalls: [hydrationCallFromOrderCreate(holdOrder.capture)],
};

await mkdir(outputDir, { recursive: true });
await writeFile(moveOutputPath, `${JSON.stringify(moveOutput, null, 2)}\n`, 'utf8');
await writeFile(holdOutputPath, `${JSON.stringify(holdOutput, null, 2)}\n`, 'utf8');

console.log(`Captured fulfillment-order move multi-line fixture: ${moveOutputPath}`);
console.log(`Captured fulfillment-order hold multi-line fixture: ${holdOutputPath}`);
