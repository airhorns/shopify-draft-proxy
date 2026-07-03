/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
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

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'orderMarkAsPaid-snapshot-staging.json');
const cleanupPath = path.join(fixtureDir, 'orderMarkAsPaid-snapshot-staging-cleanup.json');
const specPath = path.join('config', 'parity-specs', 'orders', 'orderMarkAsPaid-snapshot-staging.json');
const createRequestPath = path.join(
  'config',
  'parity-requests',
  'orders',
  'orderMarkAsPaid-snapshot-staging-create.graphql',
);
const markRequestPath = path.join(
  'config',
  'parity-requests',
  'orders',
  'orderMarkAsPaid-snapshot-staging-mark.graphql',
);
const moneyRequestPath = path.join('config', 'parity-requests', 'orders', 'orderMarkAsPaid-state-and-money.graphql');
const readRequestPath = path.join(
  'config',
  'parity-requests',
  'orders',
  'orderMarkAsPaid-snapshot-staging-read.graphql',
);

const orderCreateDocument = `#graphql
  mutation OrderMarkAsPaidSnapshotStagingCreate(
    $order: OrderCreateOrderInput!
    $options: OrderCreateOptionsInput
  ) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        email
        displayFinancialStatus
        presentmentCurrencyCode
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderMarkAsPaidNoMoneyDocument = `#graphql
  mutation OrderMarkAsPaidSnapshotStagingMark($input: OrderMarkAsPaidInput!) {
    orderMarkAsPaid(input: $input) {
      order {
        id
        displayFinancialStatus
        paymentGatewayNames
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderMarkAsPaidMoneyDocument = `#graphql
  mutation OrderMarkAsPaidStateAndMoney($input: OrderMarkAsPaidInput!) {
    orderMarkAsPaid(input: $input) {
      order {
        id
        presentmentCurrencyCode
        displayFinancialStatus
        paymentGatewayNames
        totalOutstandingSet {
          shopMoney {
            amount
            currencyCode
          }
          presentmentMoney {
            amount
            currencyCode
          }
        }
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
            presentmentMoney {
              amount
              currencyCode
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

const downstreamReadDocument = `#graphql
  query OrderMarkAsPaidSnapshotStagingRead($id: ID!) {
    order(id: $id) {
      id
      displayFinancialStatus
      paymentGatewayNames
      totalOutstandingSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
      totalReceivedSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
      netPaymentSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
      transactions {
        id
        kind
        status
        gateway
        amountSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
      }
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation OrderMarkAsPaidSnapshotStagingCleanup(
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

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${payload.trim()}\n`, 'utf8');
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function capture(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  const response = await runGraphqlRequest<JsonRecord>(trimGraphql(query), variables);
  assertNoTopLevelErrors(response, context);
  return { query: trimGraphql(query), variables, response };
}

function orderCreatePayload(step: CaptureStep): JsonRecord {
  const payload = readRecord(step.response.payload.data, 'orderCreate');
  if (!payload) throw new Error(`orderCreate response is missing payload: ${JSON.stringify(step.response.payload)}`);
  return payload;
}

function orderMarkAsPaidPayload(step: CaptureStep): JsonRecord {
  const payload = readRecord(step.response.payload.data, 'orderMarkAsPaid');
  if (!payload)
    throw new Error(`orderMarkAsPaid response is missing payload: ${JSON.stringify(step.response.payload)}`);
  return payload;
}

function orderFromCreate(step: CaptureStep): JsonRecord {
  const payload = orderCreatePayload(step);
  const order = readRecord(payload, 'order');
  const userErrors = readArray(payload, 'userErrors');
  if (!order || userErrors.length > 0) {
    throw new Error(`orderCreate did not produce a usable order: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return order;
}

function orderIdFromCreate(step: CaptureStep): string {
  const id = readString(orderFromCreate(step), 'id');
  if (!id) throw new Error(`Created order missing id: ${JSON.stringify(step.response.payload, null, 2)}`);
  return id;
}

function userErrorMessages(step: CaptureStep): unknown[] {
  return readArray(orderMarkAsPaidPayload(step), 'userErrors');
}

function createVariables(stamp: number, scenario: string, paid = false): JsonRecord {
  const priceSet = {
    shopMoney: { amount: '12.50', currencyCode: 'CAD' },
    presentmentMoney: { amount: '12.50', currencyCode: 'CAD' },
  };
  const order: JsonRecord = {
    email: `hermes-mark-paid-snapshot-${scenario}-${stamp}@example.com`,
    note: `orderMarkAsPaid snapshot staging ${scenario}`,
    tags: ['parity-probe', 'order-mark-as-paid-snapshot', scenario],
    test: true,
    currency: 'CAD',
    presentmentCurrency: 'CAD',
    financialStatus: paid ? 'PAID' : 'PENDING',
    lineItems: [
      {
        title: `Hermes mark-as-paid snapshot ${scenario}`,
        quantity: 1,
        priceSet,
        requiresShipping: false,
        taxable: false,
        sku: `hermes-mark-paid-snapshot-${scenario}-${stamp}`,
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
  return { order, options: null };
}

async function cleanupOrder(orderId: string): Promise<CaptureStep> {
  return capture(
    orderCancelDocument,
    { orderId, reason: 'OTHER', notifyCustomer: false, restock: false },
    `cleanup ${orderId}`,
  );
}

const stamp = Date.now();
const createdOrderIds: string[] = [];
const cleanup: CaptureStep[] = [];

try {
  const markableCreate = await capture(orderCreateDocument, createVariables(stamp, 'markable'), 'markable orderCreate');
  const markableOrderId = orderIdFromCreate(markableCreate);
  createdOrderIds.push(markableOrderId);
  const markNoMoney = await capture(
    orderMarkAsPaidNoMoneyDocument,
    { input: { id: markableOrderId } },
    'markable orderMarkAsPaid no-money',
  );
  const readAfterMark = await capture(downstreamReadDocument, { id: markableOrderId }, 'read after mark-as-paid');
  const repeatMark = await capture(
    orderMarkAsPaidMoneyDocument,
    { input: { id: markableOrderId } },
    'already-paid orderMarkAsPaid',
  );

  const alreadyPaidCreate = await capture(
    orderCreateDocument,
    createVariables(stamp, 'already-paid', true),
    'already-paid orderCreate',
  );
  const alreadyPaidOrderId = orderIdFromCreate(alreadyPaidCreate);
  createdOrderIds.push(alreadyPaidOrderId);
  const alreadyPaidInitial = await capture(
    orderMarkAsPaidMoneyDocument,
    { input: { id: alreadyPaidOrderId } },
    'initially already-paid orderMarkAsPaid',
  );

  const unknownMark = await capture(
    orderMarkAsPaidMoneyDocument,
    { input: { id: 'gid://shopify/Order/999999999999999' } },
    'unknown orderMarkAsPaid',
  );

  await writeJson(fixturePath, {
    scenarioId: 'orderMarkAsPaid-snapshot-staging',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    operations: {
      markableCreate,
      markNoMoney,
      readAfterMark,
      repeatMark,
      alreadyPaidCreate,
      alreadyPaidInitial,
      unknownMark,
    },
    cleanup,
    upstreamCalls: [],
  });

  await writeText(createRequestPath, orderCreateDocument);
  await writeText(markRequestPath, orderMarkAsPaidNoMoneyDocument);
  await writeText(readRequestPath, downstreamReadDocument);

  await writeJson(specPath, {
    scenarioId: 'orderMarkAsPaid-snapshot-staging',
    operationNames: ['orderCreate', 'orderMarkAsPaid', 'order'],
    scenarioStatus: 'captured',
    assertionKinds: ['runtime-staging', 'payload-shape', 'state-validation', 'downstream-read-parity'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.operations.markableCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'markable-order-create-baseline',
          capturePath: '$.operations.markableCreate.response.payload.data.orderCreate',
          proxyPath: '$.data.orderCreate',
          selectedPaths: [
            '$.order.email',
            '$.order.displayFinancialStatus',
            '$.order.presentmentCurrencyCode',
            '$.userErrors',
          ],
        },
        {
          name: 'mark-as-paid-without-money-selection',
          capturePath: '$.operations.markNoMoney.response.payload.data.orderMarkAsPaid',
          proxyPath: '$.data.orderMarkAsPaid',
          selectedPaths: ['$.order.displayFinancialStatus', '$.order.paymentGatewayNames', '$.userErrors'],
          proxyRequest: {
            documentPath: markRequestPath,
            variables: { input: { id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' } } },
            apiVersion,
          },
        },
        {
          name: 'downstream-read-after-mark-as-paid',
          capturePath: '$.operations.readAfterMark.response.payload.data.order',
          proxyPath: '$.data.order',
          selectedPaths: [
            '$.displayFinancialStatus',
            '$.paymentGatewayNames',
            '$.totalOutstandingSet',
            '$.totalReceivedSet',
            '$.netPaymentSet',
            '$.transactions',
          ],
          expectedDifferences: [
            {
              path: '$.transactions[0].id',
              matcher: 'shopify-gid:OrderTransaction',
              reason: 'Live Shopify and the proxy allocate different order transaction ids.',
            },
          ],
          proxyRequest: {
            documentPath: readRequestPath,
            variables: { id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' } },
            apiVersion,
          },
        },
        {
          name: 'repeat-mark-as-paid-rejected',
          capturePath: '$.operations.repeatMark.response.payload.data.orderMarkAsPaid',
          proxyPath: '$.data.orderMarkAsPaid',
          selectedPaths: [
            '$.order.displayFinancialStatus',
            '$.order.totalOutstandingSet',
            '$.order.transactions',
            '$.userErrors',
          ],
          expectedDifferences: [
            {
              path: '$.order.transactions[0].id',
              matcher: 'shopify-gid:OrderTransaction',
              reason: 'Live Shopify and the proxy allocate different order transaction ids.',
            },
          ],
          proxyRequest: {
            documentPath: moneyRequestPath,
            variables: { input: { id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' } } },
            apiVersion,
          },
        },
        {
          name: 'already-paid-order-create-baseline',
          capturePath: '$.operations.alreadyPaidCreate.response.payload.data.orderCreate',
          proxyPath: '$.data.orderCreate',
          selectedPaths: ['$.order.email', '$.order.displayFinancialStatus', '$.userErrors'],
          proxyRequest: {
            documentPath: createRequestPath,
            variablesCapturePath: '$.operations.alreadyPaidCreate.variables',
            apiVersion,
          },
        },
        {
          name: 'initially-already-paid-rejected',
          capturePath: '$.operations.alreadyPaidInitial.response.payload.data.orderMarkAsPaid',
          proxyPath: '$.data.orderMarkAsPaid',
          selectedPaths: ['$.userErrors'],
          proxyRequest: {
            documentPath: moneyRequestPath,
            variables: {
              input: {
                id: {
                  fromProxyResponse: 'already-paid-order-create-baseline',
                  path: '$.data.orderCreate.order.id',
                },
              },
            },
            apiVersion,
          },
        },
        {
          name: 'unknown-order-not-found',
          capturePath: '$.operations.unknownMark.response.payload.data.orderMarkAsPaid',
          proxyPath: '$.data.orderMarkAsPaid',
          proxyRequest: {
            documentPath: moneyRequestPath,
            variablesCapturePath: '$.operations.unknownMark.variables',
            apiVersion,
          },
        },
      ],
    },
    notes:
      'Live 2026-04 capture for snapshot-mode orderCreate -> orderMarkAsPaid -> order(id:) staging, including a no-money-bag mutation selection, repeated/already-paid rejection, and unknown-order not-found. The proxy replay creates orders only through public GraphQL mutations, stages mark-as-paid locally, and compares payload/read-after-write effects without runtime Shopify writes.',
  });

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        specPath,
        markableOrderId,
        alreadyPaidOrderId,
        repeatUserErrors: userErrorMessages(repeatMark),
        unknownUserErrors: userErrorMessages(unknownMark),
      },
      null,
      2,
    ),
  );
} finally {
  for (const orderId of createdOrderIds) {
    try {
      cleanup.push(await cleanupOrder(orderId));
    } catch (error) {
      cleanup.push({
        query: trimGraphql(orderCancelDocument),
        variables: { orderId, reason: 'OTHER', notifyCustomer: false, restock: false },
        response: {
          status: 0,
          payload: { errors: [{ message: error instanceof Error ? error.message : String(error) }] },
        },
      });
    }
  }
  if (createdOrderIds.length > 0) {
    await writeJson(cleanupPath, {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cleanup,
    });
  }
}
