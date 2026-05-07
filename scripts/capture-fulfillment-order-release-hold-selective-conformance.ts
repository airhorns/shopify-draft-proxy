/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { readFileSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

type CreatedOrder = {
  id: string;
  name: string | null;
  fulfillmentOrderId: string;
};

const scenarioId = 'fulfillment-order-release-hold-selective';
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

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const requestDir = path.join('config', 'parity-requests', 'shipping-fulfillments');

function readRequest(name: string): string {
  return readFileSync(path.join(requestDir, name), 'utf8');
}

const holdMutation = readRequest('fulfillment-order-release-hold-selective-hold.graphql');
const releaseHoldMutation = readRequest('fulfillment-order-release-hold-selective-release.graphql');
const orderReadQuery = readRequest('fulfillment-order-release-hold-selective-order-read.graphql');
const hydrateQuery = readRequest('fulfillment-order-release-hold-selective-hydrate.graphql');

const orderCreateMutation = `#graphql
  mutation CreateFulfillmentOrderReleaseHoldSelectiveOrder(
    $order: OrderCreateOrderInput!
    $options: OrderCreateOptionsInput
  ) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 5) {
          nodes {
            id
            status
            requestStatus
            updatedAt
            lineItems(first: 5) {
              nodes {
                id
                totalQuantity
                remainingQuantity
                lineItem {
                  id
                  title
                  quantity
                  fulfillableQuantity
                }
              }
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
  mutation CleanupFulfillmentOrderReleaseHoldSelectiveOrder(
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
      orderCancelUserErrors {
        field
        message
        code
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  return {
    query: trimGraphql(query),
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null ? (value as JsonRecord) : null;
}

function readNodes(value: unknown): JsonRecord[] {
  const record = readObject(value);
  const nodes = record?.['nodes'];
  return Array.isArray(nodes) ? nodes.filter((node): node is JsonRecord => readObject(node) !== null) : [];
}

function getFirstFulfillmentOrder(order: JsonRecord): JsonRecord | null {
  return readNodes(readObject(order['fulfillmentOrders']))[0] ?? null;
}

function asCreatedOrder(captureResult: GraphqlCapture): CreatedOrder {
  const data = readObject(captureResult.response.payload.data);
  const orderCreate = readObject(data?.['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  if (!order || typeof order['id'] !== 'string') {
    throw new Error(`Unable to create disposable selective-release order: ${JSON.stringify(captureResult.response)}`);
  }

  const fulfillmentOrder = getFirstFulfillmentOrder(order);
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(captureResult.response)}`);
  }

  return {
    id: order['id'],
    name: typeof order['name'] === 'string' ? order['name'] : null,
    fulfillmentOrderId: fulfillmentOrder['id'],
  };
}

function readUserErrors(captureResult: GraphqlCapture, payloadName: string): JsonRecord[] {
  const data = readObject(captureResult.response.payload.data);
  const payload = readObject(data?.[payloadName]);
  const errors = payload?.['userErrors'];
  return Array.isArray(errors) ? errors.filter((error): error is JsonRecord => readObject(error) !== null) : [];
}

function assertNoUserErrors(captureResult: GraphqlCapture, payloadName: string, label: string): void {
  const errors = readUserErrors(captureResult, payloadName);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function readHoldId(captureResult: GraphqlCapture): string {
  const data = readObject(captureResult.response.payload.data);
  const payload = readObject(data?.['fulfillmentOrderHold']);
  const hold = readObject(payload?.['fulfillmentHold']);
  const id = hold?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`Unable to read fulfillment hold id: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return id;
}

function holdVariables(order: CreatedOrder, handle: string): JsonRecord {
  return {
    id: order.fulfillmentOrderId,
    fulfillmentHold: {
      reason: 'OTHER',
      reasonNotes: `Selective release ${handle}`,
      notifyMerchant: false,
      externalId: handle,
      handle,
    },
  };
}

async function createOrder(): Promise<{ order: CreatedOrder; capture: GraphqlCapture }> {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const variables = {
    order: {
      email: `selective-release-${stamp}@example.com`,
      note: `fulfillment-order selective release ${stamp}`,
      tags: ['draft-proxy', 'fulfillment-order-release-hold-selective'],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'Draft',
        lastName: 'Proxy',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          title: `Selective release item ${stamp}`,
          quantity: 2,
          priceSet: {
            shopMoney: {
              amount: '20.00',
              currencyCode: 'USD',
            },
          },
          requiresShipping: true,
          taxable: true,
        },
      ],
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
  const createCapture = await capture(orderCreateMutation, variables);
  return {
    order: asCreatedOrder(createCapture),
    capture: createCapture,
  };
}

const startedAt = new Date().toISOString();
const { order, capture: create } = await createOrder();
const hydrate = await capture(hydrateQuery, { id: order.fulfillmentOrderId });

const firstHold = await capture(holdMutation, holdVariables(order, 'selective-release-first'));
assertNoUserErrors(firstHold, 'fulfillmentOrderHold', 'first hold');
const firstHoldId = readHoldId(firstHold);

const secondHold = await capture(holdMutation, holdVariables(order, 'selective-release-second'));
assertNoUserErrors(secondHold, 'fulfillmentOrderHold', 'second hold');
const secondHoldId = readHoldId(secondHold);

const releaseFirstHold = await capture(releaseHoldMutation, {
  id: order.fulfillmentOrderId,
  holdIds: [firstHoldId],
});
assertNoUserErrors(releaseFirstHold, 'fulfillmentOrderReleaseHold', 'release first hold');

const afterReleaseRead = await capture(orderReadQuery, { id: order.id });

const cleanupReleaseRemaining = await capture(releaseHoldMutation, {
  id: order.fulfillmentOrderId,
  holdIds: [secondHoldId],
});
const cleanupCancel = await capture(orderCancelMutation, {
  orderId: order.id,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: false,
});

const output = {
  metadata: {
    capturedAt: new Date().toISOString(),
    startedAt,
    storeDomain,
    apiVersion,
    scopedRoots: ['fulfillmentOrderHold', 'fulfillmentOrderReleaseHold'],
    createdOrder: order,
  },
  setup: {
    create,
    hydrate,
  },
  workflows: {
    selectiveRelease: {
      firstHold,
      secondHold,
      releaseFirstHold,
      afterReleaseRead,
    },
  },
  cleanup: {
    releaseRemainingHold: cleanupReleaseRemaining,
    cancel: cleanupCancel,
  },
  upstreamCalls: [
    {
      operationName: 'ShippingFulfillmentOrderHydrate',
      variables: { id: order.fulfillmentOrderId },
      query: trimGraphql(hydrateQuery),
      response: {
        status: hydrate.response.status,
        body: hydrate.response.payload,
      },
    },
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

console.log(`Captured fulfillment-order selective release fixture: ${outputPath}`);
