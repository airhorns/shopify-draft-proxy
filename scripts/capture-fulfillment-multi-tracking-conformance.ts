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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'fulfillment-multi-tracking-info.json');
const createRequestPath = path.join('config', 'parity-requests', 'orders', 'fulfillmentCreate-multi-tracking.graphql');
const updateRequestPath = path.join(
  'config',
  'parity-requests',
  'orders',
  'fulfillmentTrackingInfoUpdate-multi-tracking.graphql',
);
const readRequestPath = path.join('config', 'parity-requests', 'orders', 'fulfillment-multi-tracking-read.graphql');
const specPath = path.join('config', 'parity-specs', 'orders', 'fulfillment-multi-tracking-info.json');

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

function cleanGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
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
mutation FulfillmentMultiTrackingOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
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
mutation FulfillmentCreateMultiTracking($fulfillment: FulfillmentInput!, $message: String) {
  fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
    fulfillment {
      id
      status
      displayStatus
      trackingInfo(first: 5) {
        number
        url
        company
      }
    }
    userErrors { field message }
  }
}
`;

const fulfillmentTrackingInfoUpdateMutation = `#graphql
mutation FulfillmentTrackingInfoUpdateMultiTracking(
  $fulfillmentId: ID!
  $trackingInfoInput: FulfillmentTrackingInput!
  $notifyCustomer: Boolean
) {
  fulfillmentTrackingInfoUpdate(
    fulfillmentId: $fulfillmentId
    trackingInfoInput: $trackingInfoInput
    notifyCustomer: $notifyCustomer
  ) {
    fulfillment {
      id
      status
      trackingInfo(first: 5) {
        number
        url
        company
      }
    }
    userErrors { field message }
  }
}
`;

const downstreamReadQuery = `#graphql
query FulfillmentMultiTrackingRead($id: ID!) {
  order(id: $id) {
    id
    displayFulfillmentStatus
    fulfillments(first: 5) {
      id
      status
      displayStatus
      trackingInfo(first: 5) {
        number
        url
        company
      }
    }
  }
}
`;

const orderCancelMutation = `#graphql
mutation FulfillmentMultiTrackingCleanupCancel(
  $orderId: ID!
  $reason: OrderCancelReason!
  $notifyCustomer: Boolean
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
mutation FulfillmentMultiTrackingCleanupDelete($orderId: ID!) {
  orderDelete(orderId: $orderId) {
    deletedId
    userErrors { field message }
  }
}
`;

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
let orderId: string | null = null;
let cleanup: JsonRecord | null = null;

try {
  const createNumbers = [`MULTI-CREATE-${stamp}-1`, `MULTI-CREATE-${stamp}-2`];
  const createUrls = createNumbers.map((number) => `https://example.com/track/${number}`);
  const updateNumbers = [`MULTI-UPDATE-${stamp}-1`, `MULTI-UPDATE-${stamp}-2`];
  const updateUrls = updateNumbers.map((number) => `https://example.com/track/${number}`);

  const orderCreate = await capture('orderCreate', orderCreateMutation, {
    order: {
      email: `fulfillment-multi-tracking-${stamp}@example.com`,
      note: `fulfillment multi-tracking capture ${stamp}`,
      tags: ['parity-probe', 'fulfillment-multi-tracking'],
      test: true,
      lineItems: [
        {
          title: `Fulfillment multi-tracking item ${stamp}`,
          quantity: 2,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: true,
          taxable: false,
          sku: `MULTI-TRACK-${stamp}`,
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
  });
  requireNoUserErrors(orderCreate.response, 'data.orderCreate.userErrors');

  const order = requirePath(orderCreate.response['data']?.orderCreate?.order, 'orderCreate.order');
  orderId = String(requirePath(order['id'], 'order.id'));
  const fulfillmentOrderId = String(
    requirePath(order['fulfillmentOrders']?.['nodes']?.[0]?.['id'], 'order.fulfillmentOrders.nodes[0].id'),
  );

  const hydrateBeforeFulfillmentCreate = await capture('hydrateBeforeFulfillmentCreate', orderHydrateQuery, {
    id: fulfillmentOrderId,
  });

  const fulfillmentCreate = await capture('fulfillmentCreate', fulfillmentCreateMutation, {
    fulfillment: {
      notifyCustomer: false,
      trackingInfo: {
        numbers: createNumbers,
        urls: createUrls,
        company: 'Hermes',
      },
      lineItemsByFulfillmentOrder: [{ fulfillmentOrderId }],
    },
    message: 'fulfillment multi-tracking capture',
  });
  requireNoUserErrors(fulfillmentCreate.response, 'data.fulfillmentCreate.userErrors');

  const fulfillment = requirePath(
    fulfillmentCreate.response['data']?.fulfillmentCreate?.fulfillment,
    'fulfillmentCreate.fulfillment',
  );
  const fulfillmentId = String(requirePath(fulfillment['id'], 'fulfillment.id'));

  const fulfillmentTrackingInfoUpdate = await capture(
    'fulfillmentTrackingInfoUpdate',
    fulfillmentTrackingInfoUpdateMutation,
    {
      fulfillmentId,
      notifyCustomer: true,
      trackingInfoInput: {
        numbers: updateNumbers,
        urls: updateUrls,
        company: 'UPS',
      },
    },
  );
  requireNoUserErrors(fulfillmentTrackingInfoUpdate.response, 'data.fulfillmentTrackingInfoUpdate.userErrors');

  const downstreamRead = await capture('downstreamRead', downstreamReadQuery, { id: orderId });

  cleanup = await cleanupOrder(orderId);

  const capturePayload = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    adminOrigin,
    apiVersion,
    orderId,
    fulfillmentOrderId,
    fulfillmentId,
    setup: {
      orderCreate,
      hydrateBeforeFulfillmentCreate,
    },
    fulfillmentCreate,
    fulfillmentTrackingInfoUpdate,
    downstreamRead,
    upstreamCalls: [
      {
        operationName: 'OrdersFulfillmentOrderHydrate',
        variables: hydrateBeforeFulfillmentCreate.variables,
        query: hydrateBeforeFulfillmentCreate.query,
        response: {
          status: hydrateBeforeFulfillmentCreate.status,
          body: hydrateBeforeFulfillmentCreate.response,
        },
      },
    ],
    cleanup,
  };

  await writeJson(fixturePath, capturePayload);
  await writeText(createRequestPath, `${cleanGraphql(fulfillmentCreateMutation)}\n`);
  await writeText(updateRequestPath, `${cleanGraphql(fulfillmentTrackingInfoUpdateMutation)}\n`);
  await writeText(readRequestPath, `${cleanGraphql(downstreamReadQuery)}\n`);
  await writeJson(specPath, {
    scenarioId: 'fulfillment-multi-tracking-info-parity',
    operationNames: ['fulfillmentCreate', 'fulfillmentTrackingInfoUpdate'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'downstream-read-parity', 'runtime-staging'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.fulfillmentCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'fulfillment-create-multi-tracking',
          capturePath: '$.fulfillmentCreate.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          selectedPaths: [
            '$.fulfillment.status',
            '$.fulfillment.displayStatus',
            '$.fulfillment.trackingInfo',
            '$.userErrors',
          ],
        },
        {
          name: 'fulfillment-tracking-info-update-multi-tracking',
          capturePath: '$.fulfillmentTrackingInfoUpdate.response.data.fulfillmentTrackingInfoUpdate',
          proxyPath: '$.data.fulfillmentTrackingInfoUpdate',
          selectedPaths: ['$.fulfillment.status', '$.fulfillment.trackingInfo', '$.userErrors'],
          proxyRequest: {
            documentPath: updateRequestPath,
            variables: {
              fulfillmentId: { fromPrimaryProxyPath: '$.data.fulfillmentCreate.fulfillment.id' },
              trackingInfoInput: { fromCapturePath: '$.fulfillmentTrackingInfoUpdate.variables.trackingInfoInput' },
              notifyCustomer: { fromCapturePath: '$.fulfillmentTrackingInfoUpdate.variables.notifyCustomer' },
            },
            apiVersion,
          },
        },
        {
          name: 'downstream-fulfillment-multi-tracking',
          capturePath: '$.downstreamRead.response.data.order',
          proxyPath: '$.data.order',
          selectedPaths: [
            '$.displayFulfillmentStatus',
            '$.fulfillments[0].status',
            '$.fulfillments[0].displayStatus',
            '$.fulfillments[0].trackingInfo',
          ],
          proxyRequest: {
            documentPath: readRequestPath,
            variables: {
              id: { fromCapturePath: '$.orderId' },
            },
            apiVersion,
          },
        },
      ],
    },
    notes:
      'Captured fulfillmentCreate and fulfillmentTrackingInfoUpdate multi-package tracking behavior using the public FulfillmentTrackingInput numbers/urls fields. The live Admin schema for the configured API exposes number/url/company plus numbers/urls, while trackingDetails/trackingCompany are not accepted input fields.',
  });

  console.log(`Wrote ${fixturePath}`);
  console.log(`Wrote ${createRequestPath}`);
  console.log(`Wrote ${updateRequestPath}`);
  console.log(`Wrote ${readRequestPath}`);
  console.log(`Wrote ${specPath}`);
} catch (error) {
  if (orderId && !cleanup) {
    try {
      cleanup = await cleanupOrder(orderId);
      console.error(`Cleanup completed for ${orderId} after capture failure.`);
    } catch (cleanupError) {
      console.error(`Cleanup failed for ${orderId}:`, cleanupError);
    }
  }
  throw error;
}
