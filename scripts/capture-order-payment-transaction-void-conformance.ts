/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
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

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const fixturePath = path.join(fixtureDir, 'order-payment-transaction-void-live.json');
const createRequestPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'order-payment-transaction-void-create.graphql',
);
const voidRequestPath = path.join('config', 'parity-requests', 'payments', 'order-payment-transaction-void.graphql');
const readRequestPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'order-payment-transaction-void-read.graphql',
);

async function readRequest(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function payloadRoot(capture: GraphqlCapture, rootName: string): JsonRecord {
  const payload = readRecord(capture.response.payload) ?? {};
  const data = readRecord(payload['data']) ?? {};
  return readRecord(data[rootName]) ?? {};
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) throw new Error(`Missing ${label}`);
  return value;
}

function requireNoTopLevelErrors(capture: GraphqlCapture, context: string): void {
  if (capture.response.status < 200 || capture.response.status >= 300 || capture.response.payload['errors']) {
    throw new Error(`${context} returned top-level errors: ${JSON.stringify(capture.response.payload, null, 2)}`);
  }
}

function requireEmptyUserErrors(capture: GraphqlCapture, rootName: string): void {
  requireNoTopLevelErrors(capture, rootName);
  const userErrors = readArray(payloadRoot(capture, rootName)['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${rootName} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

async function capture(query: string, variables: JsonRecord): Promise<GraphqlCapture> {
  return {
    query: query.trim(),
    variables,
    response: await runGraphqlRequest<JsonRecord>(query, variables),
  };
}

function orderVariables(stamp: number): JsonRecord {
  return {
    order: {
      email: `order-payment-void-${stamp}@example.com`,
      note: `order payment transaction void live capture ${stamp}`,
      tags: ['order-payment-transaction-void', 'shopify-draft-proxy', String(stamp)],
      test: true,
      currency: 'CAD',
      transactions: [
        {
          kind: 'AUTHORIZATION',
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
      lineItems: [
        {
          title: `Order payment void ${stamp}`,
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
    },
  };
}

const createDocument = await readRequest(createRequestPath);
const voidDocument = await readRequest(voidRequestPath);
const readDocument = await readRequest(readRequestPath);
const cleanupDocument = `
  mutation OrderPaymentTransactionVoidCleanup(
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

const stamp = Date.now();
const create = await capture(createDocument, orderVariables(stamp));
requireEmptyUserErrors(create, 'orderCreate');
const order = readRecord(payloadRoot(create, 'orderCreate')['order']) ?? {};
const orderId = requireString(order['id'], 'created order id');
const authorization = readRecord(readArray(order['transactions'])[0]) ?? {};
const authorizationId = requireString(authorization['id'], 'authorization transaction id');

const voidCapture = await capture(voidDocument, { id: authorizationId });
requireEmptyUserErrors(voidCapture, 'transactionVoid');
const readAfterVoid = await capture(readDocument, { id: orderId });
requireNoTopLevelErrors(readAfterVoid, 'read after transactionVoid');
const cleanup = await capture(cleanupDocument, {
  orderId,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: true,
});
requireNoTopLevelErrors(cleanup, 'order cleanup');

await writeJson(fixturePath, {
  scenarioId: 'order-payment-transaction-void-live',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  source: 'live-shopify-admin-graphql',
  notes:
    'Live public Admin API capture for successful transactionVoid against a disposable manual authorization order. Private OrderTransaction paymentReferenceId/paymentId projection remains runtime-test-backed because public Shopify rejects paymentReferenceId on OrderTransaction.',
  voidFlow: {
    create,
    void: voidCapture,
    readAfterVoid,
  },
  cleanup,
  upstreamCalls: [],
});

console.log(JSON.stringify({ fixturePath, orderId, authorizationId }, null, 2));
