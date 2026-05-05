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
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

type CreatedOrder = {
  id: string;
  name: string | null;
  response: ConformanceGraphqlPayload<JsonRecord>;
  variables: JsonRecord;
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

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const outputPath = path.join(outputDir, 'orderMarkAsPaid-state-and-money.json');

const orderMoneyBagFields = `#graphql
  fragment OrderMarkAsPaidMoneyBagFields on Order {
    id
    name
    createdAt
    updatedAt
    closed
    closedAt
    cancelledAt
    cancelReason
    presentmentCurrencyCode
    displayFinancialStatus
    displayFulfillmentStatus
    paymentGatewayNames
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
`;

const orderCreateMutation = `#graphql
  ${orderMoneyBagFields}
  mutation OrderMarkAsPaidSetupOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...OrderMarkAsPaidMoneyBagFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderMarkAsPaidMutation = `#graphql
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

const orderCancelMutation = `#graphql
  mutation CleanupOrderMarkAsPaidStateAndMoney(
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

function userErrors(result: ConformanceGraphqlResult<JsonRecord>, pathName: string): unknown[] {
  return readArray(readRecord(result.payload.data, pathName), 'userErrors');
}

function requireOrder(result: ConformanceGraphqlResult<JsonRecord>, label: string): JsonRecord {
  const order = readRecord(readRecord(result.payload.data, 'orderCreate'), 'order');
  const errors = userErrors(result, 'orderCreate');
  if (!order || !readString(order, 'id') || errors.length > 0) {
    throw new Error(`Unable to create ${label} order: ${JSON.stringify(result.payload)}`);
  }
  return order;
}

function moneyBag(
  amount: string,
  shopCurrency: string,
  presentmentCurrency = shopCurrency,
  presentmentAmount = amount,
): JsonRecord {
  return {
    shopMoney: {
      amount,
      currencyCode: shopCurrency,
    },
    presentmentMoney: {
      amount: presentmentAmount,
      currencyCode: presentmentCurrency,
    },
  };
}

function makeOrderVariables(
  stamp: number,
  scenario: string,
  options: {
    shopCurrency: string;
    presentmentCurrency?: string;
    paid: boolean;
  },
): JsonRecord {
  const presentmentCurrency = options.presentmentCurrency ?? options.shopCurrency;
  const shopAmount = options.presentmentCurrency ? '16.99' : '12.50';
  const presentmentAmount = '12.50';
  const priceSet = moneyBag(shopAmount, options.shopCurrency, presentmentCurrency, presentmentAmount);
  const order: JsonRecord = {
    email: `hermes-mark-paid-${scenario}-${stamp}@example.com`,
    note: `orderMarkAsPaid ${scenario} state-and-money capture`,
    tags: ['parity-probe', 'order-mark-as-paid', scenario],
    test: true,
    currency: options.shopCurrency,
    presentmentCurrency,
    lineItems: [
      {
        title: `Hermes mark-as-paid ${scenario}`,
        quantity: 1,
        priceSet,
        requiresShipping: false,
        taxable: false,
        sku: `hermes-mark-paid-${scenario}-${stamp}`,
      },
    ],
  };
  if (options.paid) {
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

async function createOrder(
  stamp: number,
  scenario: string,
  options: {
    shopCurrency: string;
    presentmentCurrency?: string;
    paid: boolean;
  },
): Promise<CreatedOrder> {
  const variables = makeOrderVariables(stamp, scenario, options);
  const response = await run(orderCreateMutation, variables);
  const order = requireOrder(response, scenario);
  const id = readString(order, 'id');
  if (!id) {
    throw new Error(`Created ${scenario} order is missing id: ${JSON.stringify(response.payload)}`);
  }
  return {
    id,
    name: readString(order, 'name'),
    variables,
    response: response.payload,
  };
}

async function markAsPaid(orderId: string): Promise<GraphqlCapture> {
  const variables = { input: { id: orderId } };
  const response = await run(orderMarkAsPaidMutation, variables);
  return { variables, response };
}

async function cleanupOrder(orderId: string): Promise<GraphqlCapture> {
  const variables = {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  };
  const response = await run(orderCancelMutation, variables);
  return { variables, response };
}

function hydrateCall(order: CreatedOrder): JsonRecord {
  return {
    operationName: 'OrdersOrderHydrate',
    variables: { id: order.id },
    query: 'hand-synthesized from checked-in setup orderCreate response for orderMarkAsPaid Pattern 2 hydration',
    response: {
      status: 200,
      body: {
        data: {
          order: readRecord(readRecord(order.response.data, 'orderCreate'), 'order'),
        },
      },
    },
  };
}

function assertMultiCurrencyOrder(order: CreatedOrder): void {
  const created = readRecord(readRecord(order.response.data, 'orderCreate'), 'order');
  const presentmentCurrencyCode = readString(created, 'presentmentCurrencyCode');
  const shopCurrencyCode = readString(
    readRecord(readRecord(created, 'currentTotalPriceSet'), 'shopMoney'),
    'currencyCode',
  );
  if (presentmentCurrencyCode === shopCurrencyCode) {
    throw new Error(`Expected distinct presentment currency for multi-currency setup: ${JSON.stringify(created)}`);
  }
}

function orderMarkAsPaidPayload(result: GraphqlCapture): JsonRecord | null {
  return readRecord(result.response.payload.data, 'orderMarkAsPaid');
}

function lastTransactionPresentmentMoney(result: GraphqlCapture): JsonRecord | null {
  const order = readRecord(orderMarkAsPaidPayload(result), 'order');
  const transactions = readArray(order, 'transactions');
  const lastTransaction = asRecord(transactions.at(-1));
  return readRecord(readRecord(lastTransaction, 'amountSet'), 'presentmentMoney');
}

const stamp = Date.now();
const createdOrders: CreatedOrder[] = [];
const cleanup: GraphqlCapture[] = [];

try {
  const unpaid = await createOrder(stamp, 'unpaid-success', { shopCurrency: 'CAD', paid: false });
  createdOrders.push(unpaid);
  const alreadyPaid = await createOrder(stamp, 'already-paid', { shopCurrency: 'CAD', paid: true });
  createdOrders.push(alreadyPaid);
  const multiCurrency = await createOrder(stamp, 'multi-currency', {
    shopCurrency: 'CAD',
    presentmentCurrency: 'USD',
    paid: false,
  });
  createdOrders.push(multiCurrency);
  assertMultiCurrencyOrder(multiCurrency);

  const unpaidSuccess = await markAsPaid(unpaid.id);
  const alreadyPaidResult = await markAsPaid(alreadyPaid.id);
  const multiCurrencyResult = await markAsPaid(multiCurrency.id);

  await writeJson(outputPath, {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    setup: {
      unpaid,
      alreadyPaid,
      multiCurrency,
    },
    cases: {
      unpaidSuccess,
      alreadyPaid: alreadyPaidResult,
      multiCurrency: multiCurrencyResult,
    },
    upstreamCalls: [hydrateCall(unpaid), hydrateCall(alreadyPaid), hydrateCall(multiCurrency)],
  });

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        orders: createdOrders.map((order) => ({ id: order.id, name: order.name })),
        alreadyPaidUserErrors: readArray(orderMarkAsPaidPayload(alreadyPaidResult), 'userErrors'),
        multiCurrencyPresentment: lastTransactionPresentmentMoney(multiCurrencyResult),
      },
      null,
      2,
    ),
  );
} finally {
  for (const order of createdOrders) {
    try {
      cleanup.push(await cleanupOrder(order.id));
    } catch (error) {
      cleanup.push({
        variables: { orderId: order.id },
        response: {
          status: 0,
          payload: { errors: [{ message: error instanceof Error ? error.message : String(error) }] },
        },
      });
    }
  }
  if (createdOrders.length > 0) {
    await writeJson(path.join(outputDir, 'orderMarkAsPaid-state-and-money-cleanup.json'), {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cleanup,
    });
  }
}
