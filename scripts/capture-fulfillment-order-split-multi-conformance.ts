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

type RecordedUpstreamCall = {
  operationName: string;
  variables: JsonRecord;
  query: string;
  response: {
    status: number;
    body: JsonRecord;
  };
};

type CreatedOrder = {
  id: string;
  name: string | null;
  fulfillmentOrderId: string;
  fulfillmentOrderLineItemIds: string[];
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
const outputPath = path.join(outputDir, 'fulfillment-order-split-multi.json');
const physicalVariantA = 'gid://shopify/ProductVariant/48540157378793';
const physicalVariantB = 'gid://shopify/ProductVariant/51098706739506';

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderSplitMultiFields on FulfillmentOrder {
    id
    status
    requestStatus
    updatedAt
    supportedActions {
      action
    }
    assignedLocation {
      name
      location {
        id
        name
      }
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
  mutation CreateFulfillmentOrderSplitMultiOrder($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        fulfillmentOrders(first: 5) {
          nodes {
            ...FulfillmentOrderSplitMultiFields
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

const splitMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderSplitMulti($fulfillmentOrderSplits: [FulfillmentOrderSplitInput!]!) {
    fulfillmentOrderSplit(fulfillmentOrderSplits: $fulfillmentOrderSplits) {
      fulfillmentOrderSplits {
        fulfillmentOrder {
          ...FulfillmentOrderSplitMultiFields
        }
        remainingFulfillmentOrder {
          ...FulfillmentOrderSplitMultiFields
        }
        replacementFulfillmentOrder {
          ...FulfillmentOrderSplitMultiFields
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const mergeMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation CleanupFulfillmentOrderSplitMultiMerge($fulfillmentOrderMergeInputs: [FulfillmentOrderMergeInput!]!) {
    fulfillmentOrderMerge(fulfillmentOrderMergeInputs: $fulfillmentOrderMergeInputs) {
      fulfillmentOrderMerges {
        fulfillmentOrder {
          ...FulfillmentOrderSplitMultiFields
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const hydrateFulfillmentOrderQuery = `#graphql
  query ShippingFulfillmentOrderHydrate($id: ID!) {
    fulfillmentOrder(id: $id) {
      id status requestStatus fulfillAt fulfillBy updatedAt
      supportedActions { action }
      assignedLocation { name location { id name } }
      fulfillmentHolds { id handle reason reasonNotes displayReason heldByApp { id title } heldByRequestingApp }
      merchantRequests(first: 10) { nodes { kind message requestOptions } }
      lineItems(first: 20) { nodes { id totalQuantity remainingQuantity lineItem { id title quantity fulfillableQuantity } } }
      order { id name displayFulfillmentStatus }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation CleanupFulfillmentOrderSplitMultiOrder(
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

function getFulfillmentOrder(order: JsonRecord): JsonRecord {
  const fulfillmentOrder = readNodes(readObject(order['fulfillmentOrders']))[0];
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(order)}`);
  }
  return fulfillmentOrder;
}

function asCreatedOrder(captureResult: GraphqlCapture): CreatedOrder {
  const data = readObject(captureResult.response.payload.data);
  const orderCreate = readObject(data?.['orderCreate']);
  const userErrors = orderCreate?.['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`Unable to create disposable order: ${JSON.stringify(userErrors)}`);
  }
  const order = readObject(orderCreate?.['order']);
  if (!order || typeof order['id'] !== 'string') {
    throw new Error(`Unable to create disposable order: ${JSON.stringify(captureResult.response.payload)}`);
  }

  const fulfillmentOrder = getFulfillmentOrder(order);
  const lineItemIds = readNodes(readObject(fulfillmentOrder['lineItems']))
    .map((lineItem) => lineItem['id'])
    .filter((id): id is string => typeof id === 'string');
  if (lineItemIds.length === 0) {
    throw new Error(`Created fulfillment order has no line items: ${JSON.stringify(captureResult.response.payload)}`);
  }

  return {
    id: order['id'],
    name: typeof order['name'] === 'string' ? order['name'] : null,
    fulfillmentOrderId: fulfillmentOrder['id'] as string,
    fulfillmentOrderLineItemIds: lineItemIds,
  };
}

function hydrationCallFromOrderCreate(captureResult: GraphqlCapture): RecordedUpstreamCall {
  const data = readObject(captureResult.response.payload.data);
  const orderCreate = readObject(data?.['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  if (!order) {
    throw new Error(
      `Unable to build fulfillment order hydration cassette: ${JSON.stringify(captureResult.response.payload)}`,
    );
  }
  const fulfillmentOrder = getFulfillmentOrder(order);
  const fulfillmentOrderId = fulfillmentOrder['id'];
  if (typeof fulfillmentOrderId !== 'string') {
    throw new Error(`Hydration fulfillment order has no id: ${JSON.stringify(fulfillmentOrder)}`);
  }
  return {
    operationName: 'ShippingFulfillmentOrderHydrate',
    variables: { id: fulfillmentOrderId },
    query: trimGraphql(hydrateFulfillmentOrderQuery),
    response: {
      status: captureResult.response.status,
      body: {
        data: {
          fulfillmentOrder,
        },
      },
    },
  };
}

async function createTrackedOrder(
  label: string,
  lineItems: Array<{ variantId: string; quantity: number; title: string }>,
): Promise<{ order: CreatedOrder; capture: GraphqlCapture }> {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const captureResult = await capture(orderCreateMutation, {
    order: {
      email: `har-559-${label}-${stamp}@example.com`,
      note: `HAR-559 fulfillment-order split multi ${label} ${stamp}`,
      tags: ['har-559', 'fulfillment-order-split-multi', label],
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
      lineItems: lineItems.map((lineItem) => ({
        variantId: lineItem.variantId,
        title: `${lineItem.title} ${stamp}`,
        quantity: lineItem.quantity,
        priceSet: {
          shopMoney: {
            amount: '20.00',
            currencyCode: 'USD',
          },
        },
        requiresShipping: true,
        taxable: true,
      })),
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  });
  return { order: asCreatedOrder(captureResult), capture: captureResult };
}

function readSplitMergeInputs(splitCapture: GraphqlCapture): JsonRecord[] {
  const data = readObject(splitCapture.response.payload.data);
  const payload = readObject(data?.['fulfillmentOrderSplit']);
  const splitResults = Array.isArray(payload?.['fulfillmentOrderSplits'])
    ? (payload['fulfillmentOrderSplits'] as unknown[])
    : [];
  return splitResults
    .map((rawResult) => {
      const result = readObject(rawResult);
      const fulfillmentOrder = readObject(result?.['fulfillmentOrder']);
      const remainingFulfillmentOrder = readObject(result?.['remainingFulfillmentOrder']);
      return [fulfillmentOrder?.['id'], remainingFulfillmentOrder?.['id']].filter(
        (id): id is string => typeof id === 'string',
      );
    })
    .filter((ids) => ids.length > 1)
    .map((ids) => ({
      mergeIntents: ids.map((fulfillmentOrderId) => ({ fulfillmentOrderId })),
    }));
}

async function cleanupOrder(order: CreatedOrder): Promise<GraphqlCapture> {
  return capture(orderCancelMutation, {
    orderId: order.id,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
}

const startedAt = new Date().toISOString();
const cleanup: GraphqlCapture[] = [];
const orderA = await createTrackedOrder('order-a', [
  { variantId: physicalVariantA, quantity: 2, title: 'HAR-559 split A item 1' },
  { variantId: physicalVariantB, quantity: 3, title: 'HAR-559 split A item 2' },
]);
const orderB = await createTrackedOrder('order-b', [
  { variantId: physicalVariantA, quantity: 3, title: 'HAR-559 split B item' },
]);

const emptyLineItems = await capture(splitMutation, {
  fulfillmentOrderSplits: [
    {
      fulfillmentOrderId: orderA.order.fulfillmentOrderId,
      fulfillmentOrderLineItems: [],
    },
  ],
});

const zeroQuantity = await capture(splitMutation, {
  fulfillmentOrderSplits: [
    {
      fulfillmentOrderId: orderA.order.fulfillmentOrderId,
      fulfillmentOrderLineItems: [
        {
          id: orderA.order.fulfillmentOrderLineItemIds[0],
          quantity: 0,
        },
      ],
    },
  ],
});

const unknownFulfillmentOrderId = 'gid://shopify/FulfillmentOrder/999999999999999';
const unknownFulfillmentOrder = await capture(splitMutation, {
  fulfillmentOrderSplits: [
    {
      fulfillmentOrderId: unknownFulfillmentOrderId,
      fulfillmentOrderLineItems: [
        {
          id: orderA.order.fulfillmentOrderLineItemIds[0],
          quantity: 1,
        },
      ],
    },
  ],
});

const multiSplit = await capture(splitMutation, {
  fulfillmentOrderSplits: [
    {
      fulfillmentOrderId: orderA.order.fulfillmentOrderId,
      fulfillmentOrderLineItems: [
        {
          id: orderA.order.fulfillmentOrderLineItemIds[0],
          quantity: 1,
        },
        {
          id: orderA.order.fulfillmentOrderLineItemIds[1],
          quantity: 1,
        },
      ],
    },
    {
      fulfillmentOrderId: orderB.order.fulfillmentOrderId,
      fulfillmentOrderLineItems: [
        {
          id: orderB.order.fulfillmentOrderLineItemIds[0],
          quantity: 2,
        },
      ],
    },
  ],
});

const mergeInputs = readSplitMergeInputs(multiSplit);
const merge =
  mergeInputs.length > 0 ? await capture(mergeMutation, { fulfillmentOrderMergeInputs: mergeInputs }) : null;
cleanup.push(await cleanupOrder(orderA.order));
cleanup.push(await cleanupOrder(orderB.order));

const output = {
  metadata: {
    issue: 'HAR-559',
    capturedAt: new Date().toISOString(),
    startedAt,
    storeDomain,
    apiVersion,
    scopedRoots: ['fulfillmentOrderSplit'],
    createdOrders: [orderA.order, orderB.order],
    cleanupMergeInputs: mergeInputs,
  },
  captures: {
    orderACreate: orderA.capture,
    orderBCreate: orderB.capture,
    emptyLineItems,
    zeroQuantity,
    unknownFulfillmentOrder,
    multiSplit,
  },
  cleanup: {
    merge,
    orderCancels: cleanup,
  },
  upstreamCalls: [hydrationCallFromOrderCreate(orderA.capture), hydrationCallFromOrderCreate(orderB.capture)],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

console.log(`Captured fulfillment-order split multi fixture: ${outputPath}`);
