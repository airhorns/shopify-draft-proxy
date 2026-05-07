/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedCase = {
  label: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
if (apiVersion !== '2025-01') {
  throw new Error(
    `orderCreate math matrix capture requires SHOPIFY_CONFORMANCE_API_VERSION=2025-01, got ${apiVersion}`,
  );
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'order-create-math-matrix.json');

const orderCreateMutation = `#graphql
  mutation OrderCreateMathMatrix($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFinancialStatus
        displayFulfillmentStatus
        paymentGatewayNames
        subtotalPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        currentSubtotalPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        totalShippingPriceSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        currentTotalTaxSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        totalTaxSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        currentTotalDiscountsSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        totalDiscountsSet {
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
        totalOutstandingSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        totalReceivedSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        totalCapturableSet {
          shopMoney { amount currencyCode }
          presentmentMoney { amount currencyCode }
        }
        capturable
        discountCodes
        shippingLines(first: 5) {
          nodes {
            title
            code
            source
            originalPriceSet {
              shopMoney { amount currencyCode }
              presentmentMoney { amount currencyCode }
            }
            taxLines {
              title
              rate
              priceSet {
                shopMoney { amount currencyCode }
                presentmentMoney { amount currencyCode }
              }
            }
          }
        }
        lineItems(first: 5) {
          nodes {
            title
            quantity
            sku
            originalUnitPriceSet {
              shopMoney { amount currencyCode }
              presentmentMoney { amount currencyCode }
            }
            taxLines {
              title
              rate
              priceSet {
                shopMoney { amount currencyCode }
                presentmentMoney { amount currencyCode }
              }
            }
          }
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
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation CleanupOrderCreateMathMatrix(
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

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
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

async function run(query: string, variables: JsonRecord): Promise<ConformanceGraphqlResult<JsonRecord>> {
  return runGraphqlRequest<JsonRecord>(trimGraphql(query), variables);
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

function lineItem(
  label: string,
  amount: string,
  quantity: number,
  currencyCode: string,
  options: {
    sku?: string;
    presentmentAmount?: string;
    presentmentCurrencyCode?: string;
    taxAmount?: string;
    taxRate?: number;
  } = {},
): JsonRecord {
  const taxLines = options.taxAmount
    ? [
        {
          title: `${label} tax`,
          rate: options.taxRate ?? 0.1,
          priceSet: moneySet(
            options.taxAmount,
            currencyCode,
            options.taxAmount,
            options.presentmentCurrencyCode ?? currencyCode,
          ),
        },
      ]
    : [];
  return {
    title: label,
    quantity,
    sku: options.sku ?? label.toLowerCase().replaceAll(/\s+/gu, '-'),
    priceSet: moneySet(amount, currencyCode, options.presentmentAmount ?? amount, options.presentmentCurrencyCode),
    requiresShipping: true,
    taxable: taxLines.length > 0,
    taxLines,
  };
}

function shippingLine(
  title: string,
  amount: string,
  currencyCode: string,
  options: {
    code?: string;
    source?: string;
    presentmentAmount?: string;
    presentmentCurrencyCode?: string;
    taxAmount?: string;
    taxRate?: number;
  } = {},
): JsonRecord {
  const taxLines = options.taxAmount
    ? [
        {
          title: `${title} tax`,
          rate: options.taxRate ?? 0.1,
          priceSet: moneySet(
            options.taxAmount,
            currencyCode,
            options.taxAmount,
            options.presentmentCurrencyCode ?? currencyCode,
          ),
        },
      ]
    : [];
  return {
    title,
    code: options.code ?? title.toUpperCase().replaceAll(/\s+/gu, '_'),
    source: options.source ?? 'hermes-conformance',
    priceSet: moneySet(amount, currencyCode, options.presentmentAmount ?? amount, options.presentmentCurrencyCode),
    taxLines,
  };
}

function transaction(kind: string, amount: string, currencyCode: string, gateway = 'manual'): JsonRecord {
  return {
    kind,
    status: 'SUCCESS',
    gateway,
    test: true,
    amountSet: moneySet(amount, currencyCode),
  };
}

function baseOrder(label: string, stamp: number, currency = 'CAD'): JsonRecord {
  return {
    email: `hermes-order-math-${label}-${stamp}@example.com`,
    note: `orderCreate math matrix ${label}`,
    tags: ['order-create-math-matrix', label],
    test: true,
    currency,
  };
}

function variablesForCases(stamp: number): Record<string, JsonRecord> {
  return {
    pendingNoTax: {
      order: {
        ...baseOrder('pending-no-tax', stamp),
        lineItems: [lineItem('Pending no tax', '10.00', 2, 'CAD', { sku: 'MATH-PENDING' })],
      },
      options: { inventoryBehaviour: 'BYPASS', sendReceipt: false, sendFulfillmentReceipt: false },
    },
    paidTaxShipping: {
      order: {
        ...baseOrder('paid-tax-shipping', stamp),
        financialStatus: 'PAID',
        lineItems: [
          lineItem('Paid taxable line', '12.00', 2, 'CAD', {
            sku: 'MATH-PAID-TAX',
            taxAmount: '2.40',
            taxRate: 0.1,
          }),
        ],
        shippingLines: [
          shippingLine('Math Ground', '5.00', 'CAD', {
            code: 'MATH_GROUND',
            taxAmount: '0.50',
            taxRate: 0.1,
          }),
        ],
        transactions: [transaction('SALE', '31.90', 'CAD')],
      },
      options: { inventoryBehaviour: 'BYPASS', sendReceipt: false, sendFulfillmentReceipt: false },
    },
    fixedDiscount: {
      order: {
        ...baseOrder('fixed-discount', stamp),
        lineItems: [lineItem('Fixed discount line', '30.00', 1, 'CAD', { sku: 'MATH-DISCOUNT' })],
        shippingLines: [shippingLine('Discount Ground', '4.00', 'CAD', { code: 'MATH_DISCOUNT_GROUND' })],
        discountCode: {
          itemFixedDiscountCode: {
            code: 'MATH5',
            amountSet: moneySet('5.00', 'CAD'),
          },
        },
      },
      options: { inventoryBehaviour: 'BYPASS', sendReceipt: false, sendFulfillmentReceipt: false },
    },
    authorization: {
      order: {
        ...baseOrder('authorization', stamp),
        lineItems: [lineItem('Authorization line', '18.00', 1, 'CAD', { sku: 'MATH-AUTH' })],
        transactions: [transaction('AUTHORIZATION', '18.00', 'CAD')],
      },
      options: { inventoryBehaviour: 'BYPASS', sendReceipt: false, sendFulfillmentReceipt: false },
    },
    presentmentMoney: {
      order: {
        ...baseOrder('presentment-money', stamp, 'CAD'),
        presentmentCurrency: 'USD',
        lineItems: [
          lineItem('Presentment line', '10.00', 1, 'CAD', {
            sku: 'MATH-FX',
            presentmentAmount: '7.00',
            presentmentCurrencyCode: 'USD',
          }),
        ],
        transactions: [
          {
            ...transaction('SALE', '10.00', 'CAD'),
            amountSet: moneySet('10.00', 'CAD', '7.00', 'USD'),
          },
        ],
      },
      options: { inventoryBehaviour: 'BYPASS', sendReceipt: false, sendFulfillmentReceipt: false },
    },
  };
}

function orderIdFromCreate(result: ConformanceGraphqlResult<JsonRecord>): string | null {
  return readString(readRecord(readRecord(result.payload.data, 'orderCreate'), 'order'), 'id');
}

function userErrors(result: ConformanceGraphqlResult<JsonRecord>): unknown[] {
  return readArray(readRecord(result.payload.data, 'orderCreate'), 'userErrors');
}

async function captureCase(label: string, variables: JsonRecord): Promise<CapturedCase> {
  const response = await run(orderCreateMutation, variables);
  const orderId = orderIdFromCreate(response);
  const errors = userErrors(response);
  if (!orderId || response.payload.errors || errors.length > 0) {
    throw new Error(`orderCreate ${label} failed: ${JSON.stringify(response.payload, null, 2)}`);
  }
  console.log(`Captured ${label}: ${orderId}`);
  return { label, variables, response };
}

async function cleanupOrder(orderId: string): Promise<ConformanceGraphqlResult<JsonRecord>> {
  return run(orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
}

const stamp = Date.now();
const variables = variablesForCases(stamp);
const createdOrderIds: string[] = [];
const cases: Record<string, CapturedCase> = {};
const cleanup: Record<string, ConformanceGraphqlResult<JsonRecord>> = {};

try {
  for (const [label, caseVariables] of Object.entries(variables)) {
    const captured = await captureCase(label, caseVariables);
    cases[label] = captured;
    const orderId = orderIdFromCreate(captured.response);
    if (orderId) {
      createdOrderIds.push(orderId);
    }
  }
} finally {
  for (const orderId of createdOrderIds) {
    try {
      cleanup[orderId] = await cleanupOrder(orderId);
      console.log(`Cleanup orderCancel attempted for ${orderId}`);
    } catch (error) {
      cleanup[orderId] = {
        status: 0,
        payload: { errors: [{ message: error instanceof Error ? error.message : String(error) }] },
      };
    }
  }
}

await writeJson(fixturePath, {
  scenarioId: 'order-create-math-matrix',
  recordedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  document: trimGraphql(orderCreateMutation),
  cases,
  cleanup,
  upstreamCalls: [],
});
console.log(`Wrote ${fixturePath}`);
