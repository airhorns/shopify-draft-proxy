/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
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
type ReturnSeed = {
  label: string;
  orderId: string;
  fulfillmentLineItemId: string;
  orderCreate: GraphqlCapture;
  fulfillmentCreate: GraphqlCapture;
  orderReadAfterFulfillment: GraphqlCapture;
  seedOrder: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'returnClose-Reopen-Cancel-state-preconditions.json');
const requestDir = path.join('config', 'parity-requests', 'orders');

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

function returnFromPayload(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  return readRecord(payloadRoot(captureResult, rootName)['return']) ?? {};
}

function requireEmptyUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  const payload = readRecord(captureResult.response.payload) ?? {};
  const errors = payload['errors'];
  const data = readRecord(payload['data']) ?? {};
  const root = readRecord(data[rootName]) ?? {};
  const userErrors = readArray(root?.['userErrors']);
  if (errors || userErrors.length > 0) {
    throw new Error(`Unexpected ${rootName} errors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function requireUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  const payload = readRecord(captureResult.response.payload) ?? {};
  const errors = payload['errors'];
  const data = readRecord(payload['data']) ?? {};
  const root = readRecord(data[rootName]) ?? {};
  const userErrors = readArray(root?.['userErrors']);
  if (errors || userErrors.length === 0) {
    throw new Error(`Expected ${rootName} userErrors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function firstActiveLocationId(locations: GraphqlCapture): string {
  const payload = readRecord(locations.response.payload) ?? {};
  const data = readRecord(payload['data']) ?? {};
  const nodes = readNodes(data['locations']);
  const location = nodes.find((node) => node['isActive'] !== false) ?? nodes[0];
  return requireString(location?.['id'], 'location id');
}

function firstFulfillmentOrder(order: JsonRecord): JsonRecord {
  return readNodes(order['fulfillmentOrders'])[0] ?? {};
}

function firstFulfillmentLineItem(order: JsonRecord): JsonRecord {
  return readNodes(readRecord(readArray(order['fulfillments'])[0])?.['fulfillmentLineItems'])[0] ?? {};
}

const orderFields = `#graphql
  fragment ReturnStatusPreconditionOrderFields on Order {
    id
    name
    createdAt
    updatedAt
    displayFinancialStatus
    displayFulfillmentStatus
    totalPriceSet { shopMoney { amount currencyCode } }
    currentTotalPriceSet { shopMoney { amount currencyCode } }
    totalRefundedSet { shopMoney { amount currencyCode } }
    tags
    lineItems(first: 5) {
      nodes {
        id
        title
        quantity
        currentQuantity
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
          }
        }
      }
    }
    fulfillmentOrders(first: 5) {
      nodes {
        id
        status
        requestStatus
        assignedLocation {
          name
          location {
            id
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
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${orderFields}
  mutation ReturnStatusPreconditionOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...ReturnStatusPreconditionOrderFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation ReturnStatusPreconditionFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
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
  query ReturnStatusPreconditionOrderRead($id: ID!) {
    order(id: $id) {
      ...ReturnStatusPreconditionOrderFields
    }
  }
`;

const locationsQuery = `#graphql
  query ReturnStatusPreconditionLocations($first: Int!) {
    locations(first: $first) {
      nodes {
        id
        name
        isActive
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation ReturnStatusPreconditionOrderCancel(
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

const returnRequestMutation = await readRequest('return-request-recorded.graphql');
const returnApproveRequestMutation = await readRequest('return-approve-request-recorded.graphql');
const returnDeclineRequestMutation = await readRequest('return-decline-request-local-staging.graphql');
const returnProcessMutation = await readRequest('return-process-recorded.graphql');
const returnCloseMutation = await readRequest('return-close-state-precondition.graphql');
const returnReopenMutation = await readRequest('return-reopen-state-precondition.graphql');
const returnCancelMutation = await readRequest('return-cancel-state-precondition.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

function orderVariables(label: string): JsonRecord {
  return {
    order: {
      email: `return-status-${label}-${stamp}@example.com`,
      note: `return status precondition capture ${label} ${stamp}`,
      tags: ['return-status-preconditions', label, stamp],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'Return',
        lastName: 'Status',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          title: `Return status ${label} item ${stamp}`,
          quantity: 1,
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
}

async function createFulfilledReturnSeed(label: string): Promise<ReturnSeed> {
  const orderCreate = await capture(orderCreateMutation, orderVariables(label));
  requireEmptyUserErrors(orderCreate, 'orderCreate');

  const createdOrder = readRecord(payloadRoot(orderCreate, 'orderCreate')['order']) ?? {};
  const orderId = requireString(createdOrder['id'], `${label} order id`);
  const fulfillmentOrder = firstFulfillmentOrder(createdOrder);
  const fulfillmentOrderId = requireString(fulfillmentOrder['id'], `${label} fulfillment order id`);
  const fulfillmentOrderLineItem = readNodes(fulfillmentOrder['lineItems'])[0] ?? {};
  const fulfillmentOrderLineItemId = requireString(
    fulfillmentOrderLineItem['id'],
    `${label} fulfillment order line item id`,
  );

  const fulfillmentCreate = await capture(fulfillmentCreateMutation, {
    fulfillment: {
      notifyCustomer: false,
      trackingInfo: {
        number: `RETURN-STATUS-FULFILL-${label}-${stamp}`,
        url: `https://example.com/track/return-status-${label}-${stamp}`,
        company: 'Hermes Carrier',
      },
      lineItemsByFulfillmentOrder: [
        {
          fulfillmentOrderId,
          fulfillmentOrderLineItems: [
            {
              id: fulfillmentOrderLineItemId,
              quantity: 1,
            },
          ],
        },
      ],
    },
    message: `return status precondition fulfillment ${label} ${stamp}`,
  });
  requireEmptyUserErrors(fulfillmentCreate, 'fulfillmentCreate');

  const orderReadAfterFulfillment = await capture(orderReadQuery, { id: orderId });
  const seedOrder = readRecord(orderReadAfterFulfillment.response.payload as JsonRecord)?.['data'];
  const order = readRecord(readRecord(seedOrder)?.['order']) ?? {};
  const fulfillmentLineItem = firstFulfillmentLineItem(order);
  const fulfillmentLineItemId = requireString(fulfillmentLineItem['id'], `${label} fulfillment line item id`);

  return {
    label,
    orderId,
    fulfillmentLineItemId,
    orderCreate,
    fulfillmentCreate,
    orderReadAfterFulfillment,
    seedOrder: readRecord(seedOrder)?.['order'] ?? null,
  };
}

function returnRequestVariables(seed: ReturnSeed): JsonRecord {
  return {
    input: {
      orderId: seed.orderId,
      returnLineItems: [
        {
          fulfillmentLineItemId: seed.fulfillmentLineItemId,
          quantity: 1,
          returnReason: 'OTHER',
        },
      ],
    },
  };
}

const declineInput = {
  declineReason: 'OTHER',
  declineNote: `return status precondition decline ${stamp}`,
  notifyCustomer: false,
};

const locations = await capture(locationsQuery, { first: 10 });
const locationId = firstActiveLocationId(locations);

const requestedSeed = await createFulfilledReturnSeed('requested');
const openSeed = await createFulfilledReturnSeed('open-close-reopen');
const cancelableSeed = await createFulfilledReturnSeed('cancelable');
const declinedSeed = await createFulfilledReturnSeed('declined');
const processedSeed = await createFulfilledReturnSeed('processed');

const requestedReturnRequest = await capture(returnRequestMutation, returnRequestVariables(requestedSeed));
requireEmptyUserErrors(requestedReturnRequest, 'returnRequest');
const requestedReturn = returnFromPayload(requestedReturnRequest, 'returnRequest');
const requestedReturnId = requireString(requestedReturn['id'], 'requested return id');
const returnCloseRequestedInvalid = await capture(returnCloseMutation, { id: requestedReturnId });
requireUserErrors(returnCloseRequestedInvalid, 'returnClose');
const returnReopenRequestedInvalid = await capture(returnReopenMutation, { id: requestedReturnId });
requireUserErrors(returnReopenRequestedInvalid, 'returnReopen');

const openReturnRequest = await capture(returnRequestMutation, returnRequestVariables(openSeed));
requireEmptyUserErrors(openReturnRequest, 'returnRequest');
const openRequestedReturn = returnFromPayload(openReturnRequest, 'returnRequest');
const openReturnId = requireString(openRequestedReturn['id'], 'open return id');
const openReturnApproveRequest = await capture(returnApproveRequestMutation, { input: { id: openReturnId } });
requireEmptyUserErrors(openReturnApproveRequest, 'returnApproveRequest');
const approvedOpenReturn = returnFromPayload(openReturnApproveRequest, 'returnApproveRequest');
const approvedOpenReturnId = requireString(approvedOpenReturn['id'], 'approved open return id');
const returnCloseOpen = await capture(returnCloseMutation, { id: approvedOpenReturnId });
requireEmptyUserErrors(returnCloseOpen, 'returnClose');
const returnCloseClosedIdempotent = await capture(returnCloseMutation, { id: approvedOpenReturnId });
requireEmptyUserErrors(returnCloseClosedIdempotent, 'returnClose');
const returnReopenClosed = await capture(returnReopenMutation, { id: approvedOpenReturnId });
requireEmptyUserErrors(returnReopenClosed, 'returnReopen');
const returnReopenOpenIdempotent = await capture(returnReopenMutation, { id: approvedOpenReturnId });
requireEmptyUserErrors(returnReopenOpenIdempotent, 'returnReopen');

const cancelableReturnRequest = await capture(returnRequestMutation, returnRequestVariables(cancelableSeed));
requireEmptyUserErrors(cancelableReturnRequest, 'returnRequest');
const cancelableRequestedReturn = returnFromPayload(cancelableReturnRequest, 'returnRequest');
const cancelableReturnId = requireString(cancelableRequestedReturn['id'], 'cancelable return id');
const cancelableApproveRequest = await capture(returnApproveRequestMutation, { input: { id: cancelableReturnId } });
requireEmptyUserErrors(cancelableApproveRequest, 'returnApproveRequest');
const cancelableApprovedReturn = returnFromPayload(cancelableApproveRequest, 'returnApproveRequest');
const cancelableApprovedReturnId = requireString(cancelableApprovedReturn['id'], 'cancelable approved return id');
const returnCancelOpen = await capture(returnCancelMutation, { id: cancelableApprovedReturnId });
requireEmptyUserErrors(returnCancelOpen, 'returnCancel');
const returnCancelCanceledIdempotent = await capture(returnCancelMutation, { id: cancelableApprovedReturnId });
requireEmptyUserErrors(returnCancelCanceledIdempotent, 'returnCancel');

const declinedReturnRequest = await capture(returnRequestMutation, returnRequestVariables(declinedSeed));
requireEmptyUserErrors(declinedReturnRequest, 'returnRequest');
const declinedRequestedReturn = returnFromPayload(declinedReturnRequest, 'returnRequest');
const declinedReturnId = requireString(declinedRequestedReturn['id'], 'declined return id');
const returnDeclineRequest = await capture(returnDeclineRequestMutation, {
  input: {
    id: declinedReturnId,
    ...declineInput,
  },
});
requireEmptyUserErrors(returnDeclineRequest, 'returnDeclineRequest');
const declinedReturn = returnFromPayload(returnDeclineRequest, 'returnDeclineRequest');
const declinedClosedInvalid = await capture(returnCloseMutation, {
  id: requireString(declinedReturn['id'], 'declined closed invalid return id'),
});
requireUserErrors(declinedClosedInvalid, 'returnClose');

const processedReturnRequest = await capture(returnRequestMutation, returnRequestVariables(processedSeed));
requireEmptyUserErrors(processedReturnRequest, 'returnRequest');
const processedRequestedReturn = returnFromPayload(processedReturnRequest, 'returnRequest');
const processedReturnId = requireString(processedRequestedReturn['id'], 'processed return id');
const processedApproveRequest = await capture(returnApproveRequestMutation, { input: { id: processedReturnId } });
requireEmptyUserErrors(processedApproveRequest, 'returnApproveRequest');
const processedApprovedReturn = returnFromPayload(processedApproveRequest, 'returnApproveRequest');
const processedReturnLineItem = readNodes(processedApprovedReturn['returnLineItems'])[0] ?? {};
const processedReturnLineItemId = requireString(processedReturnLineItem['id'], 'processed return line item id');
const returnProcess = await capture(returnProcessMutation, {
  input: {
    returnId: processedReturnId,
    returnLineItems: [
      {
        id: processedReturnLineItemId,
        quantity: 1,
      },
    ],
    notifyCustomer: true,
  },
});
requireEmptyUserErrors(returnProcess, 'returnProcess');
const returnCancelProcessedInvalid = await capture(returnCancelMutation, { id: processedReturnId });
requireUserErrors(returnCancelProcessedInvalid, 'returnCancel');

async function cleanupOrder(orderId: string): Promise<GraphqlCapture> {
  return capture(orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });
}

const cleanup = {
  requested: await cleanupOrder(requestedSeed.orderId),
  openCloseReopen: await cleanupOrder(openSeed.orderId),
  cancelable: await cleanupOrder(cancelableSeed.orderId),
  declined: await cleanupOrder(declinedSeed.orderId),
  processed: await cleanupOrder(processedSeed.orderId),
};

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  source: 'live-shopify-admin-graphql',
  notes:
    'Live return status precondition capture for returnClose, returnReopen, and returnCancel success, idempotent, and invalid transition branches on disposable fulfilled orders.',
  locations,
  setup: {
    locationId,
    requested: requestedSeed,
    openCloseReopen: openSeed,
    cancelable: cancelableSeed,
    declined: declinedSeed,
    processed: processedSeed,
  },
  declineRequest: {
    declineInput,
  },
  requestedCase: {
    returnRequest: requestedReturnRequest,
    returnCloseInvalid: returnCloseRequestedInvalid,
    returnReopenInvalid: returnReopenRequestedInvalid,
  },
  openCloseReopenCase: {
    returnRequest: openReturnRequest,
    returnApproveRequest: openReturnApproveRequest,
    returnClose: returnCloseOpen,
    returnCloseIdempotent: returnCloseClosedIdempotent,
    returnReopen: returnReopenClosed,
    returnReopenIdempotent: returnReopenOpenIdempotent,
  },
  cancelableCase: {
    returnRequest: cancelableReturnRequest,
    returnApproveRequest: cancelableApproveRequest,
    returnCancel: returnCancelOpen,
    returnCancelIdempotent: returnCancelCanceledIdempotent,
  },
  declinedCase: {
    returnRequest: declinedReturnRequest,
    returnDeclineRequest,
    returnCloseInvalid: declinedClosedInvalid,
  },
  processedCase: {
    returnRequest: processedReturnRequest,
    returnApproveRequest: processedApproveRequest,
    returnProcess,
    returnCancelInvalid: returnCancelProcessedInvalid,
  },
  cleanup,
  upstreamCalls: [],
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      requestedReturnId,
      approvedOpenReturnId,
      cancelableApprovedReturnId,
      declinedReturnId,
      processedReturnId,
      cleanupUserErrors: {
        requested: readArray(readRecord(payloadRoot(cleanup.requested, 'orderCancel'))?.['userErrors']),
        openCloseReopen: readArray(readRecord(payloadRoot(cleanup.openCloseReopen, 'orderCancel'))?.['userErrors']),
        cancelable: readArray(readRecord(payloadRoot(cleanup.cancelable, 'orderCancel'))?.['userErrors']),
        declined: readArray(readRecord(payloadRoot(cleanup.declined, 'orderCancel'))?.['userErrors']),
        processed: readArray(readRecord(payloadRoot(cleanup.processed, 'orderCancel'))?.['userErrors']),
      },
    },
    null,
    2,
  ),
);
