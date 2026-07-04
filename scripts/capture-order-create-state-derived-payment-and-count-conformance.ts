/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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

const scenarioId = 'orderCreate-state-derived-payment-and-count';
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
const specPath = path.join('config', 'parity-specs', 'orders', 'orderCreate-state-derived-payment-and-count.json');
const createRequestPath = path.join(requestDir, 'orderCreate-state-derived-payment-and-count-create.graphql');
const countRequestPath = path.join(requestDir, 'orderCreate-state-derived-payment-and-count-count.graphql');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'orderCreate-state-derived-payment-and-count.json');

const orderCreateDocument = `#graphql
  mutation OrderCreateStateDerivedPaymentAndCountCreate(
    $order: OrderCreateOrderInput!
    $options: OrderCreateOptionsInput
  ) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        email
        tags
        displayFinancialStatus
        capturable
        totalCapturable
        totalCapturableSet {
          shopMoney {
            amount
            currencyCode
          }
        }
        totalOutstandingSet {
          shopMoney {
            amount
            currencyCode
          }
        }
        totalReceivedSet {
          shopMoney {
            amount
            currencyCode
          }
        }
        netPaymentSet {
          shopMoney {
            amount
            currencyCode
          }
        }
        paymentGatewayNames
        transactions {
          id
          kind
          status
          gateway
          amountSet {
            shopMoney {
              amount
              currencyCode
            }
          }
          parentTransaction {
            id
            kind
            status
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

const countReadDocument = `#graphql
  query OrderCreateStateDerivedPaymentAndCountRead($tagQuery: String!, $countLimit: Int!) {
    exactCount: ordersCount(query: $tagQuery) {
      count
      precision
    }
    limitedCount: ordersCount(query: $tagQuery, limit: $countLimit) {
      count
      precision
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation OrderCreateStateDerivedPaymentAndCountCleanup(
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

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${payload.trim()}\n`, 'utf8');
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

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function capture(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  const trimmed = trimGraphql(query);
  const response = await runGraphqlRequest<JsonRecord>(trimmed, variables);
  assertNoTopLevelErrors(response, context);
  return { query: trimmed, variables, response };
}

async function sleep(milliseconds: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

function orderCreatePayload(step: CaptureStep): JsonRecord {
  const payload = readRecord(step.response.payload.data, 'orderCreate');
  if (!payload) {
    throw new Error(`orderCreate response is missing payload: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return payload;
}

function orderFromCreate(step: CaptureStep): JsonRecord {
  const payload = orderCreatePayload(step);
  const order = readRecord(payload, 'order');
  const userErrors = readArray(payload, 'userErrors');
  if (!order || userErrors.length > 0) {
    throw new Error(`orderCreate did not create an order: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return order;
}

function orderIdFromCreate(step: CaptureStep): string {
  const id = readString(orderFromCreate(step), 'id');
  if (!id) {
    throw new Error(`orderCreate did not return an order id: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return id;
}

function assertExternalAuthorization(step: CaptureStep, context: string): void {
  const order = orderFromCreate(step);
  const gateways = readArray(order, 'paymentGatewayNames');
  const transactions = readArray(order, 'transactions');
  const transaction = asRecord(transactions[0]);
  const amountSet = readRecord(transaction, 'amountSet');
  const shopMoney = readRecord(amountSet, 'shopMoney');
  if (
    order['displayFinancialStatus'] !== 'AUTHORIZED' ||
    order['capturable'] !== true ||
    !gateways.includes('external') ||
    transaction?.['kind'] !== 'AUTHORIZATION' ||
    transaction['status'] !== 'SUCCESS' ||
    transaction['gateway'] !== 'external' ||
    shopMoney?.['amount'] !== '31.9' ||
    shopMoney['currencyCode'] !== 'CAD'
  ) {
    throw new Error(
      `${context} did not expose external authorization payment state: ${JSON.stringify(order, null, 2)}`,
    );
  }
}

function assertCountRead(step: CaptureStep): void {
  const data = asRecord(step.response.payload.data);
  const exactCount = readRecord(data, 'exactCount');
  const limitedCount = readRecord(data, 'limitedCount');
  if (
    exactCount?.['count'] !== 2 ||
    exactCount['precision'] !== 'EXACT' ||
    limitedCount?.['count'] !== 1 ||
    limitedCount['precision'] !== 'AT_LEAST'
  ) {
    throw new Error(`ordersCount read did not return the expected two-order count: ${JSON.stringify(data, null, 2)}`);
  }
}

async function captureCountReadWithRetry(variables: JsonRecord): Promise<CaptureStep> {
  let latest: CaptureStep | null = null;
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    latest = await capture(countReadDocument, variables, `ordersCount read attempt ${attempt}`);
    try {
      assertCountRead(latest);
      if (attempt > 1) {
        console.log(`ordersCount indexed after ${attempt} attempts`);
      }
      return latest;
    } catch (error) {
      if (attempt === 12) {
        throw error;
      }
      await sleep(2_000);
    }
  }
  throw new Error(`ordersCount read retry exhausted: ${JSON.stringify(latest?.response.payload, null, 2)}`);
}

function orderVariables(stamp: string, index: number, tag: string): JsonRecord {
  return {
    order: {
      email: `har-1868-state-derived-${stamp}-${index}@example.com`,
      note: `HAR-1868 state-derived payment/count ${stamp} ${index}`,
      tags: ['har-1868-state-derived-payment-count', tag],
      test: true,
      currency: 'CAD',
      lineItems: [
        {
          title: `State-derived payment/count ${stamp} ${index}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '31.90',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: false,
          taxable: false,
          sku: `HAR-1868-${stamp}-${index}`,
        },
      ],
      transactions: [
        {
          kind: 'AUTHORIZATION',
          status: 'SUCCESS',
          gateway: 'external',
          test: true,
          amountSet: {
            shopMoney: {
              amount: '31.90',
              currencyCode: 'CAD',
            },
          },
        },
      ],
    },
    options: null,
  };
}

function specPayload(): JsonRecord {
  const paymentSelectedPaths = [
    '$.order.email',
    '$.order.displayFinancialStatus',
    '$.order.capturable',
    '$.order.totalCapturableSet',
    '$.order.totalOutstandingSet',
    '$.order.totalReceivedSet',
    '$.order.netPaymentSet',
    '$.order.paymentGatewayNames',
    '$.order.transactions[0].kind',
    '$.order.transactions[0].status',
    '$.order.transactions[0].gateway',
    '$.order.transactions[0].amountSet',
    '$.order.transactions[0].parentTransaction',
    '$.userErrors',
  ];

  return {
    scenarioId,
    operationNames: ['orderCreate', 'ordersCount'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'payment-transaction-parity', 'search-filtering', 'count-limit'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.operations.firstCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live 2025-01 Shopify capture for two disposable orderCreate mutations with external authorization transactions and a unique tag, followed by ordersCount exact/limit reads. The replay stages both orderCreate calls locally through public GraphQL, then proves the count is derived from staged state and the payment projection uses the submitted gateway/amount instead of fixed manual/$25 values.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'first-create-external-authorization-payment-state',
          capturePath: '$.operations.firstCreate.response.payload.data.orderCreate',
          proxyPath: '$.data.orderCreate',
          selectedPaths: paymentSelectedPaths,
        },
        {
          name: 'second-create-external-authorization-payment-state',
          capturePath: '$.operations.secondCreate.response.payload.data.orderCreate',
          proxyPath: '$.data.orderCreate',
          selectedPaths: paymentSelectedPaths,
          proxyRequest: {
            documentPath: createRequestPath,
            variablesCapturePath: '$.operations.secondCreate.variables',
            apiVersion,
          },
        },
        {
          name: 'staged-tagged-orders-count',
          capturePath: '$.operations.countRead.response.payload.data',
          proxyPath: '$.data',
          selectedPaths: ['$.exactCount', '$.limitedCount'],
          proxyRequest: {
            documentPath: countRequestPath,
            variablesCapturePath: '$.operations.countRead.variables',
            apiVersion,
          },
        },
      ],
    },
  };
}

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const tag = `har-1868-state-derived-${stamp}`;
const createdOrderIds: string[] = [];
const cleanup: CaptureStep[] = [];

await writeText(createRequestPath, trimGraphql(orderCreateDocument));
await writeText(countRequestPath, trimGraphql(countReadDocument));
await writeJson(specPath, specPayload());

try {
  const firstCreate = await capture(orderCreateDocument, orderVariables(stamp, 1, tag), 'first orderCreate');
  assertExternalAuthorization(firstCreate, 'first orderCreate');
  createdOrderIds.push(orderIdFromCreate(firstCreate));

  const secondCreate = await capture(orderCreateDocument, orderVariables(stamp, 2, tag), 'second orderCreate');
  assertExternalAuthorization(secondCreate, 'second orderCreate');
  createdOrderIds.push(orderIdFromCreate(secondCreate));

  const countRead = await captureCountReadWithRetry({
    tagQuery: `tag:${tag}`,
    countLimit: 1,
  });

  for (const orderId of createdOrderIds) {
    cleanup.push(
      await capture(
        orderCancelDocument,
        {
          orderId,
          reason: 'OTHER',
          notifyCustomer: false,
          restock: true,
        },
        `cleanup ${orderId}`,
      ),
    );
  }

  await writeJson(fixturePath, {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes:
      'Captured from live Shopify Admin GraphQL. Test orders are created with a unique tag, external authorization transactions, and public orderCreate/orderCount requests, then cancelled in cleanup.',
    operations: {
      firstCreate,
      secondCreate,
      countRead,
    },
    cleanup,
  });

  console.log(`Wrote ${fixturePath}`);
  console.log(`Wrote ${specPath}`);
  console.log(`Wrote ${createRequestPath}`);
  console.log(`Wrote ${countRequestPath}`);
} catch (error) {
  for (const orderId of createdOrderIds) {
    try {
      cleanup.push(
        await capture(
          orderCancelDocument,
          {
            orderId,
            reason: 'OTHER',
            notifyCustomer: false,
            restock: true,
          },
          `cleanup after failure ${orderId}`,
        ),
      );
    } catch (cleanupError) {
      console.error(`Failed to clean up ${orderId}:`, cleanupError);
    }
  }
  if (cleanup.length > 0) {
    await writeJson(path.join(fixtureDir, 'orderCreate-state-derived-payment-and-count-cleanup.json'), {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cleanup,
    });
  }
  throw error;
}
