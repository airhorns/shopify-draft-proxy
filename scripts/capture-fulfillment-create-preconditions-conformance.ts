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
const v2RequestPath = path.join('config', 'parity-requests', 'orders', 'fulfillmentCreateV2-preconditions.graphql');
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

function userErrorsAt(payload: JsonRecord, pathLabel: string): JsonRecord[] {
  const errors = pathLabel
    .split('.')
    .reduce<unknown>((current, segment) => (current as JsonRecord | undefined)?.[segment], payload);
  return Array.isArray(errors) ? errors : [];
}

function isTooManyAttempts(errors: JsonRecord[]): boolean {
  return errors.some((error) => String(error['message'] ?? '').includes('Too many attempts'));
}

async function delay(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
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

// Byte-for-byte copy of the proxy's ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY
// (src/proxy/orders_payments_fulfillment.rs). On a cold fulfillmentCreate the
// proxy forwards THIS expanded document (operationName ShippingFulfillmentOrderHydrate),
// not the richer OrdersFulfillmentOrderHydrate above — the latter is only used
// here for internal status polling. Recording this per fulfillment order at its
// precondition state is what replaces the seeded order. Kept flush-left so
// trimGraphql leaves it identical to the constant.
const shippingHydrateQuery = `query ShippingFulfillmentOrderHydrate($id: ID!) {
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

const fulfillmentCreateV2Mutation = `#graphql
mutation FulfillmentCreateV2Preconditions($fulfillment: FulfillmentV2Input!, $message: String) {
  fulfillmentCreateV2(fulfillment: $fulfillment, message: $message) {
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
  let create: GraphqlStep | null = null;
  for (let attempt = 0; attempt < 6; attempt += 1) {
    create = await capture(`${label}.orderCreate`, orderCreateMutation, orderVariables(stamp, label));
    const errors = userErrorsAt(create.response, 'data.orderCreate.userErrors');
    if (errors.length === 0) break;
    if (isTooManyAttempts(errors) && attempt < 5) {
      await delay(30_000 * (attempt + 1));
      continue;
    }
    requireNoUserErrors(create.response, 'data.orderCreate.userErrors');
  }
  if (!create) {
    throw new Error(`${label}.orderCreate did not return a capture`);
  }
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

function missingFulfillmentVariables(fulfillmentOrderId: string, message: string, trackingNumber: string): JsonRecord {
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
          fulfillmentOrderLineItems: [
            {
              id: `gid://shopify/FulfillmentOrderLineItem/${resourceIdTail(fulfillmentOrderId)}`,
              quantity: 1,
            },
          ],
        },
      ],
    },
    message,
  };
}

function resourceIdTail(id: string): string {
  return id.split('/').at(-1) ?? id;
}

async function hydrateFulfillmentOrder(id: string): Promise<GraphqlStep> {
  return capture('OrdersFulfillmentOrderHydrate', orderHydrateQuery, { id });
}

// The exact cold-miss hydrate the proxy forwards for a fulfillmentCreate. Recorded
// per fulfillment order at its precondition state and surfaced as upstreamCalls.
async function captureShippingHydrate(id: string): Promise<GraphqlStep> {
  return capture('ShippingFulfillmentOrderHydrate', shippingHydrateQuery, { id });
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
  const trackedOrder = async (label: string): Promise<OrderSetup> => {
    const setup = await createOrder(stamp, label);
    orderIds.push(String(setup.order['id']));
    return setup;
  };

  const cancelled = await trackedOrder('cancelled');
  const nonPositive = await trackedOrder('non-positive');
  const nonPositiveV2 = await trackedOrder('non-positive-v2');
  const over = await trackedOrder('over');
  const closed = await trackedOrder('closed');
  const inProgress = await trackedOrder('in-progress');
  const happy = await trackedOrder('happy');

  const cancelledSetup = await capture('cancelled.orderCancel', orderCancelMutation, {
    orderId: cancelled.order['id'],
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });
  requireNoUserErrors(cancelledSetup.response, 'data.orderCancel.userErrors');
  const cancelledHydrate = await waitForFulfillmentOrderStatus(String(cancelled.fulfillmentOrder['id']), 'CLOSED');
  const cancelledShippingHydrate = await captureShippingHydrate(String(cancelled.fulfillmentOrder['id']));
  const cancelledCreate = await captureAllowingUserErrors(
    'cancelled.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables(
      String(cancelled.fulfillmentOrder['id']),
      'fulfillmentCreate cancelled precondition',
      `FCP-CANCELLED-${stamp}`,
    ),
  );

  const nonPositiveHydrate = await hydrateFulfillmentOrder(String(nonPositive.fulfillmentOrder['id']));
  const nonPositiveShippingHydrate = await captureShippingHydrate(String(nonPositive.fulfillmentOrder['id']));
  const nonPositiveCreate = await captureAllowingUserErrors(
    'nonPositive.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables(
      String(nonPositive.fulfillmentOrder['id']),
      'fulfillmentCreate non-positive quantity precondition',
      `FCP-NON-POSITIVE-${stamp}`,
      nonPositive.fulfillmentOrderLineItem,
      0,
    ),
  );

  const nonPositiveV2Hydrate = await hydrateFulfillmentOrder(String(nonPositiveV2.fulfillmentOrder['id']));
  const nonPositiveV2ShippingHydrate = await captureShippingHydrate(String(nonPositiveV2.fulfillmentOrder['id']));
  const nonPositiveV2Create = await captureAllowingUserErrors(
    'nonPositiveV2.fulfillmentCreateV2',
    fulfillmentCreateV2Mutation,
    fulfillmentVariables(
      String(nonPositiveV2.fulfillmentOrder['id']),
      'fulfillmentCreateV2 non-positive quantity precondition',
      `FCP-V2-NON-POSITIVE-${stamp}`,
      nonPositiveV2.fulfillmentOrderLineItem,
      0,
    ),
  );

  const overHydrate = await hydrateFulfillmentOrder(String(over.fulfillmentOrder['id']));
  const overShippingHydrate = await captureShippingHydrate(String(over.fulfillmentOrder['id']));
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

  const missingFulfillmentOrderId = 'gid://shopify/FulfillmentOrder/999999999';
  const missingV2FulfillmentOrderId = 'gid://shopify/FulfillmentOrder/999999998';
  const missingShippingHydrate = await captureShippingHydrate(missingFulfillmentOrderId);
  const missingCreate = await captureAllowingUserErrors(
    'missing.fulfillmentCreate',
    fulfillmentCreateMutation,
    missingFulfillmentVariables(
      missingFulfillmentOrderId,
      'fulfillmentCreate missing fulfillment order precondition',
      `FCP-MISSING-${stamp}`,
    ),
  );
  const missingV2ShippingHydrate = await captureShippingHydrate(missingV2FulfillmentOrderId);
  const missingV2Create = await captureAllowingUserErrors(
    'missingV2.fulfillmentCreateV2',
    fulfillmentCreateV2Mutation,
    missingFulfillmentVariables(
      missingV2FulfillmentOrderId,
      'fulfillmentCreateV2 missing fulfillment order precondition',
      `FCP-V2-MISSING-${stamp}`,
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
  const closedShippingHydrate = await captureShippingHydrate(String(closed.fulfillmentOrder['id']));
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
  const inProgressShippingHydrate = await captureShippingHydrate(String(inProgress.fulfillmentOrder['id']));
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
  const happyShippingHydrate = await captureShippingHydrate(String(happy.fulfillmentOrder['id']));
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
      nonPositiveQuantity: {
        setup: { orderCreate: nonPositive.create, hydrate: nonPositiveHydrate },
        variables: nonPositiveCreate.variables,
        response: nonPositiveCreate.response,
      },
      nonPositiveQuantityV2: {
        setup: { orderCreate: nonPositiveV2.create, hydrate: nonPositiveV2Hydrate },
        variables: nonPositiveV2Create.variables,
        response: nonPositiveV2Create.response,
      },
      overFulfill: {
        setup: { orderCreate: over.create, hydrate: overHydrate },
        variables: overCreate.variables,
        response: overCreate.response,
      },
      missingFulfillmentOrder: {
        variables: missingCreate.variables,
        response: missingCreate.response,
      },
      missingFulfillmentOrderV2: {
        variables: missingV2Create.variables,
        response: missingV2Create.response,
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
    upstreamCalls: [
      cancelledShippingHydrate,
      nonPositiveShippingHydrate,
      nonPositiveV2ShippingHydrate,
      overShippingHydrate,
      missingShippingHydrate,
      missingV2ShippingHydrate,
      closedShippingHydrate,
      inProgressShippingHydrate,
      happyShippingHydrate,
    ].map((step) => ({
      operationName: 'ShippingFulfillmentOrderHydrate',
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
  await writeText(v2RequestPath, `${fulfillmentCreateV2Mutation.replace(/^#graphql\n/u, '').trim()}\n`);
  await writeJson(specPath, {
    scenarioId: 'fulfillmentCreate-preconditions',
    operationNames: ['fulfillmentCreate', 'fulfillmentCreateV2'],
    scenarioStatus: 'captured',
    assertionKinds: ['validation-parity', 'user-errors-parity', 'mutation-lifecycle', 'no-upstream-passthrough'],
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
          name: 'non-positive-line-item-quantity',
          capturePath: '$.cases.nonPositiveQuantity.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          proxyRequest: {
            documentPath: requestPath,
            variables: { fromCapturePath: '$.cases.nonPositiveQuantity.variables' },
            apiVersion,
          },
        },
        {
          name: 'non-positive-line-item-quantity-v2',
          capturePath: '$.cases.nonPositiveQuantityV2.response.data.fulfillmentCreateV2',
          proxyPath: '$.data.fulfillmentCreateV2',
          proxyRequest: {
            documentPath: v2RequestPath,
            variables: { fromCapturePath: '$.cases.nonPositiveQuantityV2.variables' },
            apiVersion,
          },
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
          name: 'missing-fulfillment-order',
          capturePath: '$.cases.missingFulfillmentOrder.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          proxyRequest: {
            documentPath: requestPath,
            variables: { fromCapturePath: '$.cases.missingFulfillmentOrder.variables' },
            apiVersion,
          },
        },
        {
          name: 'missing-fulfillment-order-v2',
          capturePath: '$.cases.missingFulfillmentOrderV2.response.data.fulfillmentCreateV2',
          proxyPath: '$.data.fulfillmentCreateV2',
          proxyRequest: {
            documentPath: v2RequestPath,
            variables: { fromCapturePath: '$.cases.missingFulfillmentOrderV2.variables' },
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
      'Captured public Admin API fulfillmentCreate and deprecated fulfillmentCreateV2 preconditions. Public userErrors expose field/message only for these roots; non-positive line-item quantities return an indexed quantity path, missing fulfillment orders return field ["fulfillment"], and Admin 2026-04 accepts fulfillmentCreate after fulfillmentOrderReportProgress leaves the fulfillment order IN_PROGRESS.',
  });

  console.log(`Wrote ${fixturePath}`);
  console.log(`Wrote ${requestPath}`);
  console.log(`Wrote ${v2RequestPath}`);
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
