/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
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
  returnOrderHydrate: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'returnApprove-decline-state-preconditions.json');
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
  const root = payloadRoot(captureResult, rootName);
  const userErrors = readArray(root['userErrors']);
  if (errors || userErrors.length > 0) {
    throw new Error(`Unexpected ${rootName} errors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function requireUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  const payload = readRecord(captureResult.response.payload) ?? {};
  const errors = payload['errors'];
  const root = payloadRoot(captureResult, rootName);
  const userErrors = readArray(root['userErrors']);
  if (errors || userErrors.length === 0) {
    throw new Error(`Expected ${rootName} userErrors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function firstFulfillmentOrder(order: JsonRecord): JsonRecord {
  return readNodes(order['fulfillmentOrders'])[0] ?? {};
}

function firstFulfillmentLineItem(order: JsonRecord): JsonRecord {
  return readNodes(readRecord(readArray(order['fulfillments'])[0])?.['fulfillmentLineItems'])[0] ?? {};
}

const orderFields = `#graphql
  fragment ReturnApproveDeclineStateOrderFields on Order {
    id
    name
    createdAt
    updatedAt
    displayFinancialStatus
    displayFulfillmentStatus
    lineItems(first: 5) {
      nodes {
        id
        title
        quantity
      }
    }
    fulfillments(first: 5) {
      id
      status
      displayStatus
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
  mutation ReturnApproveDeclineStateOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...ReturnApproveDeclineStateOrderFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation ReturnApproveDeclineStateFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        id
        status
        displayStatus
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
  query ReturnApproveDeclineStateOrderRead($id: ID!) {
    order(id: $id) {
      ...ReturnApproveDeclineStateOrderFields
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation ReturnApproveDeclineStateOrderCancel(
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
const returnApproveRequestRecordedMutation = await readRequest('return-approve-request-recorded.graphql');
const returnDeclineRequestMutation = await readRequest('return-decline-request-local-staging.graphql');
const returnOrderHydrateQuery = await readRequest('return-order-hydrate.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

function orderVariables(label: string): JsonRecord {
  return {
    order: {
      email: `return-approve-decline-${label}-${stamp}@example.com`,
      note: `return approve decline state precondition capture ${label} ${stamp}`,
      tags: ['return-approve-decline', label, stamp],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'Return',
        lastName: 'State',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          title: `Return approve decline ${label} item ${stamp}`,
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
        number: `RETURN-APPROVE-DECLINE-${label}-${stamp}`,
        url: `https://example.com/track/return-approve-decline-${label}-${stamp}`,
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
    message: `return approve decline state precondition fulfillment ${label} ${stamp}`,
  });
  requireEmptyUserErrors(fulfillmentCreate, 'fulfillmentCreate');

  const orderReadAfterFulfillment = await capture(orderReadQuery, { id: orderId });
  const readPayload = readRecord(orderReadAfterFulfillment.response.payload) ?? {};
  const readData = readRecord(readPayload['data']) ?? {};
  const order = readRecord(readData['order']) ?? {};
  const fulfillmentLineItem = firstFulfillmentLineItem(order);
  const fulfillmentLineItemId = requireString(fulfillmentLineItem['id'], `${label} fulfillment line item id`);

  const returnOrderHydrate = await runGraphqlRequest(returnOrderHydrateQuery, { id: orderId });
  if (returnOrderHydrate.payload['errors']) {
    throw new Error(`${label} return-order hydrate returned errors: ${JSON.stringify(returnOrderHydrate.payload)}`);
  }

  return {
    label,
    orderId,
    fulfillmentLineItemId,
    orderCreate,
    fulfillmentCreate,
    orderReadAfterFulfillment,
    returnOrderHydrate,
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
  declineNote: `return approve decline state precondition ${stamp}`,
  notifyCustomer: false,
};

const openSeed = await createFulfilledReturnSeed('open');
const declinedSeed = await createFulfilledReturnSeed('declined');

const openReturnRequest = await capture(returnRequestMutation, returnRequestVariables(openSeed));
requireEmptyUserErrors(openReturnRequest, 'returnRequest');
const openRequestedReturn = returnFromPayload(openReturnRequest, 'returnRequest');
const openRequestedReturnId = requireString(openRequestedReturn['id'], 'open requested return id');
const openReturnApproveRequest = await capture(returnApproveRequestRecordedMutation, {
  input: { id: openRequestedReturnId },
});
requireEmptyUserErrors(openReturnApproveRequest, 'returnApproveRequest');
const openReturn = returnFromPayload(openReturnApproveRequest, 'returnApproveRequest');
const openReturnId = requireString(openReturn['id'], 'open return id');
const approveOpenInvalid = await capture(returnApproveRequestRecordedMutation, { input: { id: openReturnId } });
requireUserErrors(approveOpenInvalid, 'returnApproveRequest');
const declineOpenInvalid = await capture(returnDeclineRequestMutation, {
  input: { id: openReturnId, ...declineInput },
});
requireUserErrors(declineOpenInvalid, 'returnDeclineRequest');

const declinedReturnRequest = await capture(returnRequestMutation, returnRequestVariables(declinedSeed));
requireEmptyUserErrors(declinedReturnRequest, 'returnRequest');
const declinedRequestedReturn = returnFromPayload(declinedReturnRequest, 'returnRequest');
const declinedRequestedReturnId = requireString(declinedRequestedReturn['id'], 'declined requested return id');
const returnDeclineRequest = await capture(returnDeclineRequestMutation, {
  input: { id: declinedRequestedReturnId, ...declineInput },
});
requireEmptyUserErrors(returnDeclineRequest, 'returnDeclineRequest');
const declinedReturn = returnFromPayload(returnDeclineRequest, 'returnDeclineRequest');
const declinedReturnId = requireString(declinedReturn['id'], 'declined return id');
const approveDeclinedInvalid = await capture(returnApproveRequestRecordedMutation, { input: { id: declinedReturnId } });
requireUserErrors(approveDeclinedInvalid, 'returnApproveRequest');
const declineDeclinedInvalid = await capture(returnDeclineRequestMutation, {
  input: { id: declinedReturnId, ...declineInput },
});
requireUserErrors(declineDeclinedInvalid, 'returnDeclineRequest');

const approveMissing = await capture(returnApproveRequestRecordedMutation, {
  input: { id: 'gid://shopify/Return/999999999991' },
});
requireUserErrors(approveMissing, 'returnApproveRequest');
const declineMissing = await capture(returnDeclineRequestMutation, {
  input: { id: 'gid://shopify/Return/999999999992', ...declineInput },
});
requireUserErrors(declineMissing, 'returnDeclineRequest');

async function cleanupOrder(orderId: string): Promise<GraphqlCapture> {
  return capture(orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });
}

const cleanup = {
  open: await cleanupOrder(openSeed.orderId),
  declined: await cleanupOrder(declinedSeed.orderId),
};

const returnSeeds = [openSeed, declinedSeed];
const setupSeed = ({ returnOrderHydrate: _hydrate, ...rest }: ReturnSeed): Omit<ReturnSeed, 'returnOrderHydrate'> =>
  rest;

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  source: 'live-shopify-admin-graphql',
  notes:
    'Live returnApproveRequest/returnDeclineRequest state-precondition capture for non-REQUESTED OPEN and DECLINED returns plus unknown Return IDs, using disposable fulfilled orders.',
  setup: {
    open: setupSeed(openSeed),
    declined: setupSeed(declinedSeed),
  },
  declineRequest: {
    declineInput,
  },
  openCase: {
    returnRequest: openReturnRequest,
    returnApproveRequest: openReturnApproveRequest,
    approveOpenInvalid,
    declineOpenInvalid,
  },
  declinedCase: {
    returnRequest: declinedReturnRequest,
    returnDeclineRequest,
    approveDeclinedInvalid,
    declineDeclinedInvalid,
  },
  unknownIdCase: {
    approveMissing,
    declineMissing,
  },
  cleanup,
  upstreamCalls: returnSeeds.map((seed) => ({
    operationName: 'OrdersReturnOrderHydrate',
    variables: { id: seed.orderId },
    query: returnOrderHydrateQuery,
    response: {
      status: seed.returnOrderHydrate.status,
      body: seed.returnOrderHydrate.payload,
    },
  })),
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      openReturnId,
      declinedReturnId,
      cleanupUserErrors: {
        open: readArray(readRecord(payloadRoot(cleanup.open, 'orderCancel'))?.['userErrors']),
        declined: readArray(readRecord(payloadRoot(cleanup.declined, 'orderCancel'))?.['userErrors']),
      },
    },
    null,
    2,
  ),
);
