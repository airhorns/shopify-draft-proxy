/* oxlint-disable no-console -- CLI capture scripts intentionally report live capture results. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type CaptureStep = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'payments',
  'order-payment-transaction-cold-hydration.json',
);
const requestRoot = path.join('config', 'parity-requests', 'payments');
const createRequestPath = path.join(requestRoot, 'order-payment-transaction-void-create.graphql');
const captureRequestPath = path.join(requestRoot, 'order-capture-validation-order-capture.graphql');
const voidRequestPath = path.join(requestRoot, 'order-payment-transaction-void.graphql');
const readRequestPath = path.join(requestRoot, 'order-payment-transaction-void-read.graphql');
const orderHydrateRequestPath = path.join(requestRoot, 'order-payment-transaction-hydrate-by-order.graphql');
const transactionHydrateRequestPath = path.join(
  requestRoot,
  'order-payment-transaction-hydrate-by-transaction.graphql',
);

const cleanupDocument = `
mutation OrderPaymentTransactionColdHydrationCleanup(
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
`.trim();

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readArray(value: unknown, key: string): unknown[] {
  const field = asRecord(value)?.[key];
  return Array.isArray(field) ? field : [];
}

function readString(value: unknown, key: string): string | null {
  const field = asRecord(value)?.[key];
  return typeof field === 'string' && field.length > 0 ? field : null;
}

function root(step: CaptureStep, rootName: string): JsonRecord | null {
  return readRecord(step.response.payload.data, rootName);
}

function requireString(value: string | null, context: string): string {
  if (value === null) throw new Error(`Missing ${context}`);
  return value;
}

function requireNoTopLevelErrors(step: CaptureStep, context: string): void {
  if (step.response.status < 200 || step.response.status >= 300 || step.response.payload.errors) {
    throw new Error(`${context} returned top-level errors: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
}

function requireNoUserErrors(step: CaptureStep, rootName: string, context: string): void {
  requireNoTopLevelErrors(step, context);
  const errors = readArray(root(step, rootName), 'userErrors');
  if (errors.length > 0) throw new Error(`${context} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
}

function createdOrder(step: CaptureStep): JsonRecord {
  requireNoUserErrors(step, 'orderCreate', 'orderCreate');
  const order = readRecord(root(step, 'orderCreate'), 'order');
  if (order === null) throw new Error(`orderCreate returned no order: ${JSON.stringify(step.response.payload)}`);
  return order;
}

function firstTransaction(order: JsonRecord): JsonRecord {
  const transaction = asRecord(readArray(order, 'transactions')[0]);
  if (transaction === null) throw new Error(`Order returned no transaction: ${JSON.stringify(order)}`);
  return transaction;
}

async function readRequest(filePath: string): Promise<string> {
  return await readFile(filePath, 'utf8');
}

async function capture(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  const response = await runGraphqlRequest<JsonRecord>(query, variables);
  const step = { query, variables, response };
  requireNoTopLevelErrors(step, context);
  return step;
}

function temporarilyUnavailable(step: CaptureStep, rootName: string): boolean {
  return readArray(root(step, rootName), 'userErrors').some((error) => {
    return readString(asRecord(error), 'message') === 'Order is temporarily unavailable to be modified.';
  });
}

async function captureOrderMutation(
  query: string,
  variables: JsonRecord,
  rootName: 'orderCapture' | 'transactionVoid',
  context: string,
): Promise<CaptureStep> {
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    const step = await capture(query, variables, context);
    if (!temporarilyUnavailable(step, rootName) || attempt === 5) return step;
    await new Promise((resolve) => setTimeout(resolve, attempt * 2_000));
  }
  throw new Error(`${context} retry loop exhausted unexpectedly`);
}

function orderVariables(stamp: number, label: string, status: 'SUCCESS' | 'FAILURE'): JsonRecord {
  return {
    order: {
      email: `payment-cold-hydration-${label}-${stamp}@example.com`,
      note: `order payment transaction cold hydration ${label} ${stamp}`,
      tags: ['order-payment-transaction-cold-hydration', label, String(stamp)],
      test: true,
      currency: 'CAD',
      presentmentCurrency: 'CAD',
      transactions: [
        {
          kind: 'AUTHORIZATION',
          status,
          gateway: 'manual',
          test: true,
          amountSet: {
            shopMoney: { amount: '25.00', currencyCode: 'CAD' },
          },
        },
      ],
      lineItems: [
        {
          title: `Payment cold hydration ${label} ${stamp}`,
          quantity: 1,
          priceSet: {
            shopMoney: { amount: '25.00', currencyCode: 'CAD' },
          },
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
  };
}

function captureVariables(orderId: string, transactionId: string, amount: string): JsonRecord {
  return {
    input: {
      id: orderId,
      parentTransactionId: transactionId,
      amount,
    },
  };
}

function upstreamCall(operationName: string, step: CaptureStep): JsonRecord {
  return {
    operationName,
    variables: step.variables,
    query: step.query,
    response: {
      status: step.response.status,
      body: step.response.payload,
    },
  };
}

async function cleanupOrder(orderId: string): Promise<CaptureStep> {
  const step = await capture(
    cleanupDocument,
    { orderId, reason: 'OTHER', notifyCustomer: false, restock: true },
    `cleanup ${orderId}`,
  );
  requireNoUserErrors(step, 'orderCancel', `cleanup ${orderId}`);
  return step;
}

async function writeJson(filePath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

const [createDocument, captureDocument, voidDocument, readDocument, orderHydrateDocument, transactionHydrateDocument] =
  await Promise.all([
    readRequest(createRequestPath),
    readRequest(captureRequestPath),
    readRequest(voidRequestPath),
    readRequest(readRequestPath),
    readRequest(orderHydrateRequestPath),
    readRequest(transactionHydrateRequestPath),
  ]);

const stamp = Date.now();
const createdOrderIds: string[] = [];
const cleanup: Record<string, unknown> = {};

try {
  const captureCreate = await capture(
    createDocument,
    orderVariables(stamp, 'capture', 'SUCCESS'),
    'capture orderCreate',
  );
  const captureOrder = createdOrder(captureCreate);
  const captureOrderId = requireString(readString(captureOrder, 'id'), 'capture order id');
  const captureAuthorizationId = requireString(
    readString(firstTransaction(captureOrder), 'id'),
    'capture authorization id',
  );
  createdOrderIds.push(captureOrderId);
  const captureHydrate = await capture(orderHydrateDocument, { id: captureOrderId }, 'capture order hydrate');
  const captureSuccess = await captureOrderMutation(
    captureDocument,
    captureVariables(captureOrderId, captureAuthorizationId, '25.00'),
    'orderCapture',
    'cold capture success',
  );
  requireNoUserErrors(captureSuccess, 'orderCapture', 'cold capture success');
  const captureTransactionId = requireString(
    readString(readRecord(root(captureSuccess, 'orderCapture'), 'transaction'), 'id'),
    'capture transaction id',
  );
  const captureAlreadyCaptured = await captureOrderMutation(
    captureDocument,
    captureVariables(captureOrderId, captureAuthorizationId, '1.00'),
    'orderCapture',
    'already captured authorization',
  );
  const captureWrongParentKind = await captureOrderMutation(
    captureDocument,
    captureVariables(captureOrderId, captureTransactionId, '1.00'),
    'orderCapture',
    'capture parent has wrong transaction kind',
  );
  const captureReadAfter = await capture(readDocument, { id: captureOrderId }, 'capture downstream read');

  const voidCreate = await capture(createDocument, orderVariables(stamp, 'void', 'SUCCESS'), 'void orderCreate');
  const voidOrder = createdOrder(voidCreate);
  const voidOrderId = requireString(readString(voidOrder, 'id'), 'void order id');
  const voidAuthorizationId = requireString(readString(firstTransaction(voidOrder), 'id'), 'void authorization id');
  createdOrderIds.push(voidOrderId);
  const voidOrderHydrate = await capture(orderHydrateDocument, { id: voidOrderId }, 'void order hydrate');
  const voidTransactionHydrate = await capture(
    transactionHydrateDocument,
    { id: voidAuthorizationId },
    'void transaction hydrate',
  );
  const voidSuccess = await captureOrderMutation(
    voidDocument,
    { id: voidAuthorizationId },
    'transactionVoid',
    'cold void success',
  );
  requireNoUserErrors(voidSuccess, 'transactionVoid', 'cold void success');
  const voidAlreadyVoided = await captureOrderMutation(
    voidDocument,
    { id: voidAuthorizationId },
    'transactionVoid',
    'already voided authorization',
  );
  const voidReadAfter = await capture(readDocument, { id: voidOrderId }, 'void downstream read');

  const failedCreate = await capture(
    createDocument,
    orderVariables(stamp, 'failed-authorization', 'FAILURE'),
    'failed authorization orderCreate',
  );
  const failedOrder = createdOrder(failedCreate);
  const failedOrderId = requireString(readString(failedOrder, 'id'), 'failed authorization order id');
  const failedAuthorizationId = requireString(
    readString(firstTransaction(failedOrder), 'id'),
    'failed authorization transaction id',
  );
  createdOrderIds.push(failedOrderId);
  const failedAuthorizationHydrate = await capture(
    transactionHydrateDocument,
    { id: failedAuthorizationId },
    'failed authorization hydrate',
  );
  const voidFailedAuthorization = await captureOrderMutation(
    voidDocument,
    { id: failedAuthorizationId },
    'transactionVoid',
    'void failed authorization',
  );

  const missingOrderId = 'gid://shopify/Order/999999999999999999';
  const missingTransactionId = 'gid://shopify/OrderTransaction/999999999999999999';
  const missingOrderHydrate = await capture(orderHydrateDocument, { id: missingOrderId }, 'missing order hydrate');
  const captureMissing = await captureOrderMutation(
    captureDocument,
    captureVariables(missingOrderId, missingTransactionId, '1.00'),
    'orderCapture',
    'capture confirmed missing transaction',
  );
  const missingTransactionHydrate = await capture(
    transactionHydrateDocument,
    { id: missingTransactionId },
    'missing transaction hydrate',
  );
  const voidMissing = await captureOrderMutation(
    voidDocument,
    { id: missingTransactionId },
    'transactionVoid',
    'void confirmed missing transaction',
  );

  for (const orderId of [...createdOrderIds].reverse()) {
    cleanup[orderId] = await cleanupOrder(orderId);
  }

  await writeJson(fixturePath, {
    scenarioId: 'order-payment-transaction-cold-hydration',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    source: 'live-shopify-admin-graphql',
    notes:
      'Real Admin GraphQL capture for mutation-first and read-warmed replay of the same manual authorization targets. Exact query-only order/transaction hydrate calls are recorded for LiveHybrid cassette replay; supported capture/void writes remain local in proxy parity.',
    flows: {
      capture: {
        create: captureCreate,
        hydrate: captureHydrate,
        success: captureSuccess,
        alreadyCaptured: captureAlreadyCaptured,
        wrongParentKind: captureWrongParentKind,
        readAfter: captureReadAfter,
      },
      void: {
        create: voidCreate,
        orderHydrate: voidOrderHydrate,
        transactionHydrate: voidTransactionHydrate,
        success: voidSuccess,
        alreadyVoided: voidAlreadyVoided,
        readAfter: voidReadAfter,
      },
      failedAuthorization: {
        create: failedCreate,
        hydrate: failedAuthorizationHydrate,
        void: voidFailedAuthorization,
      },
      confirmedMissing: {
        orderHydrate: missingOrderHydrate,
        capture: captureMissing,
        transactionHydrate: missingTransactionHydrate,
        void: voidMissing,
      },
    },
    upstreamCalls: [
      upstreamCall('OrderPaymentTransactionHydrateByOrder', captureHydrate),
      upstreamCall('OrderPaymentTransactionHydrateByOrder', voidOrderHydrate),
      upstreamCall('OrderPaymentTransactionHydrateByTransaction', voidTransactionHydrate),
      upstreamCall('OrderPaymentTransactionHydrateByTransaction', failedAuthorizationHydrate),
      upstreamCall('OrderPaymentTransactionHydrateByOrder', missingOrderHydrate),
      upstreamCall('OrderPaymentTransactionHydrateByTransaction', missingTransactionHydrate),
    ],
    cleanup,
  });

  console.log(
    JSON.stringify(
      {
        fixturePath,
        capturedOrders: createdOrderIds.length,
        captureAlreadyCapturedErrors: readArray(root(captureAlreadyCaptured, 'orderCapture'), 'userErrors'),
        captureWrongParentKindErrors: readArray(root(captureWrongParentKind, 'orderCapture'), 'userErrors'),
        voidAlreadyVoidedErrors: readArray(root(voidAlreadyVoided, 'transactionVoid'), 'userErrors'),
        voidFailedAuthorizationErrors: readArray(root(voidFailedAuthorization, 'transactionVoid'), 'userErrors'),
      },
      null,
      2,
    ),
  );
} catch (error) {
  for (const orderId of [...createdOrderIds].reverse()) {
    if (cleanup[orderId]) continue;
    try {
      cleanup[orderId] = await cleanupOrder(orderId);
    } catch (cleanupError) {
      cleanup[orderId] = { error: cleanupError instanceof Error ? cleanupError.message : String(cleanupError) };
    }
  }
  throw error;
}
