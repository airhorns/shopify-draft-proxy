/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
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
  response: ConformanceGraphqlResult<JsonRecord>['payload'];
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

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const fixturePath = path.join(fixtureDir, 'order-capture-validation.json');
const specPath = path.join('config', 'parity-specs', 'payments', 'order_capture_validation.json');
const createRequestPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'order-capture-validation-order-create.graphql',
);
const captureRequestPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'order-capture-validation-order-capture.graphql',
);

const orderCreateDocument = `
mutation OrderCaptureValidationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
  orderCreate(order: $order, options: $options) {
    order {
      id
      name
      presentmentCurrencyCode
      displayFinancialStatus
      capturable
      totalCapturable
      totalCapturableSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
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
      paymentGatewayNames
      transactions {
        id
        kind
        status
        gateway
        amountSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
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
`.trim();

const orderCaptureDocument = `
mutation OrderCaptureValidation($input: OrderCaptureInput!) {
  orderCapture(input: $input) {
    transaction {
      id
      kind
      status
      gateway
      amountSet {
        shopMoney { amount currencyCode }
        presentmentMoney { amount currencyCode }
      }
      parentTransaction {
        id
        kind
        status
      }
    }
    userErrors {
      field
      message
    }
  }
}
`.trim();

const orderCancelDocument = `
mutation OrderCaptureValidationCleanup(
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
`.trim();

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

function orderFromCreate(step: CaptureStep): JsonRecord {
  const orderCreate = readRecord(step.response.data, 'orderCreate');
  const errors = readArray(orderCreate, 'userErrors');
  const order = readRecord(orderCreate, 'order');
  if (!order || errors.length > 0) {
    throw new Error(`orderCreate did not produce a usable order: ${JSON.stringify(step.response, null, 2)}`);
  }
  return order;
}

function orderIdFromCreate(step: CaptureStep): string {
  const id = readString(orderFromCreate(step), 'id');
  if (!id) {
    throw new Error(`orderCreate did not return an order id: ${JSON.stringify(step.response, null, 2)}`);
  }
  return id;
}

function authorizationIdFromCreate(step: CaptureStep): string {
  const transactions = readArray(orderFromCreate(step), 'transactions');
  const id = readString(asRecord(transactions[0]), 'id');
  if (!id) {
    throw new Error(`orderCreate did not return an authorization id: ${JSON.stringify(step.response, null, 2)}`);
  }
  return id;
}

function assertMultiCurrency(step: CaptureStep): void {
  const order = orderFromCreate(step);
  const presentmentCurrencyCode = readString(order, 'presentmentCurrencyCode');
  const shopCurrencyCode = readString(readRecord(readRecord(order, 'totalCapturableSet'), 'shopMoney'), 'currencyCode');
  if (!presentmentCurrencyCode || !shopCurrencyCode || presentmentCurrencyCode === shopCurrencyCode) {
    throw new Error(`Expected multi-currency orderCreate result: ${JSON.stringify(order, null, 2)}`);
  }
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function runCapture(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  const response = await runGraphqlRequest<JsonRecord>(query, variables);
  assertNoTopLevelErrors(response, context);
  return { query, variables, response: response.payload };
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${payload.trim()}\n`, 'utf8');
}

function moneyBag(
  shopAmount: string,
  shopCurrency: string,
  presentmentAmount: string,
  presentmentCurrency: string,
): JsonRecord {
  return {
    shopMoney: {
      amount: shopAmount,
      currencyCode: shopCurrency,
    },
    presentmentMoney: {
      amount: presentmentAmount,
      currencyCode: presentmentCurrency,
    },
  };
}

function authorizationOrderVariables(stamp: number): JsonRecord {
  const priceSet = moneyBag('16.99', 'CAD', '12.50', 'USD');
  return {
    order: {
      email: `order-capture-validation-${stamp}@example.com`,
      note: `orderCapture validation live capture ${stamp}`,
      tags: ['order-capture-validation', 'conformance'],
      test: true,
      currency: 'CAD',
      presentmentCurrency: 'USD',
      lineItems: [
        {
          title: `Order capture validation item ${stamp}`,
          quantity: 1,
          priceSet,
          requiresShipping: false,
          taxable: false,
          sku: `order-capture-validation-${stamp}`,
        },
      ],
      transactions: [
        {
          kind: 'AUTHORIZATION',
          status: 'SUCCESS',
          gateway: 'manual',
          test: true,
          amountSet: priceSet,
        },
      ],
    },
    options: null,
  };
}

function captureInput(
  orderId: string,
  parentTransactionId: string,
  amount: string,
  options: { currency?: string; finalCapture?: boolean } = {},
): JsonRecord {
  return {
    input: {
      id: orderId,
      parentTransactionId,
      amount,
      ...(options.currency ? { currency: options.currency } : {}),
      ...(options.finalCapture === undefined ? {} : { finalCapture: options.finalCapture }),
    },
  };
}

async function cleanupOrder(orderId: string): Promise<unknown> {
  const variables = {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  };
  const result = await runGraphqlRequest<JsonRecord>(orderCancelDocument, variables);
  return {
    query: orderCancelDocument,
    variables,
    response: result.payload,
  };
}

const stamp = Date.now();
const cleanup: Record<string, unknown> = {};
const orderIds: string[] = [];

try {
  const createAuthorizationOrder = await runCapture(
    orderCreateDocument,
    authorizationOrderVariables(stamp),
    'multi-currency authorization orderCreate',
  );
  assertMultiCurrency(createAuthorizationOrder);
  const orderId = orderIdFromCreate(createAuthorizationOrder);
  const authorizationId = authorizationIdFromCreate(createAuthorizationOrder);
  orderIds.push(orderId);

  const currencyRequired = await runCapture(
    orderCaptureDocument,
    captureInput(orderId, authorizationId, '5.00'),
    'orderCapture currency required',
  );
  const currencyMismatch = await runCapture(
    orderCaptureDocument,
    captureInput(orderId, authorizationId, '5.00', { currency: 'CAD' }),
    'orderCapture currency mismatch',
  );
  const parentNotFound = await runCapture(
    orderCaptureDocument,
    captureInput(orderId, 'gid://shopify/OrderTransaction/999999999999999999', '5.00', { currency: 'USD' }),
    'orderCapture parent not found',
  );
  const invalidAmount = await runCapture(
    orderCaptureDocument,
    captureInput(orderId, authorizationId, '0.00', { currency: 'USD' }),
    'orderCapture invalid amount',
  );
  const overCapture = await runCapture(
    orderCaptureDocument,
    captureInput(orderId, authorizationId, '99.99', { currency: 'USD' }),
    'orderCapture over capture',
  );
  const finalCapture = await runCapture(
    orderCaptureDocument,
    captureInput(orderId, authorizationId, '5.00', { currency: 'USD', finalCapture: true }),
    'orderCapture final capture',
  );
  const postFinalCapture = await runCapture(
    orderCaptureDocument,
    captureInput(orderId, authorizationId, '1.00', { currency: 'USD' }),
    'orderCapture post-final capture',
  );

  for (const cleanupOrderId of [...orderIds].reverse()) {
    cleanup[cleanupOrderId] = await cleanupOrder(cleanupOrderId);
  }

  await writeText(createRequestPath, orderCreateDocument);
  await writeText(captureRequestPath, orderCaptureDocument);
  await writeJson(fixturePath, {
    scenarioId: 'order_capture_validation',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    operations: {
      createAuthorizationOrder,
      currencyRequired,
      currencyMismatch,
      parentNotFound,
      invalidAmount,
      overCapture,
      finalCapture,
      postFinalCapture,
    },
    cleanup,
    upstreamCalls: [],
  });
  await writeJson(specPath, {
    scenarioId: 'order_capture_validation',
    operationNames: ['orderCreate', 'orderCapture'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'payment-transaction-validation', 'no-upstream-passthrough'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.operations.createAuthorizationOrder.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'multi-currency-authorization-order',
          capturePath: '$.operations.createAuthorizationOrder.response.data.orderCreate',
          proxyPath: '$.data.orderCreate',
          selectedPaths: ['$.order.transactions', '$.userErrors'],
          expectedDifferences: [
            {
              path: '$[0].id',
              matcher: 'shopify-gid:OrderTransaction',
              reason: 'The proxy generates a synthetic authorization transaction ID.',
            },
          ],
        },
        {
          name: 'currency-required-validation',
          capturePath: '$.operations.currencyRequired.response.data.orderCapture',
          proxyPath: '$.data.orderCapture',
          selectedPaths: ['$.transaction', '$.userErrors[0].field'],
          proxyRequest: {
            documentPath: captureRequestPath,
            variables: {
              input: {
                id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
                parentTransactionId: {
                  fromPrimaryProxyPath: '$.data.orderCreate.order.transactions[0].id',
                },
                amount: '5.00',
              },
            },
            apiVersion,
          },
        },
        {
          name: 'currency-mismatch-validation',
          capturePath: '$.operations.currencyMismatch.response.data.orderCapture',
          proxyPath: '$.data.orderCapture',
          selectedPaths: ['$.transaction', '$.userErrors[0].field'],
          proxyRequest: {
            documentPath: captureRequestPath,
            variables: {
              input: {
                id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
                parentTransactionId: {
                  fromPrimaryProxyPath: '$.data.orderCreate.order.transactions[0].id',
                },
                amount: '5.00',
                currency: 'CAD',
              },
            },
            apiVersion,
          },
        },
        {
          name: 'parent-transaction-not-found-validation',
          capturePath: '$.operations.parentNotFound.response.data.orderCapture',
          proxyPath: '$.data.orderCapture',
          selectedPaths: ['$.transaction'],
          proxyRequest: {
            documentPath: captureRequestPath,
            variablesCapturePath: '$.operations.parentNotFound.variables',
            apiVersion,
          },
        },
        {
          name: 'invalid-amount-validation',
          capturePath: '$.operations.invalidAmount.response.data.orderCapture',
          proxyPath: '$.data.orderCapture',
          selectedPaths: ['$.transaction'],
          proxyRequest: {
            documentPath: captureRequestPath,
            variables: {
              input: {
                id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
                parentTransactionId: {
                  fromPrimaryProxyPath: '$.data.orderCreate.order.transactions[0].id',
                },
                amount: '0.00',
                currency: 'USD',
              },
            },
            apiVersion,
          },
        },
        {
          name: 'over-capture-validation',
          capturePath: '$.operations.overCapture.response.data.orderCapture',
          proxyPath: '$.data.orderCapture',
          selectedPaths: ['$.transaction'],
          proxyRequest: {
            documentPath: captureRequestPath,
            variables: {
              input: {
                id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
                parentTransactionId: {
                  fromPrimaryProxyPath: '$.data.orderCreate.order.transactions[0].id',
                },
                amount: '99.99',
                currency: 'USD',
              },
            },
            apiVersion,
          },
        },
        {
          name: 'capture-after-public-final-capture-rejection',
          capturePath: '$.operations.postFinalCapture.response.data.orderCapture',
          proxyPath: '$.data.orderCapture',
          selectedPaths: ['$.transaction.kind', '$.transaction.status', '$.transaction.gateway', '$.userErrors'],
          proxyRequest: {
            documentPath: captureRequestPath,
            variables: {
              input: {
                id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
                parentTransactionId: {
                  fromPrimaryProxyPath: '$.data.orderCreate.order.transactions[0].id',
                },
                amount: '1.00',
                currency: 'USD',
              },
            },
            apiVersion,
          },
        },
      ],
    },
    notes:
      'Live 2026-04 public Admin API capture for orderCapture validation against a disposable multi-currency authorization order. Public orderCapture.userErrors exposes field/message and this manual gateway returns public messages/field paths that differ from the internal OrderCaptureUserError code contract; focused runtime tests cover the internal code projection. The fixture records Shopify rejecting finalCapture: true for the manual gateway before a follow-up capture succeeds, so final-capture lock behavior remains runtime-test-backed until a live gateway that supports finalCapture is available.',
  });

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        specPath,
        requestPaths: [createRequestPath, captureRequestPath],
        orderId,
        authorizationId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  for (const cleanupOrderId of [...orderIds].reverse()) {
    if (cleanup[cleanupOrderId]) continue;
    try {
      cleanup[cleanupOrderId] = await cleanupOrder(cleanupOrderId);
    } catch (cleanupError) {
      cleanup[cleanupOrderId] = { error: cleanupError instanceof Error ? cleanupError.message : String(cleanupError) };
    }
  }
  if (Object.keys(cleanup).length > 0) {
    await writeJson(path.join(fixtureDir, 'order-capture-validation-cleanup-failure.json'), {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cleanup,
    });
  }
  throw error;
}
