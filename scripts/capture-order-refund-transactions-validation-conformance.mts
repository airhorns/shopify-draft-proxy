/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlResult = {
  status: number;
  payload: JsonRecord;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphqlRequest: (query: string, variables?: Record<string, unknown>) => Promise<GraphqlResult>;
};

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'refund-create-transactions-validation.json');

const orderCreateMutation = `#graphql
  mutation RefundTransactionsValidationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        createdAt
        updatedAt
        displayFinancialStatus
        displayFulfillmentStatus
        presentmentCurrencyCode
        paymentGatewayNames
        totalOutstandingSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
        totalReceivedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
        totalRefundedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
        currentTotalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
        transactions {
          id
          kind
          status
          gateway
          amountSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
        }
        refunds {
          id
          note
          totalRefundedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
          transactions(first: 10) {
            nodes {
              id
              kind
              status
              gateway
              amountSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
            }
          }
        }
        lineItems(first: 10) {
          nodes {
            id
            title
            name
            quantity
            currentQuantity
            sku
            originalUnitPriceSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
            originalTotalSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const refundCreateTransactionsValidationMutation = `#graphql
  mutation RefundCreateTransactionsValidation($input: RefundInput!, $idempotencyKey: String!) {
    refundCreate(input: $input) @idempotent(key: $idempotencyKey) {
      refund {
        id
        totalRefundedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
        transactions(first: 5) {
          nodes {
            id
            kind
            status
            gateway
            amountSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
          }
        }
      }
      order {
        id
        displayFinancialStatus
        totalRefundedSet { shopMoney { amount currencyCode } presentmentMoney { amount currencyCode } }
      }
      userErrors { field message }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation RefundTransactionsValidationOrderCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job { id done }
      orderCancelUserErrors { field message code }
      userErrors { field message }
    }
  }
`;

function asRecord(value: unknown): JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : {};
}

function readPath(value: unknown, pathParts: string[]): unknown {
  return pathParts.reduce<unknown>((current, key) => asRecord(current)[key], value);
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

function assertHttpOk(label: string, result: GraphqlResult): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} returned HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertNoGraphqlErrors(label: string, result: GraphqlResult): void {
  assertHttpOk(label, result);
  if (Array.isArray(result.payload['errors']) && result.payload['errors'].length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(result.payload['errors'], null, 2)}`);
  }
}

function assertRefundUserError(result: GraphqlResult, label: string): void {
  assertNoGraphqlErrors(`refundCreate ${label}`, result);
  const userErrors = readArray(readPath(result.payload, ['data', 'refundCreate', 'userErrors']));
  const first = asRecord(userErrors[0]);
  if (typeof first['message'] !== 'string' || readPath(result.payload, ['data', 'refundCreate', 'refund']) !== null) {
    throw new Error(
      `Expected refundCreate validation failure for ${label}, got ${JSON.stringify(result.payload, null, 2)}`,
    );
  }
}

function assertRefundAccepted(result: GraphqlResult): void {
  assertNoGraphqlErrors('refundCreate happy path', result);
  const userErrors = readArray(readPath(result.payload, ['data', 'refundCreate', 'userErrors']));
  const refundId = readPath(result.payload, ['data', 'refundCreate', 'refund', 'id']);
  if (userErrors.length !== 0 || typeof refundId !== 'string') {
    throw new Error(`Expected refundCreate happy path, got ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function buildOrderVariables(stamp: number, label: string, gateway = 'manual'): JsonRecord {
  return {
    order: {
      email: `hermes-refund-transactions-${label}-${stamp}@example.com`,
      note: `refundCreate transactions validation ${label} seed order`,
      tags: ['parity-probe', 'refund-create', 'transactions-validation'],
      test: true,
      lineItems: [
        {
          title: `Hermes refundable transaction validation ${label} ${stamp}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: true,
          taxable: false,
          sku: `hermes-refund-transactions-${label}-${stamp}`,
        },
      ],
      transactions: [
        {
          kind: 'SALE',
          status: 'SUCCESS',
          gateway,
          test: true,
          amountSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'CAD',
            },
          },
        },
      ],
    },
    options: null,
  };
}

function refundVariables(
  orderId: string,
  parentId: string,
  kind: string,
  gateway: string,
  idempotencyKey: string,
): JsonRecord {
  return {
    idempotencyKey,
    input: {
      orderId,
      transactions: [
        {
          amount: '1.00',
          gateway,
          kind,
          orderId,
          parentId,
        },
      ],
    },
  };
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

async function cleanupOrder(orderId: string): Promise<GraphqlResult> {
  return runGraphqlRequest(orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
}

const stamp = Date.now();

const validationOrderVariables = buildOrderVariables(stamp, 'validation', 'bogus-parent-gateway');
const validationOrderCreate = await runGraphqlRequest(orderCreateMutation, validationOrderVariables);
assertNoGraphqlErrors('validation orderCreate setup', validationOrderCreate);
const validationOrder = asRecord(readPath(validationOrderCreate.payload, ['data', 'orderCreate', 'order']));
const validationOrderId = requireString(validationOrder['id'], 'validation order.id');
const validationSaleTransactionId = requireString(
  asRecord(readArray(validationOrder['transactions'])[0])['id'],
  'validation sale transaction.id',
);

const happyOrderVariables = buildOrderVariables(stamp, 'happy');
const happyOrderCreate = await runGraphqlRequest(orderCreateMutation, happyOrderVariables);
assertNoGraphqlErrors('happy orderCreate setup', happyOrderCreate);
const happyOrder = asRecord(readPath(happyOrderCreate.payload, ['data', 'orderCreate', 'order']));
const happyOrderId = requireString(happyOrder['id'], 'happy order.id');
const happySaleTransactionId = requireString(
  asRecord(readArray(happyOrder['transactions'])[0])['id'],
  'happy sale transaction.id',
);

const invalidKindVariables = refundVariables(
  validationOrderId,
  validationSaleTransactionId,
  'AUTHORIZATION',
  'bogus-parent-gateway',
  `refund-transactions-invalid-kind-${stamp}`,
);
const invalidKind = await runGraphqlRequest(refundCreateTransactionsValidationMutation, invalidKindVariables);
assertRefundUserError(invalidKind, 'invalid kind');

const missingParentVariables = refundVariables(
  validationOrderId,
  'gid://shopify/OrderTransaction/999999999999',
  'REFUND',
  'bogus-parent-gateway',
  `refund-transactions-missing-parent-${stamp}`,
);
const missingParent = await runGraphqlRequest(refundCreateTransactionsValidationMutation, missingParentVariables);
assertRefundUserError(missingParent, 'missing parent');

const mismatchedGatewayVariables = refundVariables(
  validationOrderId,
  validationSaleTransactionId,
  'REFUND',
  'manual',
  `refund-transactions-mismatched-gateway-${stamp}`,
);
const mismatchedGateway = await runGraphqlRequest(
  refundCreateTransactionsValidationMutation,
  mismatchedGatewayVariables,
);
assertRefundAccepted(mismatchedGateway);

const happyPathVariables = refundVariables(
  happyOrderId,
  happySaleTransactionId,
  'REFUND',
  'manual',
  `refund-transactions-happy-${stamp}`,
);
const happyPath = await runGraphqlRequest(refundCreateTransactionsValidationMutation, happyPathVariables);
assertRefundAccepted(happyPath);

let validationCleanup: GraphqlResult | { error: string };
try {
  validationCleanup = await cleanupOrder(validationOrderId);
} catch (error) {
  validationCleanup = { error: error instanceof Error ? error.message : String(error) };
}

let happyCleanup: GraphqlResult | { error: string };
try {
  happyCleanup = await cleanupOrder(happyOrderId);
} catch (error) {
  happyCleanup = { error: error instanceof Error ? error.message : String(error) };
}

await writeJson(fixturePath, {
  storeDomain,
  apiVersion,
  setup: {
    validationOrderCreate: {
      query: orderCreateMutation,
      variables: validationOrderVariables,
      response: validationOrderCreate.payload,
    },
    happyOrderCreate: {
      query: orderCreateMutation,
      variables: happyOrderVariables,
      response: happyOrderCreate.payload,
    },
  },
  invalidKind: {
    query: refundCreateTransactionsValidationMutation,
    variables: invalidKindVariables,
    response: invalidKind.payload,
  },
  missingParent: {
    query: refundCreateTransactionsValidationMutation,
    variables: missingParentVariables,
    response: missingParent.payload,
  },
  mismatchedGateway: {
    query: refundCreateTransactionsValidationMutation,
    variables: mismatchedGatewayVariables,
    response: mismatchedGateway.payload,
  },
  happyPath: {
    query: refundCreateTransactionsValidationMutation,
    variables: happyPathVariables,
    response: happyPath.payload,
  },
  cleanup: {
    validationOrderCancel: validationCleanup,
    happyOrderCancel: happyCleanup,
  },
  upstreamCalls: [
    {
      operationName: 'OrdersOrderHydrate',
      variables: { id: validationOrderId },
      query:
        'hand-synthesized from checked-in setup orderCreate response for refundCreate transaction validation branch hydration',
      response: {
        status: 200,
        body: {
          data: {
            order: validationOrder,
          },
        },
      },
    },
    {
      operationName: 'OrdersOrderHydrate',
      variables: { id: happyOrderId },
      query:
        'hand-synthesized from checked-in setup orderCreate response for refundCreate transaction happy path hydration',
      response: {
        status: 200,
        body: {
          data: {
            order: happyOrder,
          },
        },
      },
    },
  ],
});

console.log(
  JSON.stringify(
    {
      ok: true,
      fixturePath,
      storeDomain,
      apiVersion,
      validationOrderId,
      happyOrderId,
      invalidKindUserErrors: readPath(invalidKind.payload, ['data', 'refundCreate', 'userErrors']),
      missingParentUserErrors: readPath(missingParent.payload, ['data', 'refundCreate', 'userErrors']),
      mismatchedGatewayUserErrors: readPath(mismatchedGateway.payload, ['data', 'refundCreate', 'userErrors']),
      happyRefundId: readPath(happyPath.payload, ['data', 'refundCreate', 'refund', 'id']),
    },
    null,
    2,
  ),
);
