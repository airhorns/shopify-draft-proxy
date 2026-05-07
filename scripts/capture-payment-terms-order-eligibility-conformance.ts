/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
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
  label: string;
  create: {
    query: string;
    variables: JsonRecord;
    response: ConformanceGraphqlPayload<JsonRecord>;
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

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const outputPath = path.join(outputDir, 'payment-terms-create-order-eligibility.json');
const cleanupPath = path.join(outputDir, 'payment-terms-create-order-eligibility-cleanup.json');

const orderCreateDocument = `#graphql
  mutation PaymentTermsOrderEligibilityOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFinancialStatus
        closed
        closedAt
        cancelledAt
        totalOutstandingSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        currentTotalPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        totalPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCloseDocument = `#graphql
  mutation PaymentTermsOrderEligibilityOrderClose($input: OrderCloseInput!) {
    orderClose(input: $input) {
      order {
        id
        displayFinancialStatus
        closed
        closedAt
        cancelledAt
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation PaymentTermsOrderEligibilityOrderCancel(
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
      orderCancelUserErrors {
        field
        message
        code
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderReadDocument = `#graphql
  query PaymentTermsOrderEligibilityOrderRead($id: ID!) {
    order(id: $id) {
      id
      name
      displayFinancialStatus
      closed
      closedAt
      cancelledAt
      totalOutstandingSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
      currentTotalPriceSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
      totalPriceSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
    }
  }
`;

const paymentTermsOwnerHydrateDocument = `#graphql
  query PaymentTermsOwnerHydrate($id: ID!) {
    order(id: $id) {
      id
      displayFinancialStatus
      closed
      closedAt
      cancelledAt
      paymentTerms {
        id
      }
      totalOutstandingSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
      currentTotalPriceSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
      totalPriceSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
    }
  }
`;

const paymentTermsCreateDocument = `#graphql
  mutation PaymentTermsOrderEligibilityCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
    paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
      paymentTerms {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const paymentTermsDeleteDocument = `#graphql
  mutation PaymentTermsOrderEligibilityTermsCleanup($input: PaymentTermsDeleteInput!) {
    paymentTermsDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const defaultPaymentTermsAttrs = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
  paymentSchedules: [{ issuedAt: '2026-05-05T00:00:00Z' }],
};

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
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

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

async function run(query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  const cleanQuery = trimGraphql(query);
  return {
    query: cleanQuery,
    variables,
    response: await runGraphqlRequest<JsonRecord>(cleanQuery, variables),
  };
}

function assertOk(label: string, capture: GraphqlCapture): void {
  const payload = capture.response.payload;
  if (capture.response.status < 200 || capture.response.status >= 300 || payload['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(payload, null, 2)}`);
  }
}

function assertNoUserErrors(label: string, capture: GraphqlCapture, root: string): void {
  const errors = readArray(readRecord(capture.response.payload['data'], root), 'userErrors');
  const cancelErrors = readArray(readRecord(capture.response.payload['data'], root), 'orderCancelUserErrors');
  if (errors.length === 0 && cancelErrors.length === 0) {
    return;
  }
  throw new Error(`${label} returned user errors: ${JSON.stringify({ errors, cancelErrors }, null, 2)}`);
}

function paymentTermsPayload(capture: GraphqlCapture): JsonRecord | null {
  return readRecord(capture.response.payload['data'], 'paymentTermsCreate');
}

function assertPaymentTermsRejected(label: string, capture: GraphqlCapture, expectedMessage: string): void {
  assertOk(label, capture);
  const payload = paymentTermsPayload(capture);
  const terms = asRecord(payload?.['paymentTerms']);
  const errors = readArray(payload, 'userErrors').map(asRecord);
  const firstError = errors[0] ?? null;
  if (
    terms !== null ||
    errors.length !== 1 ||
    firstError?.['field'] !== null ||
    firstError?.['code'] !== 'PAYMENT_TERMS_CREATION_UNSUCCESSFUL' ||
    firstError?.['message'] !== expectedMessage
  ) {
    throw new Error(`${label} did not match expected rejection: ${JSON.stringify(payload, null, 2)}`);
  }
}

function requireCreatedPaymentTermsId(label: string, capture: GraphqlCapture): string {
  assertOk(label, capture);
  const payload = paymentTermsPayload(capture);
  const terms = asRecord(payload?.['paymentTerms']);
  const errors = readArray(payload, 'userErrors');
  const id = readString(terms, 'id');
  if (!id || errors.length > 0) {
    throw new Error(`${label} did not create payment terms: ${JSON.stringify(payload, null, 2)}`);
  }
  return id;
}

function orderVariables(label: string, stamp: number, paid: boolean): JsonRecord {
  const amount = '12.50';
  const priceSet = {
    shopMoney: { amount, currencyCode: 'USD' },
    presentmentMoney: { amount, currencyCode: 'USD' },
  };
  const order: JsonRecord = {
    email: `payment-terms-order-eligibility-${label}-${stamp}@example.com`,
    note: `payment terms order eligibility ${label} capture`,
    tags: ['shopify-draft-proxy', 'payment-terms-order-eligibility', label],
    test: true,
    currency: 'USD',
    presentmentCurrency: 'USD',
    lineItems: [
      {
        title: `Payment terms order eligibility ${label}`,
        quantity: 1,
        priceSet,
        requiresShipping: false,
        taxable: false,
        sku: `sdp-payment-terms-eligibility-${label}-${stamp}`,
      },
    ],
  };
  if (paid) {
    order['transactions'] = [
      {
        kind: 'SALE',
        status: 'SUCCESS',
        gateway: 'manual',
        test: true,
        amountSet: priceSet,
      },
    ];
  }
  return {
    order,
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
}

async function createOrder(label: string, stamp: number, paid: boolean): Promise<CreatedOrder> {
  const variables = orderVariables(label, stamp, paid);
  const create = await run(orderCreateDocument, variables);
  assertOk(`${label} orderCreate`, create);
  assertNoUserErrors(`${label} orderCreate`, create, 'orderCreate');
  const order = readRecord(readRecord(create.response.payload['data'], 'orderCreate'), 'order');
  const id = readString(order, 'id');
  if (!id) {
    throw new Error(`${label} orderCreate did not return an id: ${JSON.stringify(create.response.payload)}`);
  }
  return {
    id,
    label,
    create: {
      query: create.query,
      variables,
      response: create.response.payload,
    },
  };
}

async function closeOrder(orderId: string): Promise<GraphqlCapture> {
  const close = await run(orderCloseDocument, { input: { id: orderId } });
  assertOk('orderClose setup', close);
  assertNoUserErrors('orderClose setup', close, 'orderClose');
  return close;
}

async function cancelOrder(orderId: string): Promise<GraphqlCapture> {
  const cancel = await run(orderCancelDocument, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
  assertOk('orderCancel setup/cleanup', cancel);
  assertNoUserErrors('orderCancel setup/cleanup', cancel, 'orderCancel');
  return cancel;
}

async function readOrder(orderId: string): Promise<GraphqlCapture> {
  const read = await run(orderReadDocument, { id: orderId });
  assertOk('order read', read);
  return read;
}

function orderFromRead(read: GraphqlCapture): JsonRecord | null {
  return readRecord(read.response.payload['data'], 'order');
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function readOrderUntil(orderId: string, predicate: (order: JsonRecord) => boolean): Promise<GraphqlCapture> {
  let latest = await readOrder(orderId);
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const order = orderFromRead(latest);
    if (order && predicate(order)) {
      return latest;
    }
    await delay(1500);
    latest = await readOrder(orderId);
  }
  throw new Error(`order ${orderId} did not reach expected state: ${JSON.stringify(latest.response.payload)}`);
}

async function hydratePaymentTermsOwner(orderId: string): Promise<GraphqlCapture> {
  const hydrate = await run(paymentTermsOwnerHydrateDocument, { id: orderId });
  assertOk('PaymentTermsOwnerHydrate', hydrate);
  return hydrate;
}

async function capturePaymentTermsCreate(orderId: string): Promise<GraphqlCapture> {
  return run(paymentTermsCreateDocument, {
    referenceId: orderId,
    attrs: defaultPaymentTermsAttrs,
  });
}

async function deletePaymentTerms(paymentTermsId: string): Promise<GraphqlCapture> {
  const deleteTerms = await run(paymentTermsDeleteDocument, { input: { paymentTermsId } });
  assertOk('paymentTermsDelete cleanup', deleteTerms);
  assertNoUserErrors('paymentTermsDelete cleanup', deleteTerms, 'paymentTermsDelete');
  return deleteTerms;
}

function upstreamCallFromHydrate(hydrate: GraphqlCapture): JsonRecord {
  return {
    operationName: 'PaymentTermsOwnerHydrate',
    variables: hydrate.variables,
    query: hydrate.query,
    response: {
      status: hydrate.response.status,
      body: hydrate.response.payload,
    },
  };
}

async function captureRejectedCase(
  label: 'paid',
  order: CreatedOrder,
  setup: JsonRecord,
  expectedMessage: string,
): Promise<{
  setup: JsonRecord;
  hydrate: GraphqlCapture;
  paymentTermsCreate: GraphqlCapture;
}> {
  const hydrate = await hydratePaymentTermsOwner(order.id);
  const paymentTermsCreate = await capturePaymentTermsCreate(order.id);
  assertPaymentTermsRejected(`${label} paymentTermsCreate`, paymentTermsCreate, expectedMessage);
  return { setup, hydrate, paymentTermsCreate };
}

async function captureAcceptedCase(
  label: 'closed' | 'cancelled',
  order: CreatedOrder,
  setup: JsonRecord,
): Promise<{
  setup: JsonRecord;
  hydrate: GraphqlCapture;
  paymentTermsCreate: GraphqlCapture;
  paymentTermsDelete: GraphqlCapture;
}> {
  const hydrate = await hydratePaymentTermsOwner(order.id);
  const paymentTermsCreate = await capturePaymentTermsCreate(order.id);
  const paymentTermsId = requireCreatedPaymentTermsId(`${label} paymentTermsCreate`, paymentTermsCreate);
  const paymentTermsDelete = await deletePaymentTerms(paymentTermsId);
  return { setup, hydrate, paymentTermsCreate, paymentTermsDelete };
}

const paidMessage = 'Cannot create payment terms on an Order that has already been paid in full.';

const stamp = Date.now();
const createdOrders: CreatedOrder[] = [];
const cancelledOrderIds = new Set<string>();
const cleanup: GraphqlCapture[] = [];

try {
  const paidOrder = await createOrder('paid', stamp, true);
  createdOrders.push(paidOrder);
  const paidRead = await readOrderUntil(paidOrder.id, (order) => order['displayFinancialStatus'] === 'PAID');

  const closedOrder = await createOrder('closed', stamp, false);
  createdOrders.push(closedOrder);
  const close = await closeOrder(closedOrder.id);
  const closedRead = await readOrderUntil(
    closedOrder.id,
    (order) => order['closed'] === true && order['closedAt'] !== null,
  );

  const cancelledOrder = await createOrder('cancelled', stamp, false);
  createdOrders.push(cancelledOrder);
  const cancel = await cancelOrder(cancelledOrder.id);
  cancelledOrderIds.add(cancelledOrder.id);
  const cancelledRead = await readOrderUntil(
    cancelledOrder.id,
    (order) => order['closed'] === true && order['cancelledAt'] !== null,
  );

  const paid = await captureRejectedCase(
    'paid',
    paidOrder,
    { orderCreate: paidOrder.create, stateRead: capturePayload(paidRead) },
    paidMessage,
  );
  const closed = await captureAcceptedCase('closed', closedOrder, {
    orderCreate: closedOrder.create,
    orderClose: capturePayload(close),
    stateRead: capturePayload(closedRead),
  });
  const cancelled = await captureAcceptedCase('cancelled', cancelledOrder, {
    orderCreate: cancelledOrder.create,
    orderCancel: capturePayload(cancel),
    stateRead: capturePayload(cancelledRead),
  });

  await writeJson(outputPath, {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    cases: {
      paid: casePayload(paid),
      closed: casePayload(closed),
      cancelled: casePayload(cancelled),
    },
    upstreamCalls: [
      upstreamCallFromHydrate(paid.hydrate),
      upstreamCallFromHydrate(closed.hydrate),
      upstreamCallFromHydrate(cancelled.hydrate),
    ],
    notes:
      'Captured against disposable Shopify test Orders. Shopify rejects paid Orders before creating payment terms, but accepted unpaid closed and cancelled Orders in the public 2026-04 Admin API; setup/cleanup mutations are live-store-only evidence and are not replayed by the proxy parity request.',
  });

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        orders: createdOrders.map((order) => ({ id: order.id, label: order.label })),
        userErrors: {
          paid: readArray(paymentTermsPayload(paid.paymentTermsCreate), 'userErrors'),
          closed: readArray(paymentTermsPayload(closed.paymentTermsCreate), 'userErrors'),
          cancelled: readArray(paymentTermsPayload(cancelled.paymentTermsCreate), 'userErrors'),
        },
      },
      null,
      2,
    ),
  );
} finally {
  for (const order of createdOrders) {
    if (cancelledOrderIds.has(order.id)) {
      continue;
    }
    try {
      cleanup.push(await cancelOrder(order.id));
    } catch (error) {
      cleanup.push({
        query: trimGraphql(orderCancelDocument),
        variables: { orderId: order.id },
        response: {
          status: 0,
          payload: { errors: [{ message: error instanceof Error ? error.message : String(error) }] },
        } as ConformanceGraphqlResult<JsonRecord>,
      });
    }
  }
  if (createdOrders.length > 0) {
    await writeJson(cleanupPath, {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cleanup: cleanup.map(capturePayload),
    });
  }
}

function capturePayload(capture: GraphqlCapture): JsonRecord {
  return {
    query: capture.query,
    variables: capture.variables,
    response: capture.response.payload,
  };
}

function casePayload(caseCapture: {
  setup: JsonRecord;
  hydrate: GraphqlCapture;
  paymentTermsCreate: GraphqlCapture;
  paymentTermsDelete?: GraphqlCapture;
}): JsonRecord {
  return {
    setup: {
      ...caseCapture.setup,
      ...(caseCapture.paymentTermsDelete ? { paymentTermsDelete: capturePayload(caseCapture.paymentTermsDelete) } : {}),
    },
    hydrate: capturePayload(caseCapture.hydrate),
    query: caseCapture.paymentTermsCreate.query,
    variables: caseCapture.paymentTermsCreate.variables,
    response: caseCapture.paymentTermsCreate.response.payload,
  };
}
