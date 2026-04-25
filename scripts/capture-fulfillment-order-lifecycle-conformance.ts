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

type CreatedOrder = {
  id: string;
  name: string | null;
  fulfillmentOrderId: string;
  fulfillmentOrderLineItemId: string;
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

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputPath = path.join(outputDir, 'fulfillment-order-lifecycle.json');

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderLifecycleFields on FulfillmentOrder {
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

const orderCreateMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation CreateFulfillmentOrderLifecycleOrder($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 5) {
          nodes {
            ...FulfillmentOrderLifecycleFields
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
  mutation CleanupFulfillmentOrderLifecycleOrder(
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

const orderReadQuery = `#graphql
  ${fulfillmentOrderFields}
  query FulfillmentOrderLifecycleOrderRead($id: ID!) {
    order(id: $id) {
      id
      name
      displayFulfillmentStatus
      fulfillmentOrders(first: 10) {
        nodes {
          ...FulfillmentOrderLifecycleFields
        }
      }
    }
  }
`;

const topLevelReadQuery = `#graphql
  ${fulfillmentOrderFields}
  query FulfillmentOrderLifecycleTopLevelRead($fulfillmentOrderId: ID!, $first: Int!) {
    fulfillmentOrder(id: $fulfillmentOrderId) {
      ...FulfillmentOrderLifecycleFields
    }
    fulfillmentOrders(first: $first, includeClosed: true, sortKey: ID) {
      nodes {
        ...FulfillmentOrderLifecycleFields
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    manualHoldsFulfillmentOrders(first: $first) {
      nodes {
        ...FulfillmentOrderLifecycleFields
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

const locationsQuery = `#graphql
  query FulfillmentOrderLifecycleLocations($first: Int!) {
    locations(first: $first) {
      nodes {
        id
        name
        fulfillsOnlineOrders
        isFulfillmentService
      }
    }
  }
`;

const holdMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderHoldLifecycle($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
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
        ...FulfillmentOrderLifecycleFields
      }
      remainingFulfillmentOrder {
        ...FulfillmentOrderLifecycleFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const releaseHoldMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderReleaseHoldLifecycle($id: ID!, $holdIds: [ID!], $externalId: String) {
    fulfillmentOrderReleaseHold(id: $id, holdIds: $holdIds, externalId: $externalId) {
      fulfillmentOrder {
        ...FulfillmentOrderLifecycleFields
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
  mutation FulfillmentOrderMoveLifecycle(
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
        ...FulfillmentOrderLifecycleFields
      }
      originalFulfillmentOrder {
        ...FulfillmentOrderLifecycleFields
      }
      remainingFulfillmentOrder {
        ...FulfillmentOrderLifecycleFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const rescheduleMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderRescheduleLifecycle($id: ID!, $fulfillAt: DateTime!) {
    fulfillmentOrderReschedule(id: $id, fulfillAt: $fulfillAt) {
      fulfillmentOrder {
        ...FulfillmentOrderLifecycleFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const reportProgressMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderReportProgressLifecycle(
    $id: ID!
    $progressReport: FulfillmentOrderReportProgressInput
  ) {
    fulfillmentOrderReportProgress(id: $id, progressReport: $progressReport) {
      fulfillmentOrder {
        ...FulfillmentOrderLifecycleFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const openMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderOpenLifecycle($id: ID!) {
    fulfillmentOrderOpen(id: $id) {
      fulfillmentOrder {
        ...FulfillmentOrderLifecycleFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const closeMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderCloseLifecycle($id: ID!, $message: String) {
    fulfillmentOrderClose(id: $id, message: $message) {
      fulfillmentOrder {
        ...FulfillmentOrderLifecycleFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const cancelMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderCancelLifecycle($id: ID!) {
    fulfillmentOrderCancel(id: $id) {
      fulfillmentOrder {
        ...FulfillmentOrderLifecycleFields
      }
      replacementFulfillmentOrder {
        ...FulfillmentOrderLifecycleFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const rerouteMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrdersRerouteLifecycle(
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
        ...FulfillmentOrderLifecycleFields
      }
      userErrors {
        field
        message
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

function getFirstFulfillmentOrder(order: JsonRecord): JsonRecord | null {
  const fulfillmentOrders = readObject(order['fulfillmentOrders']);
  return readNodes(fulfillmentOrders)[0] ?? null;
}

function getFirstFulfillmentOrderLineItem(fulfillmentOrder: JsonRecord): JsonRecord | null {
  return readNodes(readObject(fulfillmentOrder['lineItems']))[0] ?? null;
}

function getAssignedLocationId(fulfillmentOrder: JsonRecord): string | null {
  const assignedLocation = readObject(fulfillmentOrder['assignedLocation']);
  const location = readObject(assignedLocation?.['location']);
  const locationId = location?.['id'];
  return typeof locationId === 'string' ? locationId : null;
}

function asCreatedOrder(captureResult: GraphqlCapture): CreatedOrder {
  const data = readObject(captureResult.response.payload.data);
  const orderCreate = readObject(data?.['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  if (!order || typeof order['id'] !== 'string') {
    throw new Error(
      `Unable to create disposable fulfillment-order lifecycle order: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }

  const fulfillmentOrder = getFirstFulfillmentOrder(order);
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(captureResult.response.payload)}`);
  }

  const lineItem = getFirstFulfillmentOrderLineItem(fulfillmentOrder);
  if (!lineItem || typeof lineItem['id'] !== 'string') {
    throw new Error(`Created fulfillment order has no line item: ${JSON.stringify(captureResult.response.payload)}`);
  }

  return {
    id: order['id'],
    name: typeof order['name'] === 'string' ? order['name'] : null,
    fulfillmentOrderId: fulfillmentOrder['id'],
    fulfillmentOrderLineItemId: lineItem['id'],
    assignedLocationId: getAssignedLocationId(fulfillmentOrder),
  };
}

async function createLifecycleOrder(
  label: string,
  quantity = 2,
): Promise<{ order: CreatedOrder; capture: GraphqlCapture }> {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const variables = {
    order: {
      email: `har-234-${label}-${stamp}@example.com`,
      note: `HAR-234 fulfillment-order lifecycle ${label} ${stamp}`,
      tags: ['har-234', 'fulfillment-order-lifecycle', label],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'HAR',
        lastName: 'Probe',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          title: `HAR-234 fulfillment item ${label} ${stamp}`,
          quantity,
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
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
  const createCapture = await capture(orderCreateMutation, variables);
  return {
    order: asCreatedOrder(createCapture),
    capture: createCapture,
  };
}

async function cleanupOrder(order: CreatedOrder): Promise<GraphqlCapture> {
  return capture(orderCancelMutation, {
    orderId: order.id,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
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

function readLocationIds(locationsCapture: GraphqlCapture): string[] {
  const data = readObject(locationsCapture.response.payload.data);
  const locations = readObject(data?.['locations']);
  return readNodes(locations)
    .map((location) => location['id'])
    .filter((id): id is string => typeof id === 'string');
}

function findAlternateLocationId(locationIds: string[], currentLocationId: string | null): string | null {
  return locationIds.find((locationId) => locationId !== currentLocationId) ?? null;
}

function readHoldId(holdCapture: GraphqlCapture): string | null {
  const data = readObject(holdCapture.response.payload.data);
  const payload = readObject(data?.['fulfillmentOrderHold']);
  const hold = readObject(payload?.['fulfillmentHold']);
  const holdId = hold?.['id'];
  return typeof holdId === 'string' ? holdId : null;
}

const startedAt = new Date().toISOString();
const createdOrders: CreatedOrder[] = [];
const cleanup: GraphqlCapture[] = [];
const locations = await capture(locationsQuery, { first: 10 });
const locationIds = readLocationIds(locations);

async function createTrackedOrder(
  label: string,
  quantity = 2,
): Promise<{ order: CreatedOrder; create: GraphqlCapture }> {
  const { order, capture: create } = await createLifecycleOrder(label, quantity);
  createdOrders.push(order);
  return { order, create };
}

const invalidId = 'gid://shopify/FulfillmentOrder/0';
const validation = {
  holdUnknownId: await capture(holdMutation, {
    id: invalidId,
    fulfillmentHold: {
      reason: 'OTHER',
      reasonNotes: 'HAR-234 unknown-id hold validation',
      notifyMerchant: false,
      externalId: 'har-234-unknown-hold',
      handle: 'har-234-unknown-hold',
    },
  }),
  releaseHoldUnknownId: await capture(releaseHoldMutation, {
    id: invalidId,
    holdIds: ['gid://shopify/FulfillmentHold/0'],
    externalId: 'har-234-unknown-release',
  }),
  moveUnknownId: await capture(moveMutation, {
    id: invalidId,
    newLocationId: locationIds[0] ?? 'gid://shopify/Location/0',
    fulfillmentOrderLineItems: null,
  }),
  openUnknownId: await capture(openMutation, { id: invalidId }),
  closeUnknownId: await capture(closeMutation, {
    id: invalidId,
    message: 'HAR-234 unknown-id close validation',
  }),
  cancelUnknownId: await capture(cancelMutation, { id: invalidId }),
  rescheduleUnknownId: await capture(rescheduleMutation, {
    id: invalidId,
    fulfillAt: new Date(Date.now() + 48 * 60 * 60 * 1000).toISOString(),
  }),
  reportProgressUnknownId: await capture(reportProgressMutation, {
    id: invalidId,
    progressReport: {
      reasonNotes: 'HAR-234 unknown-id progress validation',
    },
  }),
  rerouteUnknownId: await capture(rerouteMutation, {
    fulfillmentOrderIds: [invalidId],
    includedLocationIds: locationIds.slice(0, 1),
    excludedLocationIds: null,
  }),
};

const holdReleaseOrder = await createTrackedOrder('hold-release', 2);
const hold = await capture(holdMutation, {
  id: holdReleaseOrder.order.fulfillmentOrderId,
  fulfillmentHold: {
    reason: 'OTHER',
    reasonNotes: 'HAR-234 hold/release lifecycle capture',
    notifyMerchant: false,
    externalId: 'har-234-hold-release',
    handle: 'har-234-hold-release',
    fulfillmentOrderLineItems: [
      {
        id: holdReleaseOrder.order.fulfillmentOrderLineItemId,
        quantity: 1,
      },
    ],
  },
});
const afterHold = await readAfter(holdReleaseOrder.order);
const releaseHold = await capture(releaseHoldMutation, {
  id: holdReleaseOrder.order.fulfillmentOrderId,
  holdIds: readHoldId(hold) ? [readHoldId(hold)] : null,
  externalId: 'har-234-hold-release',
});
const afterReleaseHold = await readAfter(holdReleaseOrder.order);

const moveOrder = await createTrackedOrder('move', 2);
const moveLocationId = findAlternateLocationId(locationIds, moveOrder.order.assignedLocationId);
const move = await capture(moveMutation, {
  id: moveOrder.order.fulfillmentOrderId,
  newLocationId: moveLocationId ?? 'gid://shopify/Location/0',
  fulfillmentOrderLineItems: [
    {
      id: moveOrder.order.fulfillmentOrderLineItemId,
      quantity: 1,
    },
  ],
});
const afterMove = await readAfter(moveOrder.order);

const scheduleProgressOrder = await createTrackedOrder('schedule-progress-open-close', 1);
const fulfillAt = new Date(Date.now() + 72 * 60 * 60 * 1000).toISOString();
const reschedule = await capture(rescheduleMutation, {
  id: scheduleProgressOrder.order.fulfillmentOrderId,
  fulfillAt,
});
const afterReschedule = await readAfter(scheduleProgressOrder.order);
const reportProgress = await capture(reportProgressMutation, {
  id: scheduleProgressOrder.order.fulfillmentOrderId,
  progressReport: {
    reasonNotes: 'HAR-234 report progress lifecycle capture',
  },
});
const afterReportProgress = await readAfter(scheduleProgressOrder.order);
const open = await capture(openMutation, { id: scheduleProgressOrder.order.fulfillmentOrderId });
const afterOpen = await readAfter(scheduleProgressOrder.order);
const close = await capture(closeMutation, {
  id: scheduleProgressOrder.order.fulfillmentOrderId,
  message: 'HAR-234 close lifecycle capture',
});
const afterClose = await readAfter(scheduleProgressOrder.order);

const rerouteOrder = await createTrackedOrder('reroute-cancel', 1);
const rerouteLocationId = findAlternateLocationId(locationIds, rerouteOrder.order.assignedLocationId);
const reroute = await capture(rerouteMutation, {
  fulfillmentOrderIds: [rerouteOrder.order.fulfillmentOrderId],
  includedLocationIds: rerouteLocationId ? [rerouteLocationId] : locationIds.slice(0, 1),
  excludedLocationIds: null,
});
const afterReroute = await readAfter(rerouteOrder.order);
const cancel = await capture(cancelMutation, { id: rerouteOrder.order.fulfillmentOrderId });
const afterCancel = await readAfter(rerouteOrder.order);

for (const order of createdOrders) {
  cleanup.push(await cleanupOrder(order));
}

const output = {
  metadata: {
    issue: 'HAR-234',
    capturedAt: new Date().toISOString(),
    startedAt,
    storeDomain,
    apiVersion,
    scopedRoots: [
      'fulfillmentOrderHold',
      'fulfillmentOrderReleaseHold',
      'fulfillmentOrderMove',
      'fulfillmentOrderOpen',
      'fulfillmentOrderClose',
      'fulfillmentOrderCancel',
      'fulfillmentOrderReschedule',
      'fulfillmentOrderReportProgress',
      'fulfillmentOrdersReroute',
    ],
    locationIds,
    createdOrders,
  },
  locations,
  validation,
  workflows: {
    holdRelease: {
      create: holdReleaseOrder.create,
      hold,
      afterHold,
      releaseHold,
      afterReleaseHold,
    },
    move: {
      create: moveOrder.create,
      attemptedNewLocationId: moveLocationId,
      move,
      afterMove,
    },
    scheduleProgressOpenClose: {
      create: scheduleProgressOrder.create,
      fulfillAt,
      reschedule,
      afterReschedule,
      reportProgress,
      afterReportProgress,
      open,
      afterOpen,
      close,
      afterClose,
    },
    reroute: {
      create: rerouteOrder.create,
      attemptedIncludedLocationIds: rerouteLocationId ? [rerouteLocationId] : locationIds.slice(0, 1),
      reroute,
      afterReroute,
      cancelAfterReroute: cancel,
      afterCancel,
    },
  },
  cleanup,
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

console.log(`Captured fulfillment-order lifecycle fixture: ${outputPath}`);
