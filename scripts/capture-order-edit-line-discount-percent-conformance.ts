/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

const cap = await createConformanceCapture();

const orderEditHydrateQuery = await cap.readRequestRaw('orders', 'order-edit-hydrate.graphql');
const beginDocument = await cap.readRequest('orders', 'orderEditResidualWorkflow-begin.graphql');
const addLineItemDiscountDocument = await cap.readRequest(
  'orders',
  'orderEditResidualWorkflow-addLineItemDiscount.graphql',
);

const orderCreateMutation = `#graphql
  mutation OrderEditPercentDiscountCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order { id name }
      userErrors { field message }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation OrderEditPercentDiscountCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job { id }
      userErrors { field message }
    }
  }
`;

function moneyAmount(value: unknown, label: string): string {
  return requireString(readRecord(readRecord(value)?.['shopMoney'])?.['amount'], label);
}

function assertAmount(value: unknown, expected: string, label: string): void {
  const actual = moneyAmount(value, label);
  if (actual !== expected) {
    throw new Error(`${label} expected ${expected}, got ${actual}`);
  }
}

const shopPayload = await cap.run(
  `#graphql
  query OrderEditPercentDiscountShop { shop { currencyCode } }
`,
  {},
  'discover shop currency',
);
const shopCurrency = requireString(
  readRecord(readRecord(shopPayload['data'])?.['shop'])?.['currencyCode'],
  'shop currencyCode',
);

const createPayload = await cap.run(
  orderCreateMutation,
  {
    order: {
      email: `order-edit-percent-discount-${cap.stamp}@example.com`,
      note: `Order edit percent line discount capture ${cap.stamp}`,
      tags: ['order-edit-percent-discount', cap.stamp],
      test: true,
      currency: shopCurrency,
      lineItems: [
        {
          title: `Percent discount base line ${cap.stamp}`,
          quantity: 2,
          priceSet: { shopMoney: { amount: '100.00', currencyCode: shopCurrency } },
          requiresShipping: false,
          taxable: false,
        },
      ],
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  },
  'orderCreate',
);
const createRoot = cap.mutationRoot(createPayload, 'orderCreate', 'orderCreate');
const order = readRecord(createRoot['order']);
const orderId = requireString(order?.['id'], 'created order id');

const hydratePayload = await cap.run(orderEditHydrateQuery, { id: orderId }, 'orderEditHydrate');
const beginPayload = await cap.run(beginDocument, { id: orderId }, 'orderEditBegin');
const beginRoot = cap.mutationRoot(beginPayload, 'orderEditBegin', 'orderEditBegin');
const calculatedOrder = readRecord(beginRoot['calculatedOrder']);
const calculatedOrderId = requireString(calculatedOrder?.['id'], 'calculated order id');
const lineItem = readRecord(readArray(readRecord(calculatedOrder?.['lineItems'])?.['nodes'])[0]);
const lineItemId = requireString(lineItem?.['id'], 'calculated line item id');

const percentDiscount = {
  description: 'Ten percent off',
  percentValue: 10.0,
};
const addPercentDiscountPayload = await cap.run(
  addLineItemDiscountDocument,
  {
    id: calculatedOrderId,
    lineItemId,
    discount: percentDiscount,
  },
  'orderEditAddLineItemDiscount percentValue',
);
const addPercentDiscountRoot = cap.mutationRoot(
  addPercentDiscountPayload,
  'orderEditAddLineItemDiscount',
  'orderEditAddLineItemDiscount percentValue',
);
const calculatedLineItem = readRecord(addPercentDiscountRoot['calculatedLineItem']);
const allocation = readRecord(readArray(calculatedLineItem?.['calculatedDiscountAllocations'])[0]);

assertAmount(calculatedLineItem?.['discountedUnitPriceSet'], '90.0', 'discounted unit price');
assertAmount(allocation?.['allocatedAmountSet'], '20.0', 'allocated discount amount');
assertAmount(
  readRecord(addPercentDiscountRoot['calculatedOrder'])?.['totalPriceSet'],
  '180.0',
  'calculated order total',
);

const cleanup = await cap.runGraphqlRequest<JsonRecord>(orderCancelMutation, {
  orderId,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: false,
});

const fixturePath = cap.fixturePath('orders', 'order-edit-line-discount-percent-value.json');
await cap.writeJson(fixturePath, {
  scenarioId: 'order-edit-line-discount-percent-value',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  variables: { id: orderId },
  steps: {
    begin: { variables: { id: orderId }, response: beginPayload },
    addPercentLineItemDiscount: {
      variables: {
        id: calculatedOrderId,
        lineItemId,
        discount: percentDiscount,
      },
      response: addPercentDiscountPayload,
    },
  },
  upstreamCalls: [
    {
      operationName: 'OrdersOrderEditHydrate',
      variables: { id: orderId },
      query: orderEditHydrateQuery,
      response: {
        status: 200,
        body: hydratePayload,
      },
    },
  ],
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      shopCurrency,
      orderId,
      orderName: order?.['name'] ?? null,
      calculatedOrderId,
      lineItemId,
      discountedUnitPrice: moneyAmount(calculatedLineItem?.['discountedUnitPriceSet'], 'discounted unit price'),
      allocatedAmount: moneyAmount(allocation?.['allocatedAmountSet'], 'allocated discount amount'),
      calculatedOrderTotal: moneyAmount(
        readRecord(addPercentDiscountRoot['calculatedOrder'])?.['totalPriceSet'],
        'total',
      ),
      cleanupStatus: cleanup.status,
      cleanupUserErrors: readArray(readRecord(readRecord(cleanup.payload?.['data'])?.['orderCancel'])?.['userErrors']),
    } satisfies JsonRecord,
    null,
    2,
  ),
);
