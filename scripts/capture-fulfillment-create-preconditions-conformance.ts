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
type OrderSetup = {
  order: JsonRecord;
  fulfillmentOrder: JsonRecord;
  fulfillmentOrderLineItem: JsonRecord;
  create: GraphqlStep;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'fulfillment-create-preconditions.json');
const requestPath = path.join('config', 'parity-requests', 'orders', 'fulfillmentCreate-preconditions.graphql');
const specPath = path.join('config', 'parity-specs', 'orders', 'fulfillmentCreate-preconditions.json');

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

async function capture(name: string, query: string, variables: JsonRecord = {}): Promise<GraphqlStep> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${name} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }

  return {
    query: query.replace(/^#graphql\n/u, '').trim(),
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
    query: query.replace(/^#graphql\n/u, '').trim(),
    variables,
    status: result.status,
    response: result.payload,
  };
}

const orderCreateMutation = `#graphql
mutation FulfillmentCreatePreconditionsOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
  orderCreate(order: $order, options: $options) {
    order {
      id
      name
      email
      phone
      createdAt
      updatedAt
      closed
      closedAt
      cancelledAt
      cancelReason
      displayFinancialStatus
      displayFulfillmentStatus
      note
      tags
      fulfillments(first: 5) {
        id
        status
        displayStatus
        createdAt
        updatedAt
        trackingInfo { number url company }
      }
      fulfillmentOrders(first: 10) {
        nodes {
          id
          status
          requestStatus
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
      }
    }
    userErrors { field message }
  }
}
`;

const orderHydrateQuery = `#graphql
query OrdersFulfillmentOrderHydrate($id: ID!) {
  fulfillmentOrder(id: $id) {
    id
    order {
      id
      name
      email
      phone
      createdAt
      updatedAt
      closed
      closedAt
      cancelledAt
      cancelReason
      displayFinancialStatus
      displayFulfillmentStatus
      note
      tags
      fulfillments(first: 5) {
        id
        status
        displayStatus
        createdAt
        updatedAt
        trackingInfo { number url company }
      }
      fulfillmentOrders(first: 10) {
        nodes {
          id
          status
          requestStatus
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
      }
    }
  }
}
`;

const fulfillmentCreateMutation = `#graphql
mutation FulfillmentCreatePreconditions($fulfillment: FulfillmentInput!, $message: String) {
  fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
    fulfillment {
      id
      status
      displayStatus
      trackingInfo {
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
mutation FulfillmentCreatePreconditionsOrderCancel(
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

const fulfillmentOrderReportProgressMutation = `#graphql
mutation FulfillmentCreatePreconditionsReportProgress(
  $id: ID!
  $progressReport: FulfillmentOrderReportProgressInput
) {
  fulfillmentOrderReportProgress(id: $id, progressReport: $progressReport) {
    fulfillmentOrder {
      id
      status
      requestStatus
      lineItems(first: 5) {
        nodes {
          id
          totalQuantity
          remainingQuantity
        }
      }
    }
    userErrors { field message }
  }
}
`;

const orderDeleteMutation = `#graphql
mutation FulfillmentCreatePreconditionsOrderDelete($orderId: ID!) {
  orderDelete(orderId: $orderId) {
    deletedId
    userErrors { field message code }
  }
}
`;

function orderVariables(stamp: number, label: string): JsonRecord {
  return {
    order: {
      email: `fulfillment-create-preconditions-${label}-${stamp}@example.com`,
      note: `fulfillmentCreate preconditions ${label} ${stamp}`,
      tags: ['parity-probe', 'fulfillment-create-preconditions'],
      test: true,
      lineItems: [
        {
          title: `fulfillmentCreate preconditions ${label}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: true,
          taxable: false,
          sku: `FULFILLMENT-CREATE-PRECONDITIONS-${label}-${stamp}`,
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
              amount: '10.00',
              currencyCode: 'CAD',
            },
          },
        },
      ],
    },
    options: null,
  };
}

async function createOrder(stamp: number, label: string): Promise<OrderSetup> {
  const create = await capture(`${label}.orderCreate`, orderCreateMutation, orderVariables(stamp, label));
  requireNoUserErrors(create.response, 'data.orderCreate.userErrors');
  const order = requirePath(create.response['data']?.['orderCreate']?.['order'], `${label}.order`);
  const fulfillmentOrder = requirePath(order['fulfillmentOrders']?.['nodes']?.[0], `${label}.fulfillmentOrder`);
  const fulfillmentOrderLineItem = requirePath(
    fulfillmentOrder['lineItems']?.['nodes']?.[0],
    `${label}.fulfillmentOrderLineItem`,
  );

  return { order, fulfillmentOrder, fulfillmentOrderLineItem, create };
}

function fulfillmentVariables(
  fulfillmentOrderId: string,
  message: string,
  trackingNumber: string,
  fulfillmentOrderLineItem?: JsonRecord,
  quantity?: number,
): JsonRecord {
  const group: JsonRecord = { fulfillmentOrderId };
  if (fulfillmentOrderLineItem && quantity !== undefined) {
    group['fulfillmentOrderLineItems'] = [{ id: fulfillmentOrderLineItem['id'], quantity }];
  }

  return {
    fulfillment: {
      notifyCustomer: false,
      trackingInfo: {
        number: trackingNumber,
        url: `https://example.com/track/${trackingNumber}`,
        company: 'Hermes',
      },
      lineItemsByFulfillmentOrder: [group],
    },
    message,
  };
}

async function hydrateFulfillmentOrder(id: string): Promise<GraphqlStep> {
  return capture('OrdersFulfillmentOrderHydrate', orderHydrateQuery, { id });
}

async function waitForFulfillmentOrderStatus(id: string, wanted: string): Promise<GraphqlStep> {
  let latest = await hydrateFulfillmentOrder(id);
  for (let attempt = 0; attempt < 12; attempt += 1) {
    const status = latest.response['data']?.['fulfillmentOrder']?.['order']?.['fulfillmentOrders']?.['nodes']?.find(
      (node: JsonRecord) => node['id'] === id,
    )?.['status'];
    if (status === wanted) return latest;
    await new Promise((resolve) => setTimeout(resolve, 500));
    latest = await hydrateFulfillmentOrder(id);
  }

  throw new Error(`Timed out waiting for fulfillment order ${id} to reach ${wanted}`);
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
  const cancelled = await createOrder(stamp, 'cancelled');
  const over = await createOrder(stamp, 'over');
  const closed = await createOrder(stamp, 'closed');
  const inProgress = await createOrder(stamp, 'in-progress');
  const happy = await createOrder(stamp, 'happy');
  orderIds.push(
    String(cancelled.order['id']),
    String(over.order['id']),
    String(closed.order['id']),
    String(inProgress.order['id']),
    String(happy.order['id']),
  );

  const cancelledSetup = await capture('cancelled.orderCancel', orderCancelMutation, {
    orderId: cancelled.order['id'],
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });
  requireNoUserErrors(cancelledSetup.response, 'data.orderCancel.userErrors');
  const cancelledHydrate = await waitForFulfillmentOrderStatus(String(cancelled.fulfillmentOrder['id']), 'CLOSED');
  const cancelledCreate = await captureAllowingUserErrors(
    'cancelled.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables(
      String(cancelled.fulfillmentOrder['id']),
      'fulfillmentCreate cancelled precondition',
      `FCP-CANCELLED-${stamp}`,
    ),
  );

  const overHydrate = await hydrateFulfillmentOrder(String(over.fulfillmentOrder['id']));
  const overCreate = await captureAllowingUserErrors(
    'over.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables(
      String(over.fulfillmentOrder['id']),
      'fulfillmentCreate over quantity precondition',
      `FCP-OVER-${stamp}`,
      over.fulfillmentOrderLineItem,
      2,
    ),
  );

  const closedSetupCreate = await capture(
    'closed.setup.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables(
      String(closed.fulfillmentOrder['id']),
      'fulfillmentCreate closed setup',
      `FCP-CLOSED-SETUP-${stamp}`,
    ),
  );
  requireNoUserErrors(closedSetupCreate.response, 'data.fulfillmentCreate.userErrors');
  const closedHydrate = await waitForFulfillmentOrderStatus(String(closed.fulfillmentOrder['id']), 'CLOSED');
  const closedCreate = await captureAllowingUserErrors(
    'closed.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables(
      String(closed.fulfillmentOrder['id']),
      'fulfillmentCreate closed precondition',
      `FCP-CLOSED-${stamp}`,
    ),
  );

  const reportProgress = await capture(
    'inProgress.fulfillmentOrderReportProgress',
    fulfillmentOrderReportProgressMutation,
    {
      id: inProgress.fulfillmentOrder['id'],
      progressReport: { reasonNotes: 'fulfillmentCreate preconditions in-progress setup' },
    },
  );
  requireNoUserErrors(reportProgress.response, 'data.fulfillmentOrderReportProgress.userErrors');
  const inProgressHydrate = await waitForFulfillmentOrderStatus(
    String(inProgress.fulfillmentOrder['id']),
    'IN_PROGRESS',
  );
  const inProgressCreate = await captureAllowingUserErrors(
    'inProgress.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables(
      String(inProgress.fulfillmentOrder['id']),
      'fulfillmentCreate in-progress public behavior',
      `FCP-IN-PROGRESS-${stamp}`,
    ),
  );

  const happyHydrate = await hydrateFulfillmentOrder(String(happy.fulfillmentOrder['id']));
  const happyCreate = await capture(
    'happy.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables(String(happy.fulfillmentOrder['id']), 'fulfillmentCreate happy path', `FCP-HAPPY-${stamp}`),
  );
  requireNoUserErrors(happyCreate.response, 'data.fulfillmentCreate.userErrors');

  for (const orderId of [...orderIds].reverse()) {
    cleanup[orderId] = await cleanupOrder(orderId);
  }

  const capturePayload = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    adminOrigin,
    apiVersion,
    cases: {
      cancelled: {
        setup: { orderCreate: cancelled.create, orderCancel: cancelledSetup, hydrate: cancelledHydrate },
        variables: cancelledCreate.variables,
        response: cancelledCreate.response,
      },
      overFulfill: {
        setup: { orderCreate: over.create, hydrate: overHydrate },
        variables: overCreate.variables,
        response: overCreate.response,
      },
      closed: {
        setup: { orderCreate: closed.create, firstFulfillmentCreate: closedSetupCreate, hydrate: closedHydrate },
        variables: closedCreate.variables,
        response: closedCreate.response,
      },
      inProgress: {
        setup: { orderCreate: inProgress.create, reportProgress, hydrate: inProgressHydrate },
        variables: inProgressCreate.variables,
        response: inProgressCreate.response,
      },
      happyPath: {
        setup: { orderCreate: happy.create, hydrate: happyHydrate },
        variables: happyCreate.variables,
        response: happyCreate.response,
      },
    },
    upstreamCalls: [cancelledHydrate, overHydrate, closedHydrate, inProgressHydrate, happyHydrate].map((step) => ({
      operationName: 'OrdersFulfillmentOrderHydrate',
      variables: step.variables,
      query: step.query,
      response: {
        status: step.status,
        body: step.response,
      },
    })),
    cleanup,
  };

  await writeJson(fixturePath, capturePayload);
  await writeText(requestPath, `${fulfillmentCreateMutation.replace(/^#graphql\n/u, '').trim()}\n`);
  await writeJson(specPath, {
    scenarioId: 'fulfillmentCreate-preconditions',
    operationNames: ['fulfillmentCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['validation-parity', 'mutation-lifecycle', 'no-upstream-passthrough'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: requestPath,
      variablesCapturePath: '$.cases.cancelled.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'cancelled-order-closed-fulfillment-order',
          capturePath: '$.cases.cancelled.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
        },
        {
          name: 'over-fulfill-quantity',
          capturePath: '$.cases.overFulfill.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          proxyRequest: {
            documentPath: requestPath,
            variables: { fromCapturePath: '$.cases.overFulfill.variables' },
            apiVersion,
          },
        },
        {
          name: 'closed-fulfillment-order',
          capturePath: '$.cases.closed.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          proxyRequest: {
            documentPath: requestPath,
            variables: { fromCapturePath: '$.cases.closed.variables' },
            apiVersion,
          },
        },
        {
          name: 'in-progress-public-happy-path',
          capturePath: '$.cases.inProgress.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          selectedPaths: [
            '$.fulfillment.status',
            '$.fulfillment.displayStatus',
            '$.fulfillment.trackingInfo',
            '$.fulfillment.fulfillmentLineItems.nodes[0].quantity',
            '$.fulfillment.fulfillmentLineItems.nodes[0].lineItem.title',
            '$.userErrors',
          ],
          proxyRequest: {
            documentPath: requestPath,
            variables: { fromCapturePath: '$.cases.inProgress.variables' },
            apiVersion,
          },
        },
        {
          name: 'open-fulfillment-order-happy-path',
          capturePath: '$.cases.happyPath.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          selectedPaths: [
            '$.fulfillment.status',
            '$.fulfillment.displayStatus',
            '$.fulfillment.trackingInfo',
            '$.fulfillment.fulfillmentLineItems.nodes[0].quantity',
            '$.fulfillment.fulfillmentLineItems.nodes[0].lineItem.title',
            '$.userErrors',
          ],
          proxyRequest: {
            documentPath: requestPath,
            variables: { fromCapturePath: '$.cases.happyPath.variables' },
            apiVersion,
          },
        },
      ],
    },
    notes:
      'Captured public Admin API fulfillmentCreate preconditions. Public userErrors expose field/message only for this root; 2026-04 accepts fulfillmentCreate after fulfillmentOrderReportProgress leaves a fulfillment order IN_PROGRESS, so the scenario records that as the public happy-path contrast rather than treating it as a rejection.',
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
