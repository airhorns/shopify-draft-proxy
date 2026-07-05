/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, any>;
type GraphqlStep = {
  query: string;
  variables: JsonRecord;
  status: number;
  response: JsonRecord;
};

const selectedPaths = [
  '$.fulfillment.name',
  '$.fulfillment.status',
  '$.fulfillment.displayStatus',
  '$.fulfillment.trackingInfo',
  '$.fulfillment.fulfillmentLineItems.nodes[0].quantity',
  '$.fulfillment.fulfillmentLineItems.nodes[0].lineItem.title',
  '$.userErrors',
];

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const fixturePath = path.join(fixtureDir, 'fulfillment-create-name.json');
const requestPath = path.join('config', 'parity-requests', 'shipping-fulfillments', 'fulfillment-create-name.graphql');
const specPath = path.join('config', 'parity-specs', 'shipping-fulfillments', 'fulfillment-create-name.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphqlRequest: (query: string, variables?: Record<string, unknown>) => Promise<{ status: number; payload: any }>;
};

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, payload);
}

function requirePath<T>(value: T | null | undefined, label: string): T {
  if (value === null || value === undefined || value === '') {
    throw new Error(`Missing required capture value: ${label}`);
  }

  return value;
}

function requireNoUserErrors(payload: JsonRecord, pathLabel: string): void {
  const errors = pathLabel
    .split('.')
    .reduce<unknown>((current, segment) => (current as JsonRecord | undefined)?.[segment], payload);
  if (Array.isArray(errors) && errors.length === 0) return;
  throw new Error(`${pathLabel} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
}

function cleanGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(name: string, query: string, variables: JsonRecord = {}): Promise<GraphqlStep> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${name} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }

  return {
    query: cleanGraphql(query),
    variables,
    status: result.status,
    response: result.payload,
  };
}

async function captureAllowingUserErrors(
  name: string,
  query: string,
  variables: JsonRecord = {},
): Promise<GraphqlStep> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${name} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }

  return {
    query: cleanGraphql(query),
    variables,
    status: result.status,
    response: result.payload,
  };
}

const orderCreateMutation = `#graphql
mutation FulfillmentCreateNameOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
  orderCreate(order: $order, options: $options) {
    order {
      id
      name
      email
      createdAt
      updatedAt
      displayFinancialStatus
      displayFulfillmentStatus
      fulfillments(first: 5) {
        id
        name
        status
        displayStatus
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
                quantity
                fulfillableQuantity
              }
            }
          }
        }
      }
    }
    userErrors { field message }
  }
}
`;

const fulfillmentCreateMutation = `#graphql
mutation FulfillmentCreateName($fulfillment: FulfillmentInput!, $message: String) {
  fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
    fulfillment {
      id
      name
      status
      displayStatus
      trackingInfo(first: 5) {
        number
        url
        company
      }
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

const orderCancelMutation = `#graphql
mutation FulfillmentCreateNameOrderCancel(
  $orderId: ID!
  $reason: OrderCancelReason!
  $notifyCustomer: Boolean!
  $restock: Boolean!
) {
  orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
    job { id done }
    orderCancelUserErrors { field message code }
    userErrors { field message }
  }
}
`;

const orderDeleteMutation = `#graphql
mutation FulfillmentCreateNameOrderDelete($orderId: ID!) {
  orderDelete(orderId: $orderId) {
    deletedId
    userErrors { field message code }
  }
}
`;

// Byte-for-byte copy of the proxy's ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY
// (src/proxy/orders_payments_fulfillment.rs). On a cold fulfillmentCreate the
// proxy forwards this document to resolve the owning order from the supplied
// fulfillmentOrderId; recording its live response is what replaces the seeded
// order. Kept flush-left so trimGraphql leaves it identical to the constant.
const fulfillmentOrderHydrateQuery = `query ShippingFulfillmentOrderHydrate($id: ID!) {
    fulfillmentOrder(id: $id) {
      id
      status
      requestStatus
      fulfillAt
      fulfillBy
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
      merchantRequests(first: 10) {
        nodes {
          kind
          message
          requestOptions
        }
      }
      lineItems(first: 20) {
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
      order {
        id
        name
        displayFulfillmentStatus
      }
    }
  }`;

function orderVariables(stamp: number): JsonRecord {
  return {
    order: {
      email: `fulfillment-create-name-${stamp}@example.com`,
      note: `fulfillmentCreate name ${stamp}`,
      tags: ['parity-probe', 'fulfillment-create-name'],
      test: true,
      lineItems: [
        {
          title: 'fulfillmentCreate name sequence line',
          quantity: 2,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: true,
          taxable: false,
          sku: `FULFILLMENT-CREATE-NAME-${stamp}`,
        },
      ],
      transactions: [
        {
          kind: 'SALE',
          status: 'SUCCESS',
          gateway: 'manual',
          test: true,
          amountSet: {
            shopMoney: {
              amount: '20.00',
              currencyCode: 'CAD',
            },
          },
        },
      ],
    },
    options: null,
  };
}

function fulfillmentVariables(
  fulfillmentOrderId: string,
  fulfillmentOrderLineItemId: string,
  label: string,
  stamp: number,
): JsonRecord {
  const trackingNumber = `FCN-${label.toUpperCase()}-${stamp}`;
  return {
    fulfillment: {
      notifyCustomer: false,
      trackingInfo: {
        number: trackingNumber,
        url: `https://example.com/track/${trackingNumber}`,
        company: 'Hermes',
      },
      lineItemsByFulfillmentOrder: [
        {
          fulfillmentOrderId,
          fulfillmentOrderLineItems: [{ id: fulfillmentOrderLineItemId, quantity: 1 }],
        },
      ],
    },
    message: `fulfillmentCreate name ${label}`,
  };
}

async function cleanupOrder(orderId: string): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};
  cleanup['cancel'] = await captureAllowingUserErrors('cleanup.orderCancel', orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });
  cleanup['delete'] = await captureAllowingUserErrors('cleanup.orderDelete', orderDeleteMutation, { orderId });
  return cleanup;
}

const stamp = Date.now();
const orderIds: string[] = [];
const cleanup: JsonRecord = {};

try {
  const orderCreate = await capture('setup.orderCreate', orderCreateMutation, orderVariables(stamp));
  requireNoUserErrors(orderCreate.response, 'data.orderCreate.userErrors');
  const order = requirePath(orderCreate.response['data']?.['orderCreate']?.['order'], 'orderCreate.order');
  const orderId = String(requirePath(order['id'], 'order.id'));
  orderIds.push(orderId);
  const fulfillmentOrder = requirePath(order['fulfillmentOrders']?.['nodes']?.[0], 'order.fulfillmentOrders.nodes[0]');
  const fulfillmentOrderLineItem = requirePath(
    fulfillmentOrder['lineItems']?.['nodes']?.[0],
    'fulfillmentOrder.lineItems.nodes[0]',
  );
  const fulfillmentOrderId = String(requirePath(fulfillmentOrder['id'], 'fulfillmentOrder.id'));
  const fulfillmentOrderLineItemId = String(requirePath(fulfillmentOrderLineItem['id'], 'fulfillmentOrderLineItem.id'));

  // Record the cold-miss order hydrate the proxy forwards for this fulfillment
  // order in its full, unfulfilled state — before any fulfillmentCreate runs —
  // so de-seeded replay forwards+observes real store state.
  const fulfillmentOrderHydrate = await capture('setup.fulfillmentOrderHydrate', fulfillmentOrderHydrateQuery, {
    id: fulfillmentOrderId,
  });

  const firstFulfillmentCreate = await capture(
    'first.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables(fulfillmentOrderId, fulfillmentOrderLineItemId, 'first', stamp),
  );
  requireNoUserErrors(firstFulfillmentCreate.response, 'data.fulfillmentCreate.userErrors');

  const secondFulfillmentCreate = await capture(
    'second.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables(fulfillmentOrderId, fulfillmentOrderLineItemId, 'second', stamp),
  );
  requireNoUserErrors(secondFulfillmentCreate.response, 'data.fulfillmentCreate.userErrors');

  cleanup[orderId] = await cleanupOrder(orderId);

  const capturePayload = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    adminOrigin,
    apiVersion,
    setup: {
      orderCreate,
    },
    orderId,
    orderName: order['name'],
    fulfillmentOrderId,
    fulfillmentOrderLineItemId,
    firstFulfillmentCreate,
    secondFulfillmentCreate,
    cleanup,
    upstreamCalls: [
      {
        operationName: 'ShippingFulfillmentOrderHydrate',
        variables: fulfillmentOrderHydrate.variables,
        query: fulfillmentOrderHydrate.query,
        response: {
          status: fulfillmentOrderHydrate.status,
          body: fulfillmentOrderHydrate.response,
        },
      },
    ],
  };

  await writeJson(fixturePath, capturePayload);
  await writeText(requestPath, `${cleanGraphql(fulfillmentCreateMutation)}\n`);
  await writeJson(specPath, {
    scenarioId: 'fulfillment-create-name',
    operationNames: ['fulfillmentCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'mutation-lifecycle', 'runtime-staging'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: requestPath,
      variablesCapturePath: '$.firstFulfillmentCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'first-fulfillment-name',
          capturePath: '$.firstFulfillmentCreate.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          selectedPaths,
        },
        {
          name: 'second-fulfillment-name',
          capturePath: '$.secondFulfillmentCreate.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          selectedPaths,
          preserveProxyState: true,
          proxyRequest: {
            documentPath: requestPath,
            variables: { fromCapturePath: '$.secondFulfillmentCreate.variables' },
            apiVersion,
          },
        },
      ],
    },
    notes:
      'Captured public Admin API fulfillmentCreate Fulfillment.name reference numbers for two fulfillments on the same disposable order; names are the order name followed by -F1 and -F2.',
  });

  console.log(`Wrote ${fixturePath}`);
  console.log(`Wrote ${requestPath}`);
  console.log(`Wrote ${specPath}`);
} catch (error) {
  for (const orderId of [...orderIds].reverse()) {
    if (!cleanup[orderId]) {
      try {
        cleanup[orderId] = await cleanupOrder(orderId);
      } catch (cleanupError) {
        console.error(`Cleanup failed for ${orderId}:`, cleanupError);
      }
    }
  }
  throw error;
}
