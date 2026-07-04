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
  response: ConformanceGraphqlResult<JsonRecord>;
};

type CreatedOrder = {
  id: string;
  name: string | null;
  fulfillmentOrderId: string;
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
const outputPath = path.join(outputDir, 'fulfillment-order-set-deadline-closed-not-found.json');

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderDeadlineFields on FulfillmentOrder {
    id
    status
    requestStatus
    fulfillBy
    updatedAt
    order {
      id
      name
      displayFulfillmentStatus
    }
    lineItems(first: 5) {
      nodes {
        id
        totalQuantity
        remainingQuantity
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation CreateFulfillmentOrderDeadlineOrder($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 5) {
          nodes {
            ...FulfillmentOrderDeadlineFields
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

const fulfillmentOrderCancelMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation CloseFulfillmentOrderForDeadline($id: ID!) {
    fulfillmentOrderCancel(id: $id) {
      fulfillmentOrder {
        ...FulfillmentOrderDeadlineFields
      }
      replacementFulfillmentOrder {
        ...FulfillmentOrderDeadlineFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const setFulfillmentDeadlineMutation = `#graphql
  mutation FulfillmentOrdersSetFulfillmentDeadlineLifecycle(
    $fulfillmentOrderIds: [ID!]!
    $fulfillmentDeadline: DateTime!
  ) {
    fulfillmentOrdersSetFulfillmentDeadline(
      fulfillmentOrderIds: $fulfillmentOrderIds
      fulfillmentDeadline: $fulfillmentDeadline
    ) {
      success
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const orderReadQuery = `#graphql
  ${fulfillmentOrderFields}
  query FulfillmentOrderDeadlineOrderRead($id: ID!) {
    order(id: $id) {
      id
      name
      displayFulfillmentStatus
      fulfillmentOrders(first: 10) {
        nodes {
          ...FulfillmentOrderDeadlineFields
        }
      }
    }
  }
`;

const taggedOrderSearchQuery = `#graphql
  query FulfillmentOrderDeadlineTaggedOrders($query: String!, $first: Int!) {
    orders(first: $first, query: $query, reverse: true) {
      nodes {
        id
        name
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation CleanupFulfillmentOrderDeadlineOrder(
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

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readArray(value: unknown, key: string): unknown[] {
  const fieldValue = asRecord(value)?.[key];
  return Array.isArray(fieldValue) ? fieldValue : [];
}

function readNodes(connection: JsonRecord | null): JsonRecord[] {
  return readArray(connection, 'nodes').flatMap((node) => {
    const record = asRecord(node);
    return record ? [record] : [];
  });
}

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' ? fieldValue : null;
}

function readBoolean(value: unknown, key: string): boolean | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'boolean' ? fieldValue : null;
}

function responseData(capture: GraphqlCapture): JsonRecord {
  return asRecord(capture.response.payload.data) ?? {};
}

function userErrors(capture: GraphqlCapture, rootName: string): unknown[] {
  return readArray(readRecord(responseData(capture), rootName), 'userErrors');
}

function assertNoGraphqlErrors(capture: GraphqlCapture, label: string): void {
  if (capture.response.status < 200 || capture.response.status >= 300 || capture.response.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(capture.response.payload, null, 2)}`);
  }
}

function assertNoUserErrors(capture: GraphqlCapture, rootName: string, label: string): void {
  assertNoGraphqlErrors(capture, label);
  const errors = userErrors(capture, rootName);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function firstFulfillmentOrder(order: JsonRecord | null): JsonRecord | null {
  return asRecord(readArray(readRecord(order, 'fulfillmentOrders'), 'nodes')[0]);
}

function createdOrderFrom(capture: GraphqlCapture): CreatedOrder {
  const order = readRecord(readRecord(responseData(capture), 'orderCreate'), 'order');
  const fulfillmentOrder = firstFulfillmentOrder(order);
  const id = readString(order, 'id');
  const fulfillmentOrderId = readString(fulfillmentOrder, 'id');
  if (!id || !fulfillmentOrderId) {
    throw new Error(
      `Order setup did not return an order with a fulfillment order: ${JSON.stringify(capture.response.payload)}`,
    );
  }
  return {
    id,
    name: readString(order, 'name'),
    fulfillmentOrderId,
  };
}

function cancelledFulfillmentOrderFrom(capture: GraphqlCapture): JsonRecord {
  const fulfillmentOrder = readRecord(readRecord(responseData(capture), 'fulfillmentOrderCancel'), 'fulfillmentOrder');
  const id = readString(fulfillmentOrder, 'id');
  const status = readString(fulfillmentOrder, 'status');
  if (!fulfillmentOrder || !id || !status) {
    throw new Error(
      `fulfillmentOrderCancel did not return a target fulfillment order: ${JSON.stringify(capture.response.payload)}`,
    );
  }
  if (status !== 'CLOSED' && status !== 'CANCELLED') {
    throw new Error(`Expected fulfillmentOrderCancel to leave a closed/cancelled fulfillment order, got ${status}`);
  }
  return fulfillmentOrder;
}

function assertSetDeadlineSuccess(capture: GraphqlCapture, label: string): void {
  assertNoGraphqlErrors(capture, label);
  const root = readRecord(responseData(capture), 'fulfillmentOrdersSetFulfillmentDeadline');
  if (
    readBoolean(root, 'success') !== true ||
    userErrors(capture, 'fulfillmentOrdersSetFulfillmentDeadline').length !== 0
  ) {
    throw new Error(
      `${label} did not return success with empty userErrors: ${JSON.stringify(capture.response.payload)}`,
    );
  }
}

function assertUnknownNotFound(capture: GraphqlCapture): void {
  assertNoGraphqlErrors(capture, 'unknown fulfillment order deadline');
  const root = readRecord(responseData(capture), 'fulfillmentOrdersSetFulfillmentDeadline');
  const expected = [
    {
      field: null,
      message: 'Fulfillment orders could not be found.',
      code: null,
    },
  ];
  const actual = userErrors(capture, 'fulfillmentOrdersSetFulfillmentDeadline');
  if (readBoolean(root, 'success') !== false || JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`Unknown id branch did not match Core payload: ${JSON.stringify(capture.response.payload)}`);
  }
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

async function capture(query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  const document = trimGraphql(query);
  return {
    query: document,
    variables,
    response: await runGraphqlRequest<JsonRecord>(document, variables),
  };
}

async function cleanupOrder(orderId: string): Promise<GraphqlCapture> {
  return await capture(orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
}

async function cleanupTaggedOrders(): Promise<{ search: GraphqlCapture; cancels: Record<string, GraphqlCapture> }> {
  const search = await capture(taggedOrderSearchQuery, {
    query: 'tag:fulfillment-deadline-parity',
    first: 10,
  });
  const cancels: Record<string, GraphqlCapture> = {};
  for (const order of readNodes(readRecord(responseData(search), 'orders'))) {
    const orderId = readString(order, 'id');
    if (orderId) {
      cancels[orderId] = await cleanupOrder(orderId);
    }
  }
  return { search, cancels };
}

async function main(): Promise<void> {
  const startedAt = new Date().toISOString();
  const stamp = startedAt.replace(/[-:.TZ]/gu, '').slice(0, 14);
  const fulfillmentDeadline = '2026-12-01T00:00:00Z';
  const unknownFulfillmentOrderId = 'gid://shopify/FulfillmentOrder/999999999999999';
  const createdOrders: CreatedOrder[] = [];
  const cleanup: Record<string, GraphqlCapture> = {};
  const preCleanup = await cleanupTaggedOrders();

  let createClosedOrder: GraphqlCapture | null = null;
  let closeFulfillmentOrder: GraphqlCapture | null = null;
  let setClosedDeadline: GraphqlCapture | null = null;
  let afterSetClosedDeadline: GraphqlCapture | null = null;
  let setUnknownDeadline: GraphqlCapture | null = null;
  let closedFulfillmentOrderId: string | null = null;
  let closedFulfillmentOrderStatus: string | null = null;

  try {
    createClosedOrder = await capture(orderCreateMutation, {
      order: {
        email: `fulfillment-deadline-${stamp}@example.com`,
        note: `fulfillment-order deadline parity ${stamp}`,
        tags: ['shopify-draft-proxy', 'fulfillment-deadline-parity'],
        test: true,
        currency: 'USD',
        shippingAddress: {
          firstName: 'Deadline',
          lastName: 'Parity',
          address1: '123 Queen St W',
          city: 'Toronto',
          provinceCode: 'ON',
          countryCode: 'CA',
          zip: 'M5H 2M9',
        },
        lineItems: [
          {
            variantId: 'gid://shopify/ProductVariant/48540157378793',
            title: `Fulfillment deadline item ${stamp}`,
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
      },
      options: {
        inventoryBehaviour: 'BYPASS',
        sendReceipt: false,
        sendFulfillmentReceipt: false,
      },
    });
    assertNoUserErrors(createClosedOrder, 'orderCreate', 'create disposable order');
    const closedOrder = createdOrderFrom(createClosedOrder);
    createdOrders.push(closedOrder);

    closeFulfillmentOrder = await capture(fulfillmentOrderCancelMutation, {
      id: closedOrder.fulfillmentOrderId,
    });
    assertNoUserErrors(closeFulfillmentOrder, 'fulfillmentOrderCancel', 'close fulfillment order setup');
    const closedFulfillmentOrder = cancelledFulfillmentOrderFrom(closeFulfillmentOrder);
    closedFulfillmentOrderId = readString(closedFulfillmentOrder, 'id');
    closedFulfillmentOrderStatus = readString(closedFulfillmentOrder, 'status');
    if (!closedFulfillmentOrderId) {
      throw new Error('Closed fulfillment order id was unexpectedly empty');
    }

    setClosedDeadline = await capture(setFulfillmentDeadlineMutation, {
      fulfillmentOrderIds: [closedFulfillmentOrderId],
      fulfillmentDeadline,
    });
    assertSetDeadlineSuccess(setClosedDeadline, 'closed fulfillment order deadline');

    afterSetClosedDeadline = await capture(orderReadQuery, { id: closedOrder.id });

    setUnknownDeadline = await capture(setFulfillmentDeadlineMutation, {
      fulfillmentOrderIds: [unknownFulfillmentOrderId],
      fulfillmentDeadline,
    });
    assertUnknownNotFound(setUnknownDeadline);
  } finally {
    for (const order of [...createdOrders].reverse()) {
      cleanup[order.id] = await cleanupOrder(order.id);
    }
  }

  if (
    !createClosedOrder ||
    !closeFulfillmentOrder ||
    !setClosedDeadline ||
    !afterSetClosedDeadline ||
    !setUnknownDeadline ||
    !closedFulfillmentOrderId
  ) {
    throw new Error('Capture did not complete all required branches');
  }

  await writeJson(outputPath, {
    metadata: {
      capturedAt: new Date().toISOString(),
      startedAt,
      storeDomain,
      apiVersion,
      scopedRoots: ['fulfillmentOrdersSetFulfillmentDeadline'],
      setup: [
        'Creates a disposable order with one fulfillment order.',
        'Uses fulfillmentOrderCancel to leave an existing fulfillment order in a closed/cancelled state before setting its fulfillBy deadline.',
        'Uses a never-created fulfillment order GID for the all-ids-unresolvable branch.',
      ],
      createdOrders,
      closedFulfillmentOrder: {
        id: closedFulfillmentOrderId,
        status: closedFulfillmentOrderStatus,
      },
      unknownFulfillmentOrderId,
      fulfillmentDeadline,
    },
    workflows: {
      closedFulfillmentOrderDeadline: {
        create: createClosedOrder,
        closeFulfillmentOrder,
        setFulfillmentDeadline: setClosedDeadline,
        afterSetFulfillmentDeadline: afterSetClosedDeadline,
      },
      unknownIdDeadline: {
        setFulfillmentDeadline: setUnknownDeadline,
      },
    },
    preCleanup,
    cleanup,
    upstreamCalls: [],
  });

  console.log(`Wrote ${outputPath}`);
}

await main();
