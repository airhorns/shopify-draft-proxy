/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import { setTimeout as sleep } from 'node:timers/promises';

import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';
import type { ConformanceGraphqlResult } from './conformance-graphql-client.js';

type CaptureStep = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const scenarioId = 'orderCancel-error-messages';
const conflictMessage = 'Only one of the arguments `refund` or `refund_method` is allowed.';
const staffNoteTooLongMessage = 'Staff note is too long. Maximum length is 255 characters.';
const alreadyCancelledMessage = 'Cannot cancel an order that has already been canceled';
const longStaffNote = 'x'.repeat(300);

const cap = await createConformanceCapture();

if (cap.apiVersion !== '2026-04') {
  throw new Error(`${scenarioId} capture requires SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${cap.apiVersion}`);
}

const orderCreateDocument = await cap.readRequest('orders', 'orderCancel-error-messages-order-create.graphql');
const orderCancelDocument = await cap.readRequest('orders', 'orderCancel-error-messages.graphql');
const setupCancelDocument = await cap.readRequest('orders', 'orderCancel-error-messages-setup-cancel.graphql');

const orderReadDocument = `#graphql
  query OrderCancelErrorMessagesRead($id: ID!) {
    order(id: $id) {
      id
      closed
      closedAt
      cancelledAt
      cancelReason
    }
  }
`;

const ordersSearchDocument = `#graphql
  query OrderCancelErrorMessagesOrdersSearch($query: String!) {
    orders(first: 10, query: $query, sortKey: CREATED_AT, reverse: true) {
      nodes {
        id
        name
        closed
        cancelledAt
        cancelReason
        tags
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord, label: string): Promise<CaptureStep> {
  const trimmedQuery = trimGraphql(query);
  const response = await cap.runGraphqlRequest<JsonRecord>(trimmedQuery, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(response.payload, null, 2)}`);
  }
  return { query: trimmedQuery, variables, response };
}

function rootPayload(step: CaptureStep, rootName: string): JsonRecord {
  const root = readRecord(readRecord(step.response.payload.data)?.[rootName]);
  if (!root) {
    throw new Error(`Missing ${rootName} payload: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return root;
}

function createdOrderId(step: CaptureStep): string {
  const order = readRecord(rootPayload(step, 'orderCreate')['order']);
  return requireString(order?.['id'], 'created order id');
}

function assertEmptyUserErrors(step: CaptureStep, rootName: string, label: string): void {
  const root = rootPayload(step, rootName);
  const userErrors = readArray(root['userErrors']);
  const orderCancelUserErrors = readArray(root['orderCancelUserErrors']);
  if (userErrors.length === 0 && (rootName !== 'orderCancel' || orderCancelUserErrors.length === 0)) {
    return;
  }
  throw new Error(`${label} returned userErrors: ${JSON.stringify(root, null, 2)}`);
}

function assertOrderCancelUserError(step: CaptureStep, field: string[], message: string, label: string): void {
  const root = rootPayload(step, 'orderCancel');
  const expectedCoded = [{ field, message, code: 'INVALID' }];
  const expectedDisplayable = [{ field, message }];
  const userErrors = readArray(root['userErrors']);
  const orderCancelUserErrors = readArray(root['orderCancelUserErrors']);
  if (
    JSON.stringify(userErrors) === JSON.stringify(expectedDisplayable) &&
    JSON.stringify(orderCancelUserErrors) === JSON.stringify(expectedCoded) &&
    root['job'] === null
  ) {
    return;
  }
  throw new Error(`${label} did not match expected userError: ${JSON.stringify(root, null, 2)}`);
}

function orderCreateVariables(label: string): JsonRecord {
  return {
    order: {
      email: `order-cancel-error-messages-${label}-${cap.stamp}@example.com`,
      note: `orderCancel error messages ${label} ${cap.stamp}`,
      tags: ['order-cancel-error-messages', label, cap.stamp],
      test: true,
      currency: 'USD',
      lineItems: [
        {
          title: `Order cancel error messages ${label}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'USD',
            },
          },
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
}

function cancelVariables(orderId: string): JsonRecord {
  return {
    orderId,
    restock: false,
    reason: 'OTHER',
  };
}

async function waitForCancelled(orderId: string): Promise<CaptureStep> {
  let lastRead: CaptureStep | null = null;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    if (attempt > 0) await sleep(1000);
    lastRead = await capture(orderReadDocument, { id: orderId }, `read cancelled order attempt ${attempt + 1}`);
    const order = readRecord(readRecord(lastRead.response.payload.data)?.['order']);
    if (typeof order?.['cancelledAt'] === 'string') {
      return lastRead;
    }
  }
  throw new Error(`Order did not become cancelled: ${JSON.stringify(lastRead?.response.payload, null, 2)}`);
}

async function searchOrders(query: string): Promise<CaptureStep> {
  return capture(ordersSearchDocument, { query }, `search orders ${query}`);
}

function orderNodes(search: CaptureStep): JsonRecord[] {
  const orders = readRecord(readRecord(search.response.payload.data)?.['orders']);
  return readArray(orders?.['nodes']).flatMap((node) => {
    const order = readRecord(node);
    return order ? [order] : [];
  });
}

function firstCancelledOrderId(search: CaptureStep): string | null {
  for (const order of orderNodes(search)) {
    if (typeof order['id'] === 'string' && typeof order['cancelledAt'] === 'string') {
      return order['id'];
    }
  }
  return null;
}

async function cleanupOpenTaggedOrders(): Promise<void> {
  const search = await searchOrders('tag:order-cancel-error-messages');
  for (const order of orderNodes(search)) {
    if (typeof order['id'] === 'string' && order['cancelledAt'] === null) {
      await capture(setupCancelDocument, cancelVariables(order['id']), `cleanup prior ${order['id']}`);
    }
  }
}

await cleanupOpenTaggedOrders();

const freshOrderCreate = await capture(orderCreateDocument, orderCreateVariables('fresh'), 'create fresh order');
assertEmptyUserErrors(freshOrderCreate, 'orderCreate', 'create fresh order');
const freshOrderId = createdOrderId(freshOrderCreate);

const cancelledOrderCreate = await capture(
  orderCreateDocument,
  orderCreateVariables('already-cancelled'),
  'create already-cancelled order',
);
assertEmptyUserErrors(cancelledOrderCreate, 'orderCreate', 'create already-cancelled order');
const cancelledOrderId = createdOrderId(cancelledOrderCreate);

const setupCancel = await capture(setupCancelDocument, cancelVariables(cancelledOrderId), 'setup cancel order');
assertEmptyUserErrors(setupCancel, 'orderCancel', 'setup cancel order');
const cancelledOrderRead = await waitForCancelled(cancelledOrderId).catch(() => null);
let taggedCancelledOrderSearch: CaptureStep | null = null;
let existingCancelledOrderSearch: CaptureStep | null = null;
let alreadyCancelledOrderId: string | null = cancelledOrderRead ? cancelledOrderId : null;
if (!alreadyCancelledOrderId) {
  taggedCancelledOrderSearch = await searchOrders('tag:order-cancel-error-messages status:cancelled');
  alreadyCancelledOrderId = firstCancelledOrderId(taggedCancelledOrderSearch);
}
if (!alreadyCancelledOrderId) {
  existingCancelledOrderSearch = await searchOrders('status:cancelled');
  alreadyCancelledOrderId = firstCancelledOrderId(existingCancelledOrderSearch);
}
if (!alreadyCancelledOrderId) {
  throw new Error('Could not resolve a cancelled order id for already-cancelled capture');
}

const staffNoteTooLong = await capture(
  orderCancelDocument,
  {
    ...cancelVariables(freshOrderId),
    staffNote: longStaffNote,
  },
  'staffNote too long',
);
assertOrderCancelUserError(staffNoteTooLong, ['staffNote'], staffNoteTooLongMessage, 'staffNote too long');

const refundTrueAndRefundMethodConflict = await capture(
  orderCancelDocument,
  {
    ...cancelVariables(freshOrderId),
    refund: true,
    refundMethod: { originalPaymentMethodsRefund: true },
  },
  'refund true and refundMethod conflict',
);
assertOrderCancelUserError(refundTrueAndRefundMethodConflict, ['refund'], conflictMessage, 'refund true conflict');

const refundFalseAndRefundMethodConflict = await capture(
  orderCancelDocument,
  {
    ...cancelVariables(freshOrderId),
    refund: false,
    refundMethod: { originalPaymentMethodsRefund: true },
  },
  'refund false and refundMethod conflict',
);
assertOrderCancelUserError(refundFalseAndRefundMethodConflict, ['refund'], conflictMessage, 'refund false conflict');

const alreadyCancelled = await capture(
  orderCancelDocument,
  cancelVariables(alreadyCancelledOrderId),
  'already-cancelled order',
);
assertOrderCancelUserError(alreadyCancelled, ['orderId'], alreadyCancelledMessage, 'already-cancelled order');

const freshOrderCancel = await capture(setupCancelDocument, cancelVariables(freshOrderId), 'cleanup fresh order');
assertEmptyUserErrors(freshOrderCancel, 'orderCancel', 'cleanup fresh order');

const fixturePath = cap.fixturePath('orders', 'orderCancel-error-messages.json');
await cap.writeJson(fixturePath, {
  scenarioId,
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  setup: {
    freshOrderCreate,
    cancelledOrderCreate,
    setupCancel,
    cancelledOrderRead,
    taggedCancelledOrderSearch,
    existingCancelledOrderSearch,
  },
  expected: {
    staffNoteTooLong,
    refundTrueAndRefundMethodConflict,
    refundFalseAndRefundMethodConflict,
    alreadyCancelled,
  },
  cleanup: {
    freshOrderCancel,
  },
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      freshOrderId,
      cancelledOrderId,
      alreadyCancelledOrderId,
      staffNoteTooLong: rootPayload(staffNoteTooLong, 'orderCancel')['userErrors'],
      refundTrueAndRefundMethodConflict: rootPayload(refundTrueAndRefundMethodConflict, 'orderCancel')['userErrors'],
      refundFalseAndRefundMethodConflict: rootPayload(refundFalseAndRefundMethodConflict, 'orderCancel')['userErrors'],
      alreadyCancelled: rootPayload(alreadyCancelled, 'orderCancel')['userErrors'],
    },
    null,
    2,
  ),
);
