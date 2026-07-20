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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'return-reverse-logistics-dispose-validation.json');
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

function rootPayload(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  return readRecord(readRecord(captureResult.response.payload as JsonRecord)['data'])?.[rootName] as JsonRecord;
}

function requireEmptyUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  const payload = captureResult.response.payload as JsonRecord;
  const errors = payload['errors'];
  const root = readRecord(readRecord(payload)['data'])?.[rootName];
  const userErrors = readArray(root?.['userErrors']);
  if (errors || userErrors.length > 0) {
    throw new Error(`Unexpected ${rootName} errors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function requireUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  const payload = captureResult.response.payload as JsonRecord;
  const errors = payload['errors'];
  const root = readRecord(readRecord(payload)['data'])?.[rootName];
  const userErrors = readArray(root?.['userErrors']);
  if (errors || userErrors.length === 0) {
    throw new Error(`Expected ${rootName} userErrors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function firstActiveLocationId(locations: GraphqlCapture): string {
  const nodes = readNodes(readRecord(readRecord(locations.response.payload)['data'])?.['locations']);
  const location = nodes.find((node) => node['isActive'] !== false) ?? nodes[0];
  return requireString(location?.['id'], 'location id');
}

function firstFulfillmentOrder(order: JsonRecord): JsonRecord {
  return readNodes(order['fulfillmentOrders'])[0] ?? {};
}

function approvedReturnIds(captureResult: GraphqlCapture): {
  returnId: string;
  returnLineItemId: string;
  reverseFulfillmentOrderId: string;
  reverseFulfillmentOrderLineItemId: string;
} {
  const approvedReturn = readRecord(rootPayload(captureResult, 'returnApproveRequest')['return']) ?? {};
  const returnId = requireString(approvedReturn['id'], 'approved return id');
  const returnLineItem = readNodes(approvedReturn['returnLineItems'])[0] ?? {};
  const reverseFulfillmentOrder = readNodes(approvedReturn['reverseFulfillmentOrders'])[0] ?? {};
  const reverseFulfillmentOrderLineItem = readNodes(reverseFulfillmentOrder['lineItems'])[0] ?? {};
  return {
    returnId,
    returnLineItemId: requireString(returnLineItem['id'], 'return line item id'),
    reverseFulfillmentOrderId: requireString(reverseFulfillmentOrder['id'], 'reverse fulfillment order id'),
    reverseFulfillmentOrderLineItemId: requireString(
      reverseFulfillmentOrderLineItem['id'],
      'reverse fulfillment order line item id',
    ),
  };
}

const orderFields = `#graphql
  fragment ReturnDisposeValidationOrderFields on Order {
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
              variant {
                id
              }
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
  mutation ReturnDisposeValidationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...ReturnDisposeValidationOrderFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation ReturnDisposeValidationFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
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
              variant {
                id
              }
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
  query ReturnDisposeValidationOrderRead($id: ID!) {
    order(id: $id) {
      ...ReturnDisposeValidationOrderFields
    }
  }
`;

const locationsQuery = `#graphql
  query ReturnDisposeValidationLocations($first: Int!) {
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
  mutation ReturnDisposeValidationOrderCancel(
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
const disposeValidationMutation = await readRequest('reverse-fulfillment-order-dispose-validation.graphql');
const downstreamReadQuery = await readRequest('return-reverse-logistics-dispose-validation-read.graphql');
const reverseLogisticsDisposeMutationHydrateQuery = await readRequest(
  'reverse-logistics-dispose-mutation-hydrate.graphql',
);
// The exact document the proxy forwards to hydrate a return's order on a cold
// miss; recording its live response is what replaces the seeded order.
const returnOrderHydrateQuery = await readRequest('return-order-hydrate.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const orderVariables = {
  order: {
    email: `return-dispose-validation-${stamp}@example.com`,
    note: `Return dispose validation capture ${stamp}`,
    tags: ['return-dispose-validation', stamp],
    test: true,
    currency: 'USD',
    shippingAddress: {
      firstName: 'Return',
      lastName: 'Dispose',
      address1: '123 Queen St W',
      city: 'Toronto',
      provinceCode: 'ON',
      countryCode: 'CA',
      zip: 'M5H 2M9',
    },
    lineItems: [
      {
        title: `Return dispose custom line ${stamp}`,
        quantity: 3,
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

const locations = await capture(locationsQuery, { first: 10 });
const fallbackLocationId = firstActiveLocationId(locations);
const orderCreate = await capture(orderCreateMutation, orderVariables);
requireEmptyUserErrors(orderCreate, 'orderCreate');

const createdOrder = readRecord(rootPayload(orderCreate, 'orderCreate')['order']) ?? {};
const orderId = requireString(createdOrder['id'], 'created order id');
const fulfillmentOrder = firstFulfillmentOrder(createdOrder);
const fulfillmentOrderId = requireString(fulfillmentOrder['id'], 'created fulfillment order id');
const assignedLocation = readRecord(readRecord(fulfillmentOrder['assignedLocation'])?.['location']);
const locationId =
  typeof assignedLocation?.['id'] === 'string' ? (assignedLocation['id'] as string) : fallbackLocationId;
const disposeLocationHydrates: ConformanceGraphqlResult[] = [];
for (let index = 0; index < 3; index += 1) {
  const result = await runGraphqlRequest(reverseLogisticsDisposeMutationHydrateQuery, { ids: [locationId] });
  if (result.payload['errors']) {
    throw new Error(`reverse-logistics disposal location hydrate returned errors: ${JSON.stringify(result.payload)}`);
  }
  const location = readArray(readRecord(result.payload['data'])?.['nodes'])
    .map(readRecord)
    .find((node) => node?.['id'] === locationId);
  if (location?.['__typename'] !== 'Location') {
    throw new Error(`reverse-logistics disposal location hydrate did not return ${locationId}`);
  }
  disposeLocationHydrates.push(result);
}
const fulfillmentOrderLineItems = readNodes(fulfillmentOrder['lineItems']);

const fulfillmentCreate = await capture(fulfillmentCreateMutation, {
  fulfillment: {
    notifyCustomer: false,
    trackingInfo: {
      number: `RETURN-DISPOSE-FULFILL-${stamp}`,
      url: `https://example.com/track/RETURN-DISPOSE-FULFILL-${stamp}`,
      company: 'Hermes Carrier',
    },
    lineItemsByFulfillmentOrder: [
      {
        fulfillmentOrderId,
        fulfillmentOrderLineItems: fulfillmentOrderLineItems.map((lineItem) => ({
          id: requireString(lineItem['id'], 'created fulfillment order line item id'),
          quantity: Number(lineItem['totalQuantity'] ?? 0),
        })),
      },
    ],
  },
  message: `Return dispose validation fulfillment ${stamp}`,
});
requireEmptyUserErrors(fulfillmentCreate, 'fulfillmentCreate');

const orderReadAfterFulfillment = await capture(orderReadQuery, { id: orderId });
const orderAfterFulfillment = readRecord(readRecord(orderReadAfterFulfillment.response.payload)['data'])?.['order'];
const fulfillmentLineItems = readNodes(
  readRecord(readArray(readRecord(orderAfterFulfillment)?.['fulfillments'])[0])?.['fulfillmentLineItems'],
);

// Record the proxy's cold-miss order hydrate (byte-identical document) against the
// freshly fulfilled order — before any return exists — so replay forwards+observes
// real store state instead of relying on a seeded order. The custom (variant-less)
// line item is exactly why dispose RESTOCKED is rejected downstream.
const returnOrderHydrate = await runGraphqlRequest(returnOrderHydrateQuery, { id: orderId });
if (returnOrderHydrate.payload['errors']) {
  throw new Error(`return-order hydrate returned errors: ${JSON.stringify(returnOrderHydrate.payload)}`);
}
const customFulfillmentLineItem = fulfillmentLineItems.find((line) => {
  const lineItem = readRecord(line['lineItem']);
  return typeof lineItem?.['title'] === 'string' && lineItem['title'].includes('Return dispose custom line');
});
const customFulfillmentLineItemId = requireString(
  customFulfillmentLineItem?.['id'],
  'fulfilled custom fulfillment line item id',
);

async function requestReturn(fulfillmentLineItemId: string): Promise<GraphqlCapture> {
  const result = await capture(returnRequestMutation, {
    input: {
      orderId,
      returnLineItems: [
        {
          fulfillmentLineItemId,
          quantity: 1,
          returnReason: 'OTHER',
        },
      ],
    },
  });
  requireEmptyUserErrors(result, 'returnRequest');
  return result;
}

async function approveReturn(returnRequest: GraphqlCapture): Promise<GraphqlCapture> {
  const requestedReturn = readRecord(rootPayload(returnRequest, 'returnRequest')['return']) ?? {};
  const result = await capture(returnApproveRequestMutation, {
    input: {
      id: requireString(requestedReturn['id'], 'requested return id'),
    },
  });
  requireEmptyUserErrors(result, 'returnApproveRequest');
  return result;
}

const firstReturnRequest = await requestReturn(customFulfillmentLineItemId);
const firstReturnApproveRequest = await approveReturn(firstReturnRequest);
const firstApprovedIds = approvedReturnIds(firstReturnApproveRequest);

const emptyDispose = await capture(disposeValidationMutation, { dispositionInputs: [] });
requireUserErrors(emptyDispose, 'reverseFulfillmentOrderDispose');

const unknownLineDispose = await capture(disposeValidationMutation, {
  dispositionInputs: [
    {
      reverseFulfillmentOrderLineItemId: 'gid://shopify/ReverseFulfillmentOrderLineItem/0',
      quantity: 1,
      dispositionType: 'NOT_RESTOCKED',
      locationId,
    },
  ],
});
requireUserErrors(unknownLineDispose, 'reverseFulfillmentOrderDispose');

const customRestockedDispose = await capture(disposeValidationMutation, {
  dispositionInputs: [
    {
      reverseFulfillmentOrderLineItemId: firstApprovedIds.reverseFulfillmentOrderLineItemId,
      quantity: 1,
      dispositionType: 'RESTOCKED',
      locationId,
    },
  ],
});
requireUserErrors(customRestockedDispose, 'reverseFulfillmentOrderDispose');

const secondReturnRequest = await requestReturn(customFulfillmentLineItemId);
const secondReturnApproveRequest = await approveReturn(secondReturnRequest);
const secondApprovedIds = approvedReturnIds(secondReturnApproveRequest);

const multipleReverseFulfillmentOrdersDispose = await capture(disposeValidationMutation, {
  dispositionInputs: [
    {
      reverseFulfillmentOrderLineItemId: firstApprovedIds.reverseFulfillmentOrderLineItemId,
      quantity: 1,
      dispositionType: 'NOT_RESTOCKED',
      locationId,
    },
    {
      reverseFulfillmentOrderLineItemId: secondApprovedIds.reverseFulfillmentOrderLineItemId,
      quantity: 1,
      dispositionType: 'NOT_RESTOCKED',
      locationId,
    },
  ],
});
requireUserErrors(multipleReverseFulfillmentOrdersDispose, 'reverseFulfillmentOrderDispose');

const validDispose = await capture(disposeValidationMutation, {
  dispositionInputs: [
    {
      reverseFulfillmentOrderLineItemId: firstApprovedIds.reverseFulfillmentOrderLineItemId,
      quantity: 1,
      dispositionType: 'NOT_RESTOCKED',
      locationId,
    },
  ],
});
requireEmptyUserErrors(validDispose, 'reverseFulfillmentOrderDispose');

const downstreamRead = await capture(downstreamReadQuery, {
  firstReturnId: firstApprovedIds.returnId,
  secondReturnId: secondApprovedIds.returnId,
  orderId,
  firstReverseFulfillmentOrderId: firstApprovedIds.reverseFulfillmentOrderId,
  secondReverseFulfillmentOrderId: secondApprovedIds.reverseFulfillmentOrderId,
});

const cleanup = await capture(orderCancelMutation, {
  orderId,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: true,
});

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  source: 'live-shopify-admin-graphql',
  notes:
    'Live return/reverse-fulfillment disposal validation capture for empty inputs, unknown RFO line item, excessive quantity, custom-line RESTOCKED, multiple reverse fulfillment orders, valid NOT_RESTOCKED disposal, and downstream reads.',
  locations,
  setup: {
    orderCreate,
    fulfillmentCreate,
    orderReadAfterFulfillment,
  },
  firstReturnRequest,
  firstReturnApproveRequest,
  secondReturnRequest,
  secondReturnApproveRequest,
  disposeValidation: {
    empty: emptyDispose,
    unknownLine: unknownLineDispose,
    customRestocked: customRestockedDispose,
    multipleReverseFulfillmentOrders: multipleReverseFulfillmentOrdersDispose,
    validNotRestocked: validDispose,
  },
  downstreamRead,
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
    ...disposeLocationHydrates.map((result) => ({
      operationName: 'ReverseLogisticsDisposeMutationHydrate',
      variables: { ids: [locationId] },
      query: reverseLogisticsDisposeMutationHydrateQuery,
      response: {
        status: result.status,
        body: result.payload,
      },
    })),
  ],
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      orderId,
      firstReturnId: firstApprovedIds.returnId,
      secondReturnId: secondApprovedIds.returnId,
      firstReverseFulfillmentOrderId: firstApprovedIds.reverseFulfillmentOrderId,
      secondReverseFulfillmentOrderId: secondApprovedIds.reverseFulfillmentOrderId,
      cleanupUserErrors: readArray(readRecord(rootPayload(cleanup, 'orderCancel'))?.['userErrors']),
    },
    null,
    2,
  ),
);
