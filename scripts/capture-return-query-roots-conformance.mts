/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
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
type CleanupCapture = {
  orderCancel?: GraphqlCapture;
  orderDelete?: GraphqlCapture;
  errors: string[];
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

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'return-query-roots-recorded.json');
const requestDir = path.join('config', 'parity-requests', 'orders');
const specPath = path.join('config', 'parity-specs', 'orders', 'return-query-roots-recorded.json');

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

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

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readNodes(value: unknown): JsonRecord[] {
  return readArray(readRecord(value)?.['nodes'])
    .map(readRecord)
    .filter((node): node is JsonRecord => node !== null);
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

function payloadRoot(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  const payload = readRecord(captureResult.response.payload) ?? {};
  const data = readRecord(payload['data']) ?? {};
  return readRecord(data[rootName]) ?? {};
}

function requireNoGraphqlErrors(captureResult: GraphqlCapture, label: string): void {
  const payload = readRecord(captureResult.response.payload) ?? {};
  if (payload['errors'] !== undefined) {
    throw new Error(`Unexpected ${label} GraphQL errors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function requireEmptyUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  requireNoGraphqlErrors(captureResult, rootName);
  const root = payloadRoot(captureResult, rootName);
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`Unexpected ${rootName} userErrors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function requireRawNoGraphqlErrors(captureResult: ConformanceGraphqlResult, label: string): void {
  if (readRecord(captureResult.payload)?.['errors'] !== undefined) {
    throw new Error(`Unexpected ${label} GraphQL errors: ${JSON.stringify(captureResult.payload)}`);
  }
}

function firstFulfillmentOrder(order: JsonRecord): JsonRecord {
  return readNodes(order['fulfillmentOrders'])[0] ?? {};
}

function firstFulfillmentLineItem(order: JsonRecord): JsonRecord {
  return readNodes(readRecord(readArray(order['fulfillments'])[0])?.['fulfillmentLineItems'])[0] ?? {};
}

function requireReturnableLine(captureResult: GraphqlCapture, fulfillmentLineItemId: string): void {
  requireNoGraphqlErrors(captureResult, 'returnableFulfillments');
  const nodes = readNodes(payloadRoot(captureResult, 'returnableFulfillments'));
  const lineItems = nodes.flatMap((node) => readNodes(node['returnableFulfillmentLineItems']));
  const match = lineItems.find((line) => {
    const fulfillmentLineItem = readRecord(line['fulfillmentLineItem']);
    return fulfillmentLineItem?.['id'] === fulfillmentLineItemId && line['quantity'] === 2;
  });
  if (!match) {
    throw new Error(
      `returnableFulfillments did not expose fulfilled line ${fulfillmentLineItemId}: ${JSON.stringify(
        captureResult.response.payload,
      )}`,
    );
  }
}

function requireCalculatedLine(captureResult: GraphqlCapture, fulfillmentLineItemId: string): void {
  requireNoGraphqlErrors(captureResult, 'returnCalculate');
  const lineItems = readArray(payloadRoot(captureResult, 'returnCalculate')['returnLineItems']).map(readRecord);
  const match = lineItems.find((line) => {
    const fulfillmentLineItem = readRecord(line?.['fulfillmentLineItem']);
    return fulfillmentLineItem?.['id'] === fulfillmentLineItemId && line?.['quantity'] === 1;
  });
  if (!match) {
    throw new Error(
      `returnCalculate did not expose calculated line ${fulfillmentLineItemId}: ${JSON.stringify(
        captureResult.response.payload,
      )}`,
    );
  }
}

const orderFields = `#graphql
  fragment ReturnQueryRootsOrderFields on Order {
    id
    name
    createdAt
    updatedAt
    currencyCode
    presentmentCurrencyCode
    displayFinancialStatus
    displayFulfillmentStatus
    totalPriceSet {
      shopMoney {
        amount
        currencyCode
      }
      presentmentMoney {
        amount
        currencyCode
      }
    }
    currentTotalPriceSet {
      shopMoney {
        amount
        currencyCode
      }
      presentmentMoney {
        amount
        currencyCode
      }
    }
    lineItems(first: 5) {
      nodes {
        id
        title
        quantity
        currentQuantity
        originalUnitPriceSet {
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
    fulfillments(first: 5) {
      id
      status
      displayStatus
      createdAt
      updatedAt
      fulfillmentLineItems(first: 5) {
        nodes {
          id
          quantity
          lineItem {
            id
            title
            originalUnitPriceSet {
              shopMoney {
                amount
                currencyCode
              }
              presentmentMoney {
                amount
                currencyCode
              }
            }
            variant {
              id
            }
          }
        }
      }
    }
    fulfillmentOrders(first: 5) {
      nodes {
        id
        status
        requestStatus
        lineItems(first: 5) {
          nodes {
            id
            totalQuantity
            remainingQuantity
            lineItem {
              id
              title
            }
          }
        }
      }
    }
    returns(first: 5) {
      nodes {
        id
        name
        status
        totalQuantity
        returnLineItems(first: 5) {
          nodes {
            ... on ReturnLineItem {
              id
              quantity
              fulfillmentLineItem {
                id
              }
            }
          }
        }
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${orderFields}
  mutation ReturnQueryRootsOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...ReturnQueryRootsOrderFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation ReturnQueryRootsFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        id
        status
        displayStatus
        createdAt
        updatedAt
        fulfillmentLineItems(first: 5) {
          nodes {
            id
            quantity
            lineItem {
              id
              title
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

const orderReadQuery = `#graphql
  ${orderFields}
  query ReturnQueryRootsOrderRead($id: ID!) {
    order(id: $id) {
      ...ReturnQueryRootsOrderFields
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation ReturnQueryRootsOrderCancel(
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

const orderDeleteMutation = `#graphql
  mutation ReturnQueryRootsOrderDelete($orderId: ID!) {
    orderDelete(orderId: $orderId) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const returnableFulfillmentsQuery = await readRequest('returnable-fulfillments-recorded.graphql');
const returnCalculateQuery = await readRequest('return-calculate-recorded.graphql');
const returnOrderHydrateQuery = await readRequest('return-order-hydrate.graphql');
const returnCalculationOrderHydrateQuery = await readRequest('return-calculation-order-hydrate.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

const orderVariables = {
  order: {
    email: `return-query-roots-${stamp}@example.com`,
    note: `return query roots capture ${stamp}`,
    tags: ['return-query-roots', stamp],
    test: true,
    currency: 'USD',
    lineItems: [
      {
        variantId: 'gid://shopify/ProductVariant/48540157378793',
        title: `Return query roots item ${stamp}`,
        quantity: 2,
        priceSet: {
          shopMoney: {
            amount: '20.00',
            currencyCode: 'USD',
          },
        },
        requiresShipping: true,
        taxable: false,
      },
    ],
  },
  options: {
    inventoryBehaviour: 'BYPASS',
    sendReceipt: false,
    sendFulfillmentReceipt: false,
  },
};

async function cleanupOrder(orderId: string): Promise<CleanupCapture> {
  const cleanup: CleanupCapture = { errors: [] };
  try {
    cleanup.orderCancel = await capture(orderCancelMutation, {
      orderId,
      reason: 'OTHER',
      notifyCustomer: false,
      restock: true,
    });
  } catch (error) {
    cleanup.errors.push(`orderCancel cleanup failed: ${(error as Error).message}`);
  }
  try {
    cleanup.orderDelete = await capture(orderDeleteMutation, { orderId });
  } catch (error) {
    cleanup.errors.push(`orderDelete cleanup failed: ${(error as Error).message}`);
  }
  return cleanup;
}

let orderIdForCleanup: string | null = null;
let cleanupStarted = false;

try {
  const orderCreate = await capture(orderCreateMutation, orderVariables);
  requireEmptyUserErrors(orderCreate, 'orderCreate');

  const createdOrder = readRecord(payloadRoot(orderCreate, 'orderCreate')['order']) ?? {};
  const orderId = requireString(createdOrder['id'], 'created order id');
  orderIdForCleanup = orderId;
  const fulfillmentOrder = firstFulfillmentOrder(createdOrder);
  const fulfillmentOrderId = requireString(fulfillmentOrder['id'], 'created fulfillment order id');
  const fulfillmentOrderLineItem = readNodes(fulfillmentOrder['lineItems'])[0] ?? {};
  const fulfillmentOrderLineItemId = requireString(
    fulfillmentOrderLineItem['id'],
    'created fulfillment order line item id',
  );

  const fulfillmentCreate = await capture(fulfillmentCreateMutation, {
    fulfillment: {
      notifyCustomer: false,
      trackingInfo: {
        number: `RETURN-QUERY-FULFILL-${stamp}`,
        url: `https://example.com/track/return-query-roots-${stamp}`,
        company: 'Hermes Carrier',
      },
      lineItemsByFulfillmentOrder: [
        {
          fulfillmentOrderId,
          fulfillmentOrderLineItems: [
            {
              id: fulfillmentOrderLineItemId,
              quantity: 2,
            },
          ],
        },
      ],
    },
    message: `return query roots fulfillment ${stamp}`,
  });
  requireEmptyUserErrors(fulfillmentCreate, 'fulfillmentCreate');

  const orderReadAfterFulfillment = await capture(orderReadQuery, { id: orderId });
  requireNoGraphqlErrors(orderReadAfterFulfillment, 'orderReadAfterFulfillment');
  const orderAfterFulfillment = readRecord(readRecord(orderReadAfterFulfillment.response.payload)?.['data'])?.['order'];
  const fulfillmentLineItem = firstFulfillmentLineItem(readRecord(orderAfterFulfillment) ?? {});
  const fulfillmentLineItemId = requireString(fulfillmentLineItem['id'], 'fulfilled fulfillment line item id');

  const returnOrderHydrate = await runGraphqlRequest(returnOrderHydrateQuery, { id: orderId });
  requireRawNoGraphqlErrors(returnOrderHydrate, 'return-order hydrate');
  const returnCalculationOrderHydrate = await runGraphqlRequest(returnCalculationOrderHydrateQuery, { id: orderId });
  requireRawNoGraphqlErrors(returnCalculationOrderHydrate, 'return-calculation-order hydrate');

  const returnableFulfillments = await capture(returnableFulfillmentsQuery, { orderId });
  requireReturnableLine(returnableFulfillments, fulfillmentLineItemId);

  const returnCalculate = await capture(returnCalculateQuery, {
    input: {
      orderId,
      returnLineItems: [
        {
          fulfillmentLineItemId,
          quantity: 1,
        },
      ],
    },
  });
  requireCalculatedLine(returnCalculate, fulfillmentLineItemId);

  cleanupStarted = true;
  const cleanup = await cleanupOrder(orderId);

  await writeJson(fixturePath, {
    capturedAt: new Date().toISOString(),
    apiVersion,
    storeDomain,
    source: 'live-shopify-admin-graphql',
    notes:
      'Live public Admin GraphQL capture for returnableFulfillments and returnCalculate on a disposable fulfilled order. The parity replay creates and fulfills a local staged order through public GraphQL mutations before exercising the query roots.',
    setup: {
      orderCreate,
      fulfillmentCreate,
      orderReadAfterFulfillment,
    },
    returnableFulfillments,
    returnCalculate,
    cleanup,
    upstreamCalls: [
      {
        operationName: 'OrdersReturnOrderHydrate',
        variables: { id: orderId },
        query: returnOrderHydrateQuery,
        response: {
          status: returnOrderHydrate.status,
          body: returnOrderHydrate.payload,
        },
      },
      {
        operationName: 'ReturnCalculationOrderHydrate',
        variables: { id: orderId },
        query: returnCalculationOrderHydrateQuery,
        response: {
          status: returnCalculationOrderHydrate.status,
          body: returnCalculationOrderHydrate.payload,
        },
      },
    ],
  });

  await writeJson(specPath, {
    scenarioId: 'return-query-roots-recorded',
    operationNames: ['orderCreate', 'fulfillmentCreate', 'returnableFulfillments', 'returnCalculate'],
    scenarioStatus: 'captured',
    assertionKinds: ['runtime-staging', 'payload-shape', 'calculation-parity', 'no-upstream-passthrough'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentCapturePath: '$.setup.orderCreate.query',
      variablesCapturePath: '$.setup.orderCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live 2026-04 return query-root evidence for a fulfilled order. Replay stages the order and fulfillment locally, then verifies returnableFulfillments and returnCalculate derive quantities, line-item identity, and money from the staged order graph instead of passing the reads through to Shopify.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'order-create-query-root-setup',
          capturePath: '$.setup.orderCreate.response.payload.data.orderCreate.order',
          proxyPath: '$.data.orderCreate.order',
          selectedPaths: [
            '$.displayFulfillmentStatus',
            '$.fulfillmentOrders.nodes[0].status',
            '$.fulfillmentOrders.nodes[0].requestStatus',
            '$.fulfillmentOrders.nodes[0].lineItems.nodes[0].totalQuantity',
            '$.fulfillmentOrders.nodes[0].lineItems.nodes[0].remainingQuantity',
            '$.fulfillmentOrders.nodes[0].lineItems.nodes[0].lineItem.title',
          ],
        },
        {
          name: 'fulfillment-create-query-root-setup',
          capturePath: '$.setup.fulfillmentCreate.response.payload.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          selectedPaths: [
            '$.fulfillment.status',
            '$.fulfillment.fulfillmentLineItems.nodes[0].quantity',
            '$.fulfillment.fulfillmentLineItems.nodes[0].lineItem.title',
            '$.userErrors',
          ],
          proxyRequest: {
            documentCapturePath: '$.setup.fulfillmentCreate.query',
            variables: {
              fulfillment: {
                notifyCustomer: {
                  fromCapturePath: '$.setup.fulfillmentCreate.variables.fulfillment.notifyCustomer',
                },
                trackingInfo: {
                  fromCapturePath: '$.setup.fulfillmentCreate.variables.fulfillment.trackingInfo',
                },
                lineItemsByFulfillmentOrder: [
                  {
                    fulfillmentOrderId: {
                      fromPrimaryProxyPath: '$.data.orderCreate.order.fulfillmentOrders.nodes[0].id',
                    },
                    fulfillmentOrderLineItems: [
                      {
                        id: {
                          fromPrimaryProxyPath:
                            '$.data.orderCreate.order.fulfillmentOrders.nodes[0].lineItems.nodes[0].id',
                        },
                        quantity: {
                          fromCapturePath:
                            '$.setup.fulfillmentCreate.variables.fulfillment.lineItemsByFulfillmentOrder[0].fulfillmentOrderLineItems[0].quantity',
                        },
                      },
                    ],
                  },
                ],
              },
              message: {
                fromCapturePath: '$.setup.fulfillmentCreate.variables.message',
              },
            },
            apiVersion,
          },
        },
        {
          name: 'returnable-fulfillments-staged-order',
          capturePath: '$.returnableFulfillments.response.payload.data.returnableFulfillments',
          proxyPath: '$.data.returnableFulfillments',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/returnable-fulfillments-recorded.graphql',
            variables: {
              orderId: {
                fromPrimaryProxyPath: '$.data.orderCreate.order.id',
              },
            },
            apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.nodes[0].fulfillment.id',
              matcher: 'exact-string:gid://shopify/Fulfillment/1?shopify-draft-proxy=synthetic',
              reason: 'The replay must return the locally staged fulfillment, proving the read did not pass through.',
            },
            {
              path: '$.nodes[0].returnableFulfillmentLineItems.nodes[0].fulfillmentLineItem.id',
              matcher: 'exact-string:gid://shopify/FulfillmentLineItem/1',
              reason: 'The replay must return the locally staged fulfillment line item.',
            },
            {
              path: '$.nodes[0].returnableFulfillmentLineItems.nodes[0].fulfillmentLineItem.lineItem.id',
              matcher: 'exact-string:gid://shopify/LineItem/1',
              reason: 'The replay must keep the local line item linked to the returnable fulfillment line item.',
            },
          ],
        },
        {
          name: 'return-calculate-staged-fulfilled-line',
          capturePath: '$.returnCalculate.response.payload.data.returnCalculate',
          proxyPath: '$.data.returnCalculate',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/return-calculate-recorded.graphql',
            variables: {
              input: {
                orderId: {
                  fromPrimaryProxyPath: '$.data.orderCreate.order.id',
                },
                returnLineItems: [
                  {
                    fulfillmentLineItemId: {
                      fromProxyResponse: 'fulfillment-create-query-root-setup',
                      path: '$.data.fulfillmentCreate.fulfillment.fulfillmentLineItems.nodes[0].id',
                    },
                    quantity: {
                      fromCapturePath: '$.returnCalculate.variables.input.returnLineItems[0].quantity',
                    },
                  },
                ],
              },
            },
            apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.returnLineItems[0].fulfillmentLineItem.id',
              matcher: 'exact-string:gid://shopify/FulfillmentLineItem/1',
              reason: 'The calculation must be based on the locally staged fulfillment line item.',
            },
            {
              path: '$.returnLineItems[0].fulfillmentLineItem.lineItem.id',
              matcher: 'exact-string:gid://shopify/LineItem/1',
              reason: 'The calculation must preserve the staged line item identity.',
            },
          ],
        },
      ],
    },
  });

  console.log(
    JSON.stringify(
      {
        fixturePath,
        specPath,
        orderId,
        fulfillmentLineItemId,
        cleanupErrors: cleanup.errors,
      },
      null,
      2,
    ),
  );
} finally {
  if (orderIdForCleanup !== null && !cleanupStarted) {
    const cleanup = await cleanupOrder(orderIdForCleanup);
    console.error(JSON.stringify({ cleanupAfterFailure: cleanup }, null, 2));
  }
}
