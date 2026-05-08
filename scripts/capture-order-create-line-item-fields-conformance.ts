/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
if (apiVersion !== '2025-01') {
  throw new Error(
    `orderCreate line-item field capture requires SHOPIFY_CONFORMANCE_API_VERSION=2025-01, got ${apiVersion}`,
  );
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'order-create-line-item-fields.json');

const productLookupQuery = `#graphql
  query ProductForOrderCreateLineItemFields {
    products(first: 1, query: "status:active") {
      nodes {
        id
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  mutation OrderCreateLineItemFields($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        subtotalPriceSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        lineItems(first: 5) {
          nodes {
            id
            title
            quantity
            sku
            customAttributes { key value }
            requiresShipping
            taxable
            vendor
            product { id }
            originalUnitPriceSet { shopMoney { amount currencyCode } }
            discountAllocations {
              allocatedAmountSet { shopMoney { amount currencyCode } }
            }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query OrderCreateLineItemFieldsDownstreamRead($id: ID!) {
    order(id: $id) {
      id
      subtotalPriceSet { shopMoney { amount currencyCode } }
      totalDiscountsSet { shopMoney { amount currencyCode } }
      currentTotalPriceSet { shopMoney { amount currencyCode } }
      lineItems(first: 5) {
        nodes {
          id
          title
          quantity
          sku
          customAttributes { key value }
          requiresShipping
          taxable
          vendor
          product { id }
          originalUnitPriceSet { shopMoney { amount currencyCode } }
          discountAllocations {
            allocatedAmountSet { shopMoney { amount currencyCode } }
          }
        }
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation CleanupOrderCreateLineItemFields(
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

function orderIdFromCreate(result: ConformanceGraphqlResult<JsonRecord>): string | null {
  return readString(readRecord(readRecord(result.payload.data, 'orderCreate'), 'order'), 'id');
}

function userErrors(result: ConformanceGraphqlResult<JsonRecord>): unknown[] {
  return readArray(readRecord(result.payload.data, 'orderCreate'), 'userErrors');
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
const productLookup = await run(productLookupQuery, {});
const productNode = readArray(readRecord(productLookup.payload.data, 'products'), 'nodes')[0];
const productId = readString(productNode, 'id');
if (!productId || productLookup.payload.errors) {
  throw new Error(
    `Unable to resolve a product id for orderCreate line-item field capture: ${JSON.stringify(productLookup.payload, null, 2)}`,
  );
}

const variables = {
  order: {
    email: `hermes-order-line-item-fields-${stamp}@example.com`,
    note: 'orderCreate line-item field coverage',
    tags: ['order-create-line-item-fields'],
    test: true,
    currency: 'USD',
    lineItems: [
      {
        title: 'Line item field coverage',
        quantity: 2,
        sku: `LINE-FIELDS-${stamp}`,
        vendor: 'Hermes Vendor',
        productId,
        properties: [
          { name: 'engraving', value: 'Ada' },
          { name: 'fulfillment_note', value: 'Pack flat' },
        ],
        requiresShipping: false,
        taxable: false,
        priceSet: {
          shopMoney: {
            amount: '12.00',
            currencyCode: 'USD',
          },
        },
        taxLines: [],
      },
    ],
  },
  options: {
    inventoryBehaviour: 'BYPASS',
    sendReceipt: false,
    sendFulfillmentReceipt: false,
  },
};

const mutation = await run(orderCreateMutation, variables);
const orderId = orderIdFromCreate(mutation);
const errors = userErrors(mutation);
if (!orderId || mutation.payload.errors || errors.length > 0) {
  throw new Error(`orderCreate line-item field capture failed: ${JSON.stringify(mutation.payload, null, 2)}`);
}

const downstreamRead = await run(downstreamReadQuery, { id: orderId });
const cleanup = await cleanupOrder(orderId);

await writeJson(fixturePath, {
  scenarioId: 'order-create-line-item-fields',
  recordedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  setup: {
    productLookup: {
      variables: {},
      response: productLookup.payload,
    },
  },
  document: trimGraphql(orderCreateMutation),
  variables,
  mutation: {
    response: mutation.payload,
  },
  downstreamRead: {
    document: trimGraphql(downstreamReadQuery),
    variables: { id: orderId },
    response: downstreamRead.payload,
  },
  cleanup: {
    variables: { orderId },
    response: cleanup.payload,
  },
  upstreamCalls: [],
});

console.log(`Captured orderCreate line-item fields: ${orderId}`);
console.log(`Wrote ${fixturePath}`);
