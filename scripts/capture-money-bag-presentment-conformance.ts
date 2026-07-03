/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
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

const scenarioId = 'money-bag-presentment-parity';
const expectedApiVersion = '2025-01';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: expectedApiVersion,
  exitOnMissing: true,
});

if (apiVersion !== expectedApiVersion) {
  throw new Error(`${scenarioId} requires SHOPIFY_CONFORMANCE_API_VERSION=${expectedApiVersion}, got ${apiVersion}`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const requestDir = path.join('config', 'parity-requests', 'orders');
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(outputDir, 'money-bag-presentment-parity.json');

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

async function capture(query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  return {
    query,
    variables,
    response: await runGraphqlRequest<JsonRecord>(query, variables),
  };
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function readRecord(value: unknown, context: string): JsonRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function readArray(value: unknown, context: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${context} was not an array: ${JSON.stringify(value)}`);
  }
  return value;
}

function readString(value: unknown, context: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${context} was not a non-empty string: ${JSON.stringify(value)}`);
  }
  return value;
}

function rootPayload(result: GraphqlCapture, rootName: string): JsonRecord {
  const data = readRecord(result.response.payload.data, `${rootName} data`);
  return readRecord(data[rootName], `${rootName} payload`);
}

function assertHttpOk(label: string, result: GraphqlCapture): void {
  if (result.response.status < 200 || result.response.status >= 300) {
    throw new Error(`${label} returned HTTP ${result.response.status}: ${JSON.stringify(result.response.payload)}`);
  }
}

function assertNoTopLevelErrors(label: string, result: GraphqlCapture): void {
  assertHttpOk(label, result);
  if (result.response.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result.response.payload.errors)}`);
  }
}

function assertEmptyUserErrors(label: string, result: GraphqlCapture, rootName: string): void {
  assertNoTopLevelErrors(label, result);
  const errors = readArray(rootPayload(result, rootName)['userErrors'], `${rootName}.userErrors`);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertUserError(
  label: string,
  result: GraphqlCapture,
  rootName: string,
  field: unknown[],
  message: string,
): void {
  assertNoTopLevelErrors(label, result);
  const errors = readArray(rootPayload(result, rootName)['userErrors'], `${rootName}.userErrors`);
  const first = readRecord(errors[0], `${rootName}.userErrors[0]`);
  if (JSON.stringify(first['field']) !== JSON.stringify(field) || first['message'] !== message) {
    throw new Error(`${label} returned unexpected userErrors: ${JSON.stringify(errors)}`);
  }
}

function orderIdFromCreate(result: GraphqlCapture): string {
  const order = readRecord(rootPayload(result, 'orderCreate')['order'], 'orderCreate.order');
  return readString(order['id'], 'orderCreate.order.id');
}

function calculatedOrderIdFromBegin(result: GraphqlCapture): string {
  const calculatedOrder = readRecord(rootPayload(result, 'orderEditBegin')['calculatedOrder'], 'calculatedOrder');
  return readString(calculatedOrder['id'], 'orderEditBegin.calculatedOrder.id');
}

function transactionIdFromMarkAsPaid(result: GraphqlCapture): string {
  const order = readRecord(rootPayload(result, 'orderMarkAsPaid')['order'], 'orderMarkAsPaid.order');
  const transactions = readArray(order['transactions'], 'orderMarkAsPaid.order.transactions');
  const first = readRecord(transactions[0], 'orderMarkAsPaid.order.transactions[0]');
  return readString(first['id'], 'orderMarkAsPaid.order.transactions[0].id');
}

function moneySet(
  amount: string,
  currencyCode: string,
  presentmentAmount = amount,
  presentmentCurrencyCode = currencyCode,
): JsonRecord {
  return {
    shopMoney: { amount, currencyCode },
    presentmentMoney: { amount: presentmentAmount, currencyCode: presentmentCurrencyCode },
  };
}

function orderCreateVariables(stamp: string): JsonRecord {
  return {
    order: {
      email: `money-bag-presentment-${stamp}@example.com`,
      note: `money-bag presentment ${stamp}`,
      tags: ['money-bag-presentment-parity'],
      test: true,
      currency: 'CAD',
      presentmentCurrency: 'USD',
      shippingAddress: {
        firstName: 'Conformance',
        lastName: 'MoneyBag',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          title: `MoneyBag line ${stamp}`,
          quantity: 1,
          sku: `MBAG-${stamp}`,
          requiresShipping: true,
          taxable: true,
          priceSet: moneySet('12.00', 'CAD', '8.00', 'USD'),
          taxLines: [
            {
              title: 'Line tax',
              rate: 0.125,
              priceSet: moneySet('1.50', 'CAD', '1.00', 'USD'),
            },
          ],
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

const [orderCreateDocument, markAsPaidDocument, refundDocument, editBeginDocument, editCommitDocument, cancelDocument] =
  await Promise.all([
    readRequest('money-bag-presentment-single-create.graphql'),
    readRequest('money-bag-presentment-mark-as-paid.graphql'),
    readRequest('money-bag-presentment-refund.graphql'),
    readRequest('money-bag-presentment-order-edit-begin.graphql'),
    readRequest('money-bag-presentment-order-edit-commit.graphql'),
    readRequest('orderCancel-parity.graphql'),
  ]);

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const createdOrderIds: string[] = [];
const cleanup: GraphqlCapture[] = [];

try {
  const orderCreate = await capture(orderCreateDocument, orderCreateVariables(stamp));
  assertEmptyUserErrors('orderCreate', orderCreate, 'orderCreate');
  const orderId = orderIdFromCreate(orderCreate);
  createdOrderIds.push(orderId);

  const orderEditBegin = await capture(editBeginDocument, { id: orderId });
  assertEmptyUserErrors('orderEditBegin', orderEditBegin, 'orderEditBegin');
  const calculatedOrderId = calculatedOrderIdFromBegin(orderEditBegin);

  const orderEditCommit = await capture(editCommitDocument, { id: calculatedOrderId });
  assertUserError(
    'orderEditCommit',
    orderEditCommit,
    'orderEditCommit',
    ['id'],
    'There must be at least one change to be made.',
  );

  const markAsPaid = await capture(markAsPaidDocument, { input: { id: orderId } });
  assertEmptyUserErrors('orderMarkAsPaid', markAsPaid, 'orderMarkAsPaid');
  const saleTransactionId = transactionIdFromMarkAsPaid(markAsPaid);

  const refund = await capture(refundDocument, {
    input: {
      orderId,
      currency: 'USD',
      allowOverRefunding: true,
      transactions: [
        {
          amount: '5.00',
          gateway: 'manual',
          kind: 'REFUND',
          orderId,
          parentId: saleTransactionId,
        },
      ],
    },
  });
  assertEmptyUserErrors('refundCreate', refund, 'refundCreate');

  for (const createdOrderId of createdOrderIds) {
    cleanup.push(
      await capture(cancelDocument, {
        orderId: createdOrderId,
        reason: 'OTHER',
        notifyCustomer: false,
        restock: true,
      }),
    );
  }

  await writeJson(fixturePath, {
    capturedAt: new Date().toISOString(),
    scenarioId,
    source: 'live-shopify-admin-graphql',
    storeDomain,
    apiVersion,
    notes:
      'Live Shopify Admin GraphQL capture for MoneyBag presentment propagation through orderCreate, orderEditBegin, orderEditCommit, orderMarkAsPaid, and refundCreate on one disposable multi-currency order. The edit branch runs before payment/refund because Shopify rejects edit begin on a paid/refunded order. The test order is created through public Admin GraphQL and cancelled in cleanup; no proxy/local-runtime output is used as Shopify evidence.',
    orderCreate,
    orderEditBegin,
    orderEditCommit,
    markAsPaid,
    refund,
    cleanup,
    upstreamCalls: [],
  });

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        orderIds: createdOrderIds,
        refundTotal: rootPayload(refund, 'refundCreate')['refund'],
        orderEditCommit: rootPayload(orderEditCommit, 'orderEditCommit'),
      },
      null,
      2,
    ),
  );
} finally {
  for (const createdOrderId of createdOrderIds) {
    if (cleanup.some((entry) => JSON.stringify(entry.variables).includes(createdOrderId))) {
      continue;
    }
    try {
      await capture(cancelDocument, {
        orderId: createdOrderId,
        reason: 'OTHER',
        notifyCustomer: false,
        restock: true,
      });
    } catch (error) {
      console.error(`Cleanup failed for ${createdOrderId}: ${(error as Error).message}`);
    }
  }
}
