/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

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
  fulfillmentOrderLineItemId: string;
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

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'fulfillment-order-hold-validation.json');

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderHoldValidationFields on FulfillmentOrder {
    id
    status
    requestStatus
    updatedAt
    supportedActions {
      action
    }
    fulfillmentHolds {
      id
      handle
      reason
      reasonNotes
      displayReason
      heldByApp {
        id
        title
      }
      heldByRequestingApp
    }
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
`;

const orderCreateMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation CreateFulfillmentOrderHoldValidationOrder($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 5) {
          nodes {
            ...FulfillmentOrderHoldValidationFields
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
  mutation CleanupFulfillmentOrderHoldValidationOrder(
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

const hydrateQuery = `#graphql
  query ShippingFulfillmentOrderHydrate($id: ID!) {
    fulfillmentOrder(id: $id) {
      id
      status
      requestStatus
      fulfillAt
      fulfillBy
      updatedAt
      supportedActions { action }
      assignedLocation { name location { id name } }
      fulfillmentHolds { id handle reason reasonNotes displayReason heldByApp { id title } heldByRequestingApp }
      merchantRequests(first: 10) { nodes { kind message requestOptions } }
      lineItems(first: 20) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
      order { id name displayFulfillmentStatus }
    }
  }
`;

const holdMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderHoldValidation($id: ID!, $fulfillmentHold: FulfillmentOrderHoldInput!) {
    fulfillmentOrderHold(id: $id, fulfillmentHold: $fulfillmentHold) {
      fulfillmentHold {
        id
        handle
        reason
        reasonNotes
        displayReason
        heldByRequestingApp
      }
      fulfillmentOrder {
        ...FulfillmentOrderHoldValidationFields
      }
      remainingFulfillmentOrder {
        ...FulfillmentOrderHoldValidationFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const releaseHoldMutation = `#graphql
  mutation CleanupFulfillmentOrderHoldValidationHolds($id: ID!, $holdIds: [ID!]) {
    fulfillmentOrderReleaseHold(id: $id, holdIds: $holdIds) {
      fulfillmentOrder {
        id
        status
      }
      userErrors {
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

function getFirstFulfillmentOrderLineItem(fulfillmentOrder: JsonRecord): JsonRecord | null {
  return readNodes(readObject(fulfillmentOrder['lineItems']))[0] ?? null;
}

function asCreatedOrder(captureResult: GraphqlCapture): CreatedOrder {
  const data = readObject(captureResult.response.payload.data);
  const orderCreate = readObject(data?.['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  if (!order || typeof order['id'] !== 'string') {
    throw new Error(
      `Unable to create disposable hold-validation order: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }

  const fulfillmentOrder = getFirstFulfillmentOrder(order);
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(captureResult.response.payload)}`);
  }

  const lineItem = getFirstFulfillmentOrderLineItem(fulfillmentOrder);
  if (!lineItem || typeof lineItem['id'] !== 'string') {
    throw new Error(`Created fulfillment order has no line item: ${JSON.stringify(captureResult.response.payload)}`);
  }

  return {
    id: order['id'],
    name: typeof order['name'] === 'string' ? order['name'] : null,
    fulfillmentOrderId: fulfillmentOrder['id'],
    fulfillmentOrderLineItemId: lineItem['id'],
  };
}

async function createValidationOrder(): Promise<{ order: CreatedOrder; capture: GraphqlCapture }> {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const variables = {
    order: {
      email: `har-552-hold-validation-${stamp}@example.com`,
      note: `HAR-552 fulfillment-order hold validation ${stamp}`,
      tags: ['har-552', 'fulfillment-order-hold-validation'],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'HAR',
        lastName: 'Probe',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          title: `HAR-552 fulfillment-order hold validation ${stamp}`,
          quantity: 5,
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

function holdVariables(
  order: CreatedOrder,
  handle: string,
  lineItems: Array<{ id: string; quantity: number }> | null,
): JsonRecord {
  const fulfillmentHold: JsonRecord = {
    reason: 'OTHER',
    reasonNotes: `HAR-552 ${handle}`,
    notifyMerchant: false,
    externalId: handle,
    handle,
  };
  if (lineItems) {
    fulfillmentHold['fulfillmentOrderLineItems'] = lineItems;
  }
  return {
    id: order.fulfillmentOrderId,
    fulfillmentHold,
  };
}

function readHoldIds(...captures: GraphqlCapture[]): string[] {
  return captures
    .map((captureResult) => {
      const data = readObject(captureResult.response.payload.data);
      const payload = readObject(data?.['fulfillmentOrderHold']);
      const hold = readObject(payload?.['fulfillmentHold']);
      return hold?.['id'];
    })
    .filter((id): id is string => typeof id === 'string');
}

const startedAt = new Date().toISOString();
const { order, capture: create } = await createValidationOrder();
const hydrate = await capture(hydrateQuery, { id: order.fulfillmentOrderId });

const firstHold = await capture(
  holdMutation,
  holdVariables(order, 'appA-1', [{ id: order.fulfillmentOrderLineItemId, quantity: 2 }]),
);
const duplicateHandle = await capture(holdMutation, holdVariables(order, 'appA-1', null));
const notSplittable = await capture(
  holdMutation,
  holdVariables(order, 'appA-2', [{ id: order.fulfillmentOrderLineItemId, quantity: 1 }]),
);
const secondHold = await capture(holdMutation, holdVariables(order, 'appA-3', null));
const additionalHolds: GraphqlCapture[] = [];
for (let index = 4; index <= 11; index += 1) {
  additionalHolds.push(await capture(holdMutation, holdVariables(order, `appA-${index}`, null)));
}
const limitReached = await capture(holdMutation, holdVariables(order, 'appA-12', null));
const zeroQuantity = await capture(
  holdMutation,
  holdVariables(order, 'appA-13', [{ id: order.fulfillmentOrderLineItemId, quantity: 0 }]),
);
const duplicateLineItems = await capture(
  holdMutation,
  holdVariables(order, 'appA-14', [
    { id: order.fulfillmentOrderLineItemId, quantity: 1 },
    { id: order.fulfillmentOrderLineItemId, quantity: 1 },
  ]),
);

const holdIds = readHoldIds(firstHold, secondHold, ...additionalHolds);
const cleanupRelease =
  holdIds.length > 0 ? await capture(releaseHoldMutation, { id: order.fulfillmentOrderId, holdIds }) : null;
const cleanupCancel = await capture(orderCancelMutation, {
  orderId: order.id,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: false,
});

const output = {
  metadata: {
    issue: 'HAR-552',
    capturedAt: new Date().toISOString(),
    startedAt,
    storeDomain,
    apiVersion,
    scopedRoots: ['fulfillmentOrderHold'],
    createdOrder: order,
  },
  setup: {
    create,
    hydrate,
  },
  workflows: {
    validation: {
      firstHold,
      duplicateHandle,
      notSplittable,
      secondHold,
      additionalHolds,
      limitReached,
      zeroQuantity,
      duplicateLineItems,
    },
  },
  cleanup: {
    releaseHold: cleanupRelease,
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

console.log(`Captured fulfillment-order hold validation fixture: ${outputPath}`);
