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
};

type RecordedUpstreamCall = {
  operationName: string;
  query: string;
  variables: JsonRecord;
  response: {
    status: number;
    body: unknown;
  };
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
const outputPath = path.join(outputDir, 'fulfillment-order-open-report-progress-preconditions.json');

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderPreconditionFields on FulfillmentOrder {
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
      heldByRequestingApp
    }
    lineItems(first: 20) {
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

const orderCreateMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderPreconditionsOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 5) {
          nodes {
            ...FulfillmentOrderPreconditionFields
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

const holdMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderPreconditionsHold($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
    fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
      fulfillmentHold {
        id
        handle
        reason
        reasonNotes
      }
      fulfillmentOrder {
        ...FulfillmentOrderPreconditionFields
      }
      remainingFulfillmentOrder {
        ...FulfillmentOrderPreconditionFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const openMutation = `#graphql
  mutation FulfillmentOrderStatusPreconditionOpen($id: ID!) {
    fulfillmentOrderOpen(id: $id) {
      fulfillmentOrder {
        id
        status
        updatedAt
        supportedActions {
          action
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const reportProgressMutation = `#graphql
  mutation FulfillmentOrderStatusPreconditionReportProgress(
    $id: ID!
    $progressReport: FulfillmentOrderReportProgressInput
  ) {
    fulfillmentOrderReportProgress(id: $id, progressReport: $progressReport) {
      fulfillmentOrder {
        id
        status
        updatedAt
        supportedActions {
          action
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
  query FulfillmentOrderStatusPreconditionOrderRead($orderId: ID!) {
    order(id: $orderId) {
      id
    fulfillmentOrders(first: 10) {
        nodes {
          id
          status
          updatedAt
          supportedActions {
            action
          }
        }
      }
    }
  }
`;

const fulfillmentOrderReadQuery = `#graphql
  ${fulfillmentOrderFields}
  query FulfillmentOrderPreconditionsRead($id: ID!) {
    fulfillmentOrder(id: $id) {
      ...FulfillmentOrderPreconditionFields
    }
  }
`;

const fulfillmentOrderStatusSearchQuery = `#graphql
  query FulfillmentOrderPreconditionsStatusSearch($query: String!, $first: Int!) {
    fulfillmentOrders(first: $first, includeClosed: true, query: $query, sortKey: ID, reverse: true) {
      nodes {
        id
        status
        requestStatus
        order {
          id
          name
        }
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation FulfillmentOrderPreconditionsCleanup(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job {
        id
        done
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const shippingFulfillmentOrderHydrateQuery = `
query ShippingFulfillmentOrderHydrate($id: ID!) {
  node(id: $id) {
    __typename
    ... on FulfillmentOrder {
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
      lineItems(first: 250) {
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
  }
}
`;

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(name: string, query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  const response = await runGraphqlRequest(query, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${name} failed: ${JSON.stringify(response.payload, null, 2)}`);
  }
  return { query: trimGraphql(query), variables, response };
}

async function captureUpstreamHydrate(id: string): Promise<RecordedUpstreamCall> {
  const response = await runGraphqlRequest(shippingFulfillmentOrderHydrateQuery, { id });
  const payload = readObject(response.payload);
  const errors = payload?.['errors'];
  const node = readObject(readObject(payload?.['data'])?.['node']);
  const allowedHeldByAppErrors =
    Array.isArray(errors) &&
    errors.every((error) =>
      String(readObject(error)?.['message'] ?? '').startsWith('Access denied for heldByApp field.'),
    ) &&
    node !== null;
  if (response.status < 200 || response.status >= 300 || (errors && !allowedHeldByAppErrors) || node === null) {
    throw new Error(`ShippingFulfillmentOrderHydrate failed: ${JSON.stringify(response.payload, null, 2)}`);
  }
  return {
    operationName: 'ShippingFulfillmentOrderHydrate',
    query: shippingFulfillmentOrderHydrateQuery,
    variables: { id },
    response: {
      status: response.status,
      body: response.payload,
    },
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

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) throw new Error(`Missing ${label}`);
  return value;
}

function assertNoUserErrors(captureResult: GraphqlCapture, pathParts: string[], label: string): void {
  let cursor: unknown = captureResult.response.payload;
  for (const part of pathParts) cursor = readObject(cursor)?.[part];
  if (Array.isArray(cursor) && cursor.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(cursor, null, 2)}`);
}

function assertUserError(captureResult: GraphqlCapture, pathParts: string[], message: string): void {
  let cursor: unknown = captureResult.response.payload;
  for (const part of pathParts) cursor = readObject(cursor)?.[part];
  const errors = readObject(cursor)?.['userErrors'];
  if (
    Array.isArray(errors) &&
    errors.length === 1 &&
    readObject(errors[0])?.['field'] === null &&
    readObject(errors[0])?.['message'] === message
  ) {
    return;
  }
  throw new Error(`Unexpected userErrors for ${pathParts.join('.')}: ${JSON.stringify(errors, null, 2)}`);
}

function createdOrderFrom(captureResult: GraphqlCapture): CreatedOrder {
  const data = readObject(captureResult.response.payload.data);
  const orderCreate = readObject(data?.['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  const fulfillmentOrder = readNodes(readObject(order?.['fulfillmentOrders']))[0];
  const lineItem = readNodes(readObject(fulfillmentOrder?.['lineItems']))[0];
  return {
    id: requireString(order?.['id'], 'order.id'),
    name: typeof order?.['name'] === 'string' ? order['name'] : null,
    fulfillmentOrderId: requireString(fulfillmentOrder?.['id'], 'fulfillmentOrder.id'),
    fulfillmentOrderLineItemId: requireString(lineItem?.['id'], 'fulfillmentOrder.lineItems.nodes[0].id'),
  };
}

async function createOrder(label: string): Promise<{ create: GraphqlCapture; order: CreatedOrder }> {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const create = await capture('orderCreate', orderCreateMutation, {
    order: {
      email: `fulfillment-order-preconditions-${label}-${stamp}@example.com`,
      note: `fulfillment order preconditions ${label} ${stamp}`,
      tags: ['fulfillment-order-preconditions', label],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'Fulfillment',
        lastName: 'Precondition',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          title: `Fulfillment order preconditions ${label} ${stamp}`,
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
  assertNoUserErrors(create, ['data', 'orderCreate', 'userErrors'], `${label} orderCreate`);
  return { create, order: createdOrderFrom(create) };
}

async function readFulfillmentOrder(id: string): Promise<GraphqlCapture> {
  return capture('fulfillmentOrderRead', fulfillmentOrderReadQuery, { id });
}

async function waitForFulfillmentOrderStatus(id: string, wanted: string): Promise<GraphqlCapture> {
  let latest = await readFulfillmentOrder(id);
  for (let attempt = 0; attempt < 12; attempt += 1) {
    const data = readObject(latest.response.payload.data);
    const fulfillmentOrder = readObject(data?.['fulfillmentOrder']);
    if (fulfillmentOrder?.['status'] === wanted) return latest;
    await new Promise((resolve) => setTimeout(resolve, 500));
    latest = await readFulfillmentOrder(id);
  }
  throw new Error(`Timed out waiting for fulfillment order ${id} to reach ${wanted}`);
}

async function readAfter(order: CreatedOrder): Promise<GraphqlCapture> {
  return capture('orderRead', orderReadQuery, { orderId: order.id });
}

async function readAfterOrderId(orderId: string): Promise<GraphqlCapture> {
  return capture('orderRead', orderReadQuery, { orderId });
}

async function cleanupOrder(order: CreatedOrder): Promise<GraphqlCapture> {
  return capture('orderCancel.cleanup', orderCancelMutation, {
    orderId: order.id,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });
}

const startedAt = new Date().toISOString();
const createdOrders: CreatedOrder[] = [];
const cleanup: JsonRecord = {};

try {
  const closedSearch = await capture('closedStatusSearch', fulfillmentOrderStatusSearchQuery, {
    query: 'status:closed',
    first: 50,
  });
  let closedNode: JsonRecord | undefined;
  let openClosed: GraphqlCapture | undefined;
  for (const [index, candidate] of readNodes(
    readObject(closedSearch.response.payload.data)?.['fulfillmentOrders'],
  ).entries()) {
    const candidateId = requireString(candidate['id'], `closed candidate ${index} id`);
    const candidateOpen = await capture(`closed.fulfillmentOrderOpen.candidate.${index}`, openMutation, {
      id: candidateId,
    });
    const userErrors = readObject(readObject(candidateOpen.response.payload.data)?.['fulfillmentOrderOpen'])?.[
      'userErrors'
    ];
    const firstError = Array.isArray(userErrors) ? readObject(userErrors[0]) : null;
    if (
      firstError?.['field'] === null &&
      firstError?.['message'] === 'Expected fulfillment order status to be valid but it was closed.'
    ) {
      closedNode = candidate;
      openClosed = candidateOpen;
      break;
    }
  }
  if (!closedNode || !openClosed) {
    throw new Error('No live CLOSED fulfillment order produced the plain closed fulfillmentOrderOpen userError.');
  }
  assertUserError(
    openClosed,
    ['data', 'fulfillmentOrderOpen'],
    'Expected fulfillment order status to be valid but it was closed.',
  );
  const closedOrder = readObject(closedNode['order']);
  const closedOrderId = requireString(closedOrder?.['id'], 'closed order.id');
  const closedFulfillmentOrderId = requireString(closedNode['id'], 'closed fulfillmentOrder.id');
  const closedHydrate = await readFulfillmentOrder(closedFulfillmentOrderId);
  const closedUpstream = await captureUpstreamHydrate(closedFulfillmentOrderId);
  const afterOpenClosedRead = await readAfterOrderId(closedOrderId);

  const onHold = await createOrder('on-hold');
  createdOrders.push(onHold.order);
  const hold = await capture('onHold.fulfillmentOrderHold', holdMutation, {
    id: onHold.order.fulfillmentOrderId,
    fulfillmentHold: {
      reason: 'OTHER',
      reasonNotes: 'fulfillment order precondition hold setup',
      notifyMerchant: false,
      externalId: `fulfillment-order-precondition-hold-${Date.now()}`,
    },
  });
  assertNoUserErrors(hold, ['data', 'fulfillmentOrderHold', 'userErrors'], 'onHold fulfillmentOrderHold');
  const onHoldHydrate = await waitForFulfillmentOrderStatus(onHold.order.fulfillmentOrderId, 'ON_HOLD');
  const onHoldUpstream = await captureUpstreamHydrate(onHold.order.fulfillmentOrderId);
  const openOnHold = await capture('onHold.fulfillmentOrderOpen', openMutation, {
    id: onHold.order.fulfillmentOrderId,
  });
  assertUserError(
    openOnHold,
    ['data', 'fulfillmentOrderOpen'],
    'Expected fulfillment order status to be valid but it was on_hold.',
  );
  const afterOpenOnHoldRead = await readAfter(onHold.order);
  const reportProgressOnHold = await capture('onHold.fulfillmentOrderReportProgress', reportProgressMutation, {
    id: onHold.order.fulfillmentOrderId,
    progressReport: {
      reasonNotes: 'fulfillment order precondition on-hold report progress rejection',
    },
  });
  assertUserError(
    reportProgressOnHold,
    ['data', 'fulfillmentOrderReportProgress'],
    'Cannot report progress on a fulfillment order in this state.',
  );
  const afterReportProgressOnHoldRead = await readAfter(onHold.order);

  const cancelledSearch = await capture('cancelledStatusSearch', fulfillmentOrderStatusSearchQuery, {
    query: 'status:CANCELLED',
    first: 5,
  });

  for (const order of [...createdOrders].reverse()) {
    cleanup[order.id] = await cleanupOrder(order);
  }

  await writeJson(outputPath, {
    metadata: {
      capturedAt: new Date().toISOString(),
      startedAt,
      storeDomain,
      apiVersion,
      scopedRoots: ['fulfillmentOrderOpen', 'fulfillmentOrderReportProgress'],
      notes: [
        'Public Admin GraphQL 2026-04 exposes field/message for these userErrors; local runtime tests cover the proxy-only code projection.',
        'The report-progress live rejection uses the ON_HOLD fulfillment order created in this capture; SCHEDULED and CANCELLED report-progress branches remain covered by store-state runtime tests.',
        'ON_HOLD upstream hydration records Shopify partial data as returned to the proxy when the app lacks read_apps for fulfillmentHolds.heldByApp.',
        'The disposable conformance shop had no queryable fulfillment order with status CANCELLED during this capture; the proxy keeps the CANCELLED branch covered by store-state runtime tests.',
      ],
      createdOrders,
    },
    closed: {
      search: closedSearch,
      hydrate: closedHydrate,
      open: openClosed,
      afterOpenRead: afterOpenClosedRead,
    },
    onHold: {
      create: onHold.create,
      setup: hold,
      hydrate: onHoldHydrate,
      open: openOnHold,
      afterOpenRead: afterOpenOnHoldRead,
      reportProgress: reportProgressOnHold,
      afterReportProgressRead: afterReportProgressOnHoldRead,
    },
    cancelledSearch,
    cleanup,
    upstreamCalls: [closedUpstream, onHoldUpstream],
  });

  console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath: outputPath }, null, 2));
} catch (error) {
  for (const order of [...createdOrders].reverse()) {
    if (cleanup[order.id] !== undefined) continue;
    try {
      cleanup[order.id] = await cleanupOrder(order);
    } catch (cleanupError) {
      console.error(`cleanup failed for ${order.id}:`, cleanupError);
    }
  }
  throw error;
}
