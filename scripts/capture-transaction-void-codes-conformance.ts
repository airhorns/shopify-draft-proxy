/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
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
const fixturePath = path.join(fixtureDir, 'transaction-void-codes.json');
const specPath = path.join('config', 'parity-specs', 'payments', 'transaction_void_codes.json');
const createRequestPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'transaction-void-codes-order-create.graphql',
);
const captureRequestPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'transaction-void-codes-order-capture.graphql',
);
const voidRequestPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'transaction-void-codes-transaction-void.graphql',
);

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

async function readRequest(filePath: string): Promise<string> {
  return (await readFile(filePath, 'utf8')).trim();
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function requireNoUserErrors(payload: unknown, pathName: string, context: string): void {
  const errors = readArray(payload, pathName);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function requireOrderId(step: CaptureStep, context: string): string {
  const order = readRecord(readRecord(step.response.data, 'orderCreate'), 'order');
  const id = readString(order, 'id');
  if (!id) {
    throw new Error(`${context} did not return an order id: ${JSON.stringify(step.response)}`);
  }
  return id;
}

function requireFirstTransactionId(step: CaptureStep, context: string): string {
  const order = readRecord(readRecord(step.response.data, 'orderCreate'), 'order');
  const transactions = readArray(order, 'transactions');
  const id = readString(asRecord(transactions[0]), 'id');
  if (!id) {
    throw new Error(`${context} did not return a transaction id: ${JSON.stringify(step.response)}`);
  }
  return id;
}

async function runCapture(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  const response = await runGraphqlRequest<JsonRecord>(query, variables);
  assertNoTopLevelErrors(response, context);
  return { query, variables, response: response.payload };
}

function orderVariables(stamp: number, label: string, kind: 'AUTHORIZATION' | 'CAPTURE'): JsonRecord {
  return {
    order: {
      email: `transaction-void-codes-${label}-${stamp}@example.com`,
      note: `transactionVoid code parity ${label}`,
      test: true,
      lineItems: [
        {
          title: `transactionVoid code parity ${label}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '25.00',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: false,
          taxable: false,
        },
      ],
      transactions: [
        {
          kind,
          status: 'SUCCESS',
          gateway: 'manual',
          test: true,
          amountSet: {
            shopMoney: {
              amount: '25.00',
              currencyCode: 'CAD',
            },
          },
        },
      ],
    },
    options: null,
  };
}

async function main(): Promise<void> {
  const createDocument = await readRequest(createRequestPath);
  const captureDocument = await readRequest(captureRequestPath);
  const voidDocument = await readRequest(voidRequestPath);
  const cancelDocument = `
    mutation TransactionVoidCodesOrderCleanup(
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

  const stamp = Date.now();
  const cleanup: Record<string, unknown> = {};
  const orderIds: string[] = [];

  try {
    const createCaptureOrder = await runCapture(
      createDocument,
      orderVariables(stamp, 'capture-transaction', 'CAPTURE'),
      'capture transaction orderCreate',
    );
    requireNoUserErrors(
      readRecord(createCaptureOrder.response.data, 'orderCreate'),
      'userErrors',
      'capture orderCreate',
    );
    const captureOrderId = requireOrderId(createCaptureOrder, 'capture orderCreate');
    const captureTransactionId = requireFirstTransactionId(createCaptureOrder, 'capture orderCreate');
    orderIds.push(captureOrderId);

    const voidCapture = await runCapture(
      voidDocument,
      { id: captureTransactionId },
      'transactionVoid capture transaction',
    );

    const voidMissing = await runCapture(
      voidDocument,
      { id: 'gid://shopify/OrderTransaction/999999999999999999' },
      'transactionVoid missing transaction',
    );

    const createAuthorizationOrder = await runCapture(
      createDocument,
      orderVariables(stamp, 'captured-authorization', 'AUTHORIZATION'),
      'authorization orderCreate',
    );
    requireNoUserErrors(
      readRecord(createAuthorizationOrder.response.data, 'orderCreate'),
      'userErrors',
      'authorization orderCreate',
    );
    const authorizationOrderId = requireOrderId(createAuthorizationOrder, 'authorization orderCreate');
    const authorizationTransactionId = requireFirstTransactionId(createAuthorizationOrder, 'authorization orderCreate');
    orderIds.push(authorizationOrderId);

    const captureAuthorization = await runCapture(
      captureDocument,
      {
        input: {
          id: authorizationOrderId,
          parentTransactionId: authorizationTransactionId,
          amount: '25.00',
          currency: 'CAD',
        },
      },
      'orderCapture authorization',
    );
    requireNoUserErrors(
      readRecord(captureAuthorization.response.data, 'orderCapture'),
      'userErrors',
      'orderCapture authorization',
    );

    const voidCapturedAuthorization = await runCapture(
      voidDocument,
      { id: authorizationTransactionId },
      'transactionVoid captured authorization',
    );

    for (const orderId of [...orderIds].reverse()) {
      const result = await runGraphqlRequest<JsonRecord>(cancelDocument, {
        orderId,
        reason: 'OTHER',
        notifyCustomer: false,
        restock: true,
      });
      cleanup[orderId] = result.payload;
      assertNoTopLevelErrors(result, `cleanup ${orderId}`);
    }

    await writeJson(fixturePath, {
      scenarioId: 'transaction_void_codes',
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      operations: {
        createCaptureOrder,
        voidCapture,
        voidMissing,
        createAuthorizationOrder,
        captureAuthorization,
        voidCapturedAuthorization,
      },
      upstreamCalls: [],
      cleanup,
    });

    await writeJson(specPath, {
      scenarioId: 'transaction_void_codes',
      operationNames: ['orderCreate', 'orderCapture', 'transactionVoid'],
      scenarioStatus: 'captured',
      assertionKinds: ['user-errors-parity', 'payment-transaction-validation', 'no-upstream-passthrough'],
      liveCaptureFiles: [fixturePath],
      proxyRequest: {
        documentPath: createRequestPath,
        variablesCapturePath: '$.operations.createCaptureOrder.variables',
        apiVersion,
      },
      comparisonMode: 'captured-vs-proxy-request',
      comparison: {
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [
          {
            name: 'auth-not-successful-error-code',
            capturePath: '$.operations.voidCapture.response.data.transactionVoid',
            proxyPath: '$.data.transactionVoid',
            proxyRequest: {
              documentPath: voidRequestPath,
              variables: {
                id: {
                  fromPrimaryProxyPath: '$.data.orderCreate.order.transactions[0].id',
                },
              },
              apiVersion,
            },
          },
          {
            name: 'transaction-not-found-error-code',
            capturePath: '$.operations.voidMissing.response.data.transactionVoid',
            proxyPath: '$.data.transactionVoid',
            proxyRequest: {
              documentPath: voidRequestPath,
              variablesCapturePath: '$.operations.voidMissing.variables',
              apiVersion,
            },
          },
          {
            name: 'create-authorization-order',
            capturePath: '$.operations.createAuthorizationOrder.response.data.orderCreate.userErrors',
            proxyPath: '$.data.orderCreate.userErrors',
            proxyRequest: {
              documentPath: createRequestPath,
              variablesCapturePath: '$.operations.createAuthorizationOrder.variables',
              apiVersion,
            },
          },
          {
            name: 'capture-authorization',
            capturePath: '$.operations.captureAuthorization.response.data.orderCapture.userErrors',
            proxyPath: '$.data.orderCapture.userErrors',
            proxyRequest: {
              documentPath: captureRequestPath,
              variables: {
                input: {
                  id: {
                    fromProxyResponse: 'create-authorization-order',
                    path: '$.data.orderCreate.order.id',
                  },
                  parentTransactionId: {
                    fromProxyResponse: 'create-authorization-order',
                    path: '$.data.orderCreate.order.transactions[0].id',
                  },
                  amount: '25.00',
                  currency: 'CAD',
                },
              },
              apiVersion,
            },
          },
          {
            name: 'auth-not-voidable-error-code',
            capturePath: '$.operations.voidCapturedAuthorization.response.data.transactionVoid',
            proxyPath: '$.data.transactionVoid',
            proxyRequest: {
              documentPath: voidRequestPath,
              variables: {
                id: {
                  fromProxyResponse: 'create-authorization-order',
                  path: '$.data.orderCreate.order.transactions[0].id',
                },
              },
              apiVersion,
            },
          },
        ],
      },
      notes:
        'Live public Admin API capture for transactionVoid validation codes. Shopify 2025-01 and 2026-04 return field ["parentTransactionId"] for these public GraphQL userErrors; this spec records that public shape alongside TRANSACTION_NOT_FOUND, AUTH_NOT_SUCCESSFUL, and AUTH_NOT_VOIDABLE.',
    });

    console.log(`Wrote ${fixturePath}`);
    console.log(`Wrote ${specPath}`);
  } catch (error) {
    for (const orderId of [...orderIds].reverse()) {
      if (cleanup[orderId]) continue;
      try {
        cleanup[orderId] = (
          await runGraphqlRequest<JsonRecord>(cancelDocument, {
            orderId,
            reason: 'OTHER',
            notifyCustomer: false,
            restock: true,
          })
        ).payload;
      } catch (cleanupError) {
        cleanup[orderId] = { error: String(cleanupError) };
      }
    }
    throw error;
  }
}

await main();
