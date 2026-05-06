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
  fulfillmentOrderLineItemId: string;
};

type SplitPair = {
  primaryFulfillmentOrderId: string;
  primaryLineItemId: string;
  siblingFulfillmentOrderId: string;
  siblingLineItemId: string | null;
};

const scenarioId = 'fulfillment-order-merge-validation';
const physicalVariantId = 'gid://shopify/ProductVariant/48540157378793';
const unknownFulfillmentOrderId = 'gid://shopify/FulfillmentOrder/999999999999999';
const unknownFulfillmentOrderLineItemId = 'gid://shopify/FulfillmentOrderLineItem/999999999999999';

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

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderMergeValidationFields on FulfillmentOrder {
    id
    status
    requestStatus
    fulfillBy
    lineItems(first: 10) {
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
  mutation FulfillmentOrderMergeValidationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 10) {
          nodes {
            ...FulfillmentOrderMergeValidationFields
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
  mutation FulfillmentOrderMergeValidationOrderCancel(
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

const orderReadQuery = `#graphql
  ${fulfillmentOrderFields}
  query FulfillmentOrderMergeValidationOrderRead($id: ID!) {
    order(id: $id) {
      id
      name
      displayFulfillmentStatus
      fulfillmentOrders(first: 10) {
        nodes {
          ...FulfillmentOrderMergeValidationFields
        }
      }
    }
  }
`;

const splitMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderMergeValidationSplit($fulfillmentOrderSplits: [FulfillmentOrderSplitInput!]!) {
    fulfillmentOrderSplit(fulfillmentOrderSplits: $fulfillmentOrderSplits) {
      fulfillmentOrderSplits {
        fulfillmentOrder {
          ...FulfillmentOrderMergeValidationFields
        }
        remainingFulfillmentOrder {
          ...FulfillmentOrderMergeValidationFields
        }
        replacementFulfillmentOrder {
          ...FulfillmentOrderMergeValidationFields
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
  mutation FulfillmentOrderMergeValidation($fulfillmentOrderMergeInputs: [FulfillmentOrderMergeInput!]!) {
    fulfillmentOrderMerge(fulfillmentOrderMergeInputs: $fulfillmentOrderMergeInputs) {
      fulfillmentOrderMerges {
        fulfillmentOrder {
          id
          status
          fulfillBy
          lineItems(first: 10) {
            nodes {
              id
              totalQuantity
              remainingQuantity
            }
          }
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

const fulfillmentOrderCancelMutation = `#graphql
  mutation FulfillmentOrderMergeValidationCancel($id: ID!) {
    fulfillmentOrderCancel(id: $id) {
      fulfillmentOrder {
        id
        status
        requestStatus
      }
      replacementFulfillmentOrder {
        id
        status
        requestStatus
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

function readFirstLineItemId(fulfillmentOrder: JsonRecord): string | null {
  const lineItem = readNodes(readObject(fulfillmentOrder['lineItems']))[0];
  return typeof lineItem?.['id'] === 'string' ? lineItem['id'] : null;
}

function getFirstFulfillmentOrder(order: JsonRecord): JsonRecord {
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

  const fulfillmentOrder = getFirstFulfillmentOrder(order);
  const lineItemId = readFirstLineItemId(fulfillmentOrder);
  if (!lineItemId) {
    throw new Error(`Created fulfillment order has no line item: ${JSON.stringify(fulfillmentOrder)}`);
  }

  return {
    id: order['id'],
    name: typeof order['name'] === 'string' ? order['name'] : null,
    fulfillmentOrderId: fulfillmentOrder['id'] as string,
    fulfillmentOrderLineItemId: lineItemId,
  };
}

function readSplitPair(splitCapture: GraphqlCapture): SplitPair {
  const data = readObject(splitCapture.response.payload.data);
  const payload = readObject(data?.['fulfillmentOrderSplit']);
  const userErrors = payload?.['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`Unable to split disposable fulfillment order: ${JSON.stringify(userErrors)}`);
  }
  const splitResult = Array.isArray(payload?.['fulfillmentOrderSplits'])
    ? readObject(payload['fulfillmentOrderSplits'][0])
    : null;
  const fulfillmentOrder = readObject(splitResult?.['fulfillmentOrder']);
  const remainingFulfillmentOrder = readObject(splitResult?.['remainingFulfillmentOrder']);
  const fulfillmentOrderId = fulfillmentOrder?.['id'];
  const remainingFulfillmentOrderId = remainingFulfillmentOrder?.['id'];
  if (
    !fulfillmentOrder ||
    !remainingFulfillmentOrder ||
    typeof fulfillmentOrderId !== 'string' ||
    typeof remainingFulfillmentOrderId !== 'string'
  ) {
    throw new Error(`Split response did not return two fulfillment orders: ${JSON.stringify(payload)}`);
  }

  const primaryLineItemId = readFirstLineItemId(fulfillmentOrder);
  if (!primaryLineItemId) {
    throw new Error(`Split primary fulfillment order has no line item: ${JSON.stringify(fulfillmentOrder)}`);
  }

  return {
    primaryFulfillmentOrderId: fulfillmentOrderId,
    primaryLineItemId,
    siblingFulfillmentOrderId: remainingFulfillmentOrderId,
    siblingLineItemId: readFirstLineItemId(remainingFulfillmentOrder ?? {}),
  };
}

function upstreamCallFromOrderRead(orderRead: GraphqlCapture): RecordedUpstreamCall {
  return {
    operationName: 'FulfillmentOrderMergeValidationOrderRead',
    variables: orderRead.variables,
    query: trimGraphql(orderReadQuery),
    response: {
      status: orderRead.response.status,
      body: orderRead.response.payload as JsonRecord,
    },
  };
}

async function createTrackedOrder(label: string): Promise<{ order: CreatedOrder; create: GraphqlCapture }> {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const create = await capture(orderCreateMutation, {
    order: {
      email: `fulfillment-order-merge-validation-${label}-${stamp}@example.com`,
      note: `fulfillment-order merge validation ${label} ${stamp}`,
      tags: ['fulfillment-order-merge-validation', label],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'Merge',
        lastName: 'Validation',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: physicalVariantId,
          title: `fulfillment-order merge validation ${label} ${stamp}`,
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
  });
  return { order: asCreatedOrder(create), create };
}

async function splitOrder(order: CreatedOrder): Promise<{ split: GraphqlCapture; pair: SplitPair }> {
  const split = await capture(splitMutation, {
    fulfillmentOrderSplits: [
      {
        fulfillmentOrderId: order.fulfillmentOrderId,
        fulfillmentOrderLineItems: [
          {
            id: order.fulfillmentOrderLineItemId,
            quantity: 1,
          },
        ],
      },
    ],
  });
  return { split, pair: readSplitPair(split) };
}

async function readOrder(order: CreatedOrder): Promise<GraphqlCapture> {
  return capture(orderReadQuery, { id: order.id });
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
const createdOrders: CreatedOrder[] = [];
const cleanedOrderIds = new Set<string>();

async function createTrackedCleanupOrder(label: string): Promise<{ order: CreatedOrder; create: GraphqlCapture }> {
  const created = await createTrackedOrder(label);
  createdOrders.push(created.order);
  return created;
}

async function cleanupCreatedOrders(): Promise<void> {
  for (const order of createdOrders) {
    if (cleanedOrderIds.has(order.id)) continue;
    cleanedOrderIds.add(order.id);
    try {
      cleanup.push(await cleanupOrder(order));
    } catch (error) {
      console.error(`Failed to cleanup disposable order ${order.id}:`, error);
    }
  }
}

async function main(): Promise<void> {
  try {
    const validationOrder = await createTrackedCleanupOrder('validation');
    const validationSplit = await splitOrder(validationOrder.order);
    const validationOrderRead = await readOrder(validationOrder.order);

    const missing = await capture(mergeMutation, {
      fulfillmentOrderMergeInputs: [
        {
          mergeIntents: [
            { fulfillmentOrderId: validationSplit.pair.primaryFulfillmentOrderId },
            { fulfillmentOrderId: unknownFulfillmentOrderId },
          ],
        },
      ],
    });

    const zeroQuantity = await capture(mergeMutation, {
      fulfillmentOrderMergeInputs: [
        {
          mergeIntents: [
            {
              fulfillmentOrderId: validationSplit.pair.primaryFulfillmentOrderId,
              fulfillmentOrderLineItems: [
                {
                  id: validationSplit.pair.primaryLineItemId,
                  quantity: 0,
                },
              ],
            },
            { fulfillmentOrderId: validationSplit.pair.siblingFulfillmentOrderId },
          ],
        },
      ],
    });

    const invalidLineItem = await capture(mergeMutation, {
      fulfillmentOrderMergeInputs: [
        {
          mergeIntents: [
            {
              fulfillmentOrderId: validationSplit.pair.primaryFulfillmentOrderId,
              fulfillmentOrderLineItems: [
                {
                  id: unknownFulfillmentOrderLineItemId,
                  quantity: 1,
                },
              ],
            },
            { fulfillmentOrderId: validationSplit.pair.siblingFulfillmentOrderId },
          ],
        },
      ],
    });

    const excessiveQuantity = await capture(mergeMutation, {
      fulfillmentOrderMergeInputs: [
        {
          mergeIntents: [
            {
              fulfillmentOrderId: validationSplit.pair.primaryFulfillmentOrderId,
              fulfillmentOrderLineItems: [
                {
                  id: validationSplit.pair.primaryLineItemId,
                  quantity: 999,
                },
              ],
            },
            { fulfillmentOrderId: validationSplit.pair.siblingFulfillmentOrderId },
          ],
        },
      ],
    });

    const nonOpenOrder = await createTrackedCleanupOrder('non-open');
    const nonOpenSplit = await splitOrder(nonOpenOrder.order);
    const cancelSibling = await capture(fulfillmentOrderCancelMutation, {
      id: nonOpenSplit.pair.siblingFulfillmentOrderId,
    });
    const nonOpenOrderRead = await readOrder(nonOpenOrder.order);
    const nonOpenMerge = await capture(mergeMutation, {
      fulfillmentOrderMergeInputs: [
        {
          mergeIntents: [
            { fulfillmentOrderId: nonOpenSplit.pair.primaryFulfillmentOrderId },
            { fulfillmentOrderId: nonOpenSplit.pair.siblingFulfillmentOrderId },
          ],
        },
      ],
    });

    const happyOrderRead = validationOrderRead;
    const happySplit = validationSplit;
    const happyMerge = await capture(mergeMutation, {
      fulfillmentOrderMergeInputs: [
        {
          mergeIntents: [
            { fulfillmentOrderId: happySplit.pair.primaryFulfillmentOrderId },
            { fulfillmentOrderId: happySplit.pair.siblingFulfillmentOrderId },
          ],
        },
      ],
    });
    const happyAfterMergeOrderRead = await readOrder(validationOrder.order);

    await cleanupCreatedOrders();

    const output = {
      metadata: {
        capturedAt: new Date().toISOString(),
        startedAt,
        storeDomain,
        apiVersion,
        scopedRoots: ['fulfillmentOrderMerge'],
        createdOrders,
        unknownFulfillmentOrderId,
        unknownFulfillmentOrderLineItemId,
      },
      workflows: {
        validation: {
          create: validationOrder.create,
          split: validationSplit.split,
          pair: validationSplit.pair,
          orderRead: validationOrderRead,
          missing,
          zeroQuantity,
          invalidLineItem,
          excessiveQuantity,
        },
        nonOpen: {
          create: nonOpenOrder.create,
          split: nonOpenSplit.split,
          pair: nonOpenSplit.pair,
          cancelSibling,
          orderRead: nonOpenOrderRead,
          merge: nonOpenMerge,
        },
        happy: {
          create: validationOrder.create,
          split: validationSplit.split,
          pair: validationSplit.pair,
          orderRead: happyOrderRead,
          merge: happyMerge,
          afterMergeOrderRead: happyAfterMergeOrderRead,
        },
      },
      cleanup,
      upstreamCalls: [upstreamCallFromOrderRead(validationOrderRead), upstreamCallFromOrderRead(nonOpenOrderRead)],
    };

    await mkdir(outputDir, { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

    console.log(`Captured fulfillment-order merge validation fixture: ${outputPath}`);
  } finally {
    await cleanupCreatedOrders();
  }
}

await main();
