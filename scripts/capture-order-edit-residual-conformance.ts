/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'order-edit-residual-calculated-edits.json');
const requestDir = path.join('config', 'parity-requests', 'orders');

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
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

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

// Run a GraphQL document live and return its parsed response body ({data, errors}).
// Throws on transport errors and (unless the document is the disposable cleanup)
// on top-level GraphQL errors so a failed capture never silently writes a fixture.
async function run(query: string, variables: JsonRecord, label: string): Promise<JsonRecord> {
  const result: ConformanceGraphqlResult<JsonRecord> = await runGraphqlRequest<JsonRecord>(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return result.payload as JsonRecord;
}

function mutationRoot(payload: JsonRecord, rootName: string, label: string): JsonRecord {
  const root = readRecord(readRecord(payload['data'])?.[rootName]);
  if (!root) {
    throw new Error(`${label} missing ${rootName}: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${label} ${rootName} userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
  return root;
}

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

// The exact order-edit hydrate query the proxy forwards on a cold orderEditBegin.
// Read from the shared .graphql file (include_str! into ORDER_EDIT_HYDRATE_QUERY)
// so the recorded cassette byte-matches the forward under the strict matcher.
const orderEditHydrateQuery = await readFile(path.join(requestDir, 'order-edit-hydrate.graphql'), 'utf8');

const beginDocument = await readRequest('orderEditResidualWorkflow-begin.graphql');
const addCustomItemDocument = await readRequest('orderEditResidualWorkflow-addCustomItem.graphql');
const addLineItemDiscountDocument = await readRequest('orderEditResidualWorkflow-addLineItemDiscount.graphql');
const removeDiscountDocument = await readRequest('orderEditResidualWorkflow-removeDiscount.graphql');
const addShippingLineDocument = await readRequest('orderEditResidualWorkflow-addShippingLine.graphql');
const updateShippingLineDocument = await readRequest('orderEditResidualWorkflow-updateShippingLine.graphql');
const removeShippingLineDocument = await readRequest('orderEditResidualWorkflow-removeShippingLine.graphql');

// Disposable order with a single non-taxable custom line at $10 CAD. Non-taxable
// keeps the calculated-order totals tax-free so the proxy's local edit math
// (which does not model tax) matches Shopify's live calculated order.
const orderCreateMutation = `#graphql
  mutation OrderEditResidualCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation OrderEditResidualCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCreatePayload = await run(
  orderCreateMutation,
  {
    order: {
      email: `order-edit-residual-${stamp}@example.com`,
      note: `HAR-369 residual full capture ${stamp}`,
      tags: ['order-edit-residual', stamp],
      test: true,
      currency: 'CAD',
      lineItems: [
        {
          title: 'HAR-369 base custom line',
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: false,
          taxable: false,
          sku: `har-369-base-${stamp}`,
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
const createdOrder = mutationRoot(orderCreatePayload, 'orderCreate', 'orderCreate');
const orderId = requireString(readRecord(createdOrder['order'])?.['id'], 'created order id');

// Capture the exact hydrate the proxy forwards for this order on a cold edit begin.
const hydratePayload = await run(orderEditHydrateQuery, { id: orderId }, 'orderEditHydrate');

// --- Live residual order-edit workflow against the disposable order ---
const beginPayload = await run(beginDocument, { id: orderId }, 'orderEditBegin');
const beginRoot = mutationRoot(beginPayload, 'orderEditBegin', 'orderEditBegin');
const calculatedOrderId = requireString(readRecord(beginRoot['calculatedOrder'])?.['id'], 'calculated order id');

const addCustomItemPayload = await run(
  addCustomItemDocument,
  { id: calculatedOrderId, price: { amount: '3.00', currencyCode: 'CAD' } },
  'orderEditAddCustomItem',
);
const addCustomItemRoot = mutationRoot(addCustomItemPayload, 'orderEditAddCustomItem', 'orderEditAddCustomItem');
const customLineItemId = requireString(
  readRecord(addCustomItemRoot['calculatedLineItem'])?.['id'],
  'custom calculated line item id',
);

const addLineItemDiscountPayload = await run(
  addLineItemDiscountDocument,
  {
    id: calculatedOrderId,
    lineItemId: customLineItemId,
    discount: {
      description: 'HAR-369 line discount',
      fixedValue: { amount: '1.00', currencyCode: 'CAD' },
    },
  },
  'orderEditAddLineItemDiscount',
);
const addLineItemDiscountRoot = mutationRoot(
  addLineItemDiscountPayload,
  'orderEditAddLineItemDiscount',
  'orderEditAddLineItemDiscount',
);
const discountApplicationId = requireString(
  readRecord(
    readRecord(
      readArray(readRecord(addLineItemDiscountRoot['calculatedLineItem'])?.['calculatedDiscountAllocations'])[0],
    )?.['discountApplication'],
  )?.['id'],
  'discount application id',
);

const removeDiscountPayload = await run(
  removeDiscountDocument,
  { id: calculatedOrderId, discountApplicationId },
  'orderEditRemoveDiscount',
);
mutationRoot(removeDiscountPayload, 'orderEditRemoveDiscount', 'orderEditRemoveDiscount');

const addShippingLinePayload = await run(
  addShippingLineDocument,
  {
    id: calculatedOrderId,
    shippingLine: { title: 'HAR-369 Ground', price: { amount: '4.00', currencyCode: 'CAD' } },
  },
  'orderEditAddShippingLine',
);
const addShippingLineRoot = mutationRoot(
  addShippingLinePayload,
  'orderEditAddShippingLine',
  'orderEditAddShippingLine',
);
const shippingLineId = requireString(
  readRecord(addShippingLineRoot['calculatedShippingLine'])?.['id'],
  'calculated shipping line id',
);

const updateShippingLinePayload = await run(
  updateShippingLineDocument,
  {
    id: calculatedOrderId,
    shippingLineId,
    shippingLine: { title: 'HAR-369 Express', price: { amount: '6.00', currencyCode: 'CAD' } },
  },
  'orderEditUpdateShippingLine',
);
mutationRoot(updateShippingLinePayload, 'orderEditUpdateShippingLine', 'orderEditUpdateShippingLine');

const removeShippingLinePayload = await run(
  removeShippingLineDocument,
  { id: calculatedOrderId, shippingLineId },
  'orderEditRemoveShippingLine',
);
mutationRoot(removeShippingLinePayload, 'orderEditRemoveShippingLine', 'orderEditRemoveShippingLine');

// Best-effort cleanup of the disposable order (open uncommitted edit is discarded
// server-side when the order is cancelled). Cleanup errors are reported, not fatal.
const cleanup = await runGraphqlRequest<JsonRecord>(orderCancelMutation, {
  orderId,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: false,
});

await writeJson(fixturePath, {
  scenarioId: 'order-edit-residual-workflow-calculated-edits',
  apiVersion,
  storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  variables: { id: orderId },
  steps: {
    begin: { variables: { id: orderId }, response: beginPayload },
    addCustomItem: {
      variables: { id: calculatedOrderId, price: { amount: '3.00', currencyCode: 'CAD' } },
      response: addCustomItemPayload,
    },
    addLineItemDiscount: {
      variables: {
        id: calculatedOrderId,
        lineItemId: customLineItemId,
        discount: { description: 'HAR-369 line discount', fixedValue: { amount: '1.00', currencyCode: 'CAD' } },
      },
      response: addLineItemDiscountPayload,
    },
    removeDiscount: {
      variables: { id: calculatedOrderId, discountApplicationId },
      response: removeDiscountPayload,
    },
    addShippingLine: {
      variables: {
        id: calculatedOrderId,
        shippingLine: { title: 'HAR-369 Ground', price: { amount: '4.00', currencyCode: 'CAD' } },
      },
      response: addShippingLinePayload,
    },
    updateShippingLine: {
      variables: {
        id: calculatedOrderId,
        shippingLineId,
        shippingLine: { title: 'HAR-369 Express', price: { amount: '6.00', currencyCode: 'CAD' } },
      },
      response: updateShippingLinePayload,
    },
    removeShippingLine: {
      variables: { id: calculatedOrderId, shippingLineId },
      response: removeShippingLinePayload,
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
      orderId,
      calculatedOrderId,
      cleanupStatus: cleanup.status,
      cleanupUserErrors: readArray(readRecord(readRecord(cleanup.payload?.['data'])?.['orderCancel'])?.['userErrors']),
    },
    null,
    2,
  ),
);
