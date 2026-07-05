/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { readFileSync } from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CaptureResult = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const scenarioId = 'fulfillment-order-close-state';
const apiVersion = '2026-04';
const { storeDomain, adminOrigin } = readConformanceScriptConfig({
  defaultApiVersion: apiVersion,
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

function readRequest(name: string): string {
  return readFileSync(path.join('config', 'parity-requests', 'shipping-fulfillments', name), 'utf8');
}

const submitRequestMutation = readRequest('fulfillment-order-submit-request-lifecycle.graphql');
const acceptRequestMutation = readRequest('fulfillment-order-accept-request-lifecycle.graphql');
const closeMutation = readRequest('fulfillment-order-lifecycle-close.graphql');
const directReadQuery = readRequest('fulfillment-order-close-state-read.graphql');

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderCloseStateFields on FulfillmentOrder {
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
`;

const orderCreateMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderCloseStateOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 5) {
          nodes {
            ...FulfillmentOrderCloseStateFields
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
  mutation FulfillmentOrderCloseStateOrderCancel(
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
      orderCancelUserErrors {
        field
        message
        code
      }
    }
  }
`;

const fulfillmentServiceCreateMutation = `#graphql
  mutation FulfillmentOrderCloseStateServiceCreate($name: String!, $callbackUrl: URL!) {
    fulfillmentServiceCreate(
      name: $name
      callbackUrl: $callbackUrl
      trackingSupport: true
      inventoryManagement: true
      requiresShippingMethod: true
    ) {
      fulfillmentService {
        id
        handle
        serviceName
        type
        location {
          id
          name
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentServiceDeleteMutation = `#graphql
  mutation FulfillmentOrderCloseStateServiceDelete($id: ID!) {
    fulfillmentServiceDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const moveMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderCloseStateMove($id: ID!, $newLocationId: ID!) {
    fulfillmentOrderMove(id: $id, newLocationId: $newLocationId) {
      movedFulfillmentOrder {
        ...FulfillmentOrderCloseStateFields
      }
      originalFulfillmentOrder {
        ...FulfillmentOrderCloseStateFields
      }
      remainingFulfillmentOrder {
        ...FulfillmentOrderCloseStateFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

// Byte-for-byte copy of the proxy's ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY
// (src/proxy/orders_payments_fulfillment.rs). The submit / accept / close
// lifecycle mutations forward this exact (expanded, not compact) document on a
// cold fulfillment-order miss; recording its live response is what lets
// de-seeded replay forward+observe the order. Kept flush-left so trimGraphql
// leaves it identical to the constant.
const hydrateQuery = `query ShippingFulfillmentOrderHydrate($id: ID!) {
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

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

function isJsonRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readObject(value: unknown): JsonRecord | null {
  return isJsonRecord(value) ? value : null;
}

function readNodes(value: unknown): JsonRecord[] {
  const record = readObject(value);
  const nodes = record?.['nodes'];
  return Array.isArray(nodes) ? nodes.filter(isJsonRecord) : [];
}

function data(captureResult: CaptureResult): JsonRecord {
  return readObject(captureResult.response.payload.data) ?? {};
}

function userErrors(captureResult: CaptureResult, root: string): unknown[] {
  const payload = readObject(data(captureResult)[root]);
  const errors = payload?.['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors(captureResult: CaptureResult, root: string, label: string): void {
  const errors = userErrors(captureResult, root);
  if (
    captureResult.response.status < 200 ||
    captureResult.response.status >= 300 ||
    captureResult.response.payload.errors
  ) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(captureResult.response.payload)}`);
  }
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function readStringField(record: JsonRecord, field: string, label: string): string {
  const value = record[field];
  if (typeof value !== 'string') {
    throw new Error(`Unable to read ${label}: ${JSON.stringify(record)}`);
  }
  return value;
}

function firstFulfillmentOrder(captureResult: CaptureResult): JsonRecord {
  const orderCreate = readObject(data(captureResult)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  const fulfillmentOrder = readNodes(readObject(order?.['fulfillmentOrders']))[0];
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return fulfillmentOrder;
}

function movedFulfillmentOrder(captureResult: CaptureResult): JsonRecord {
  const payload = readObject(data(captureResult)['fulfillmentOrderMove']);
  const fulfillmentOrder = readObject(payload?.['movedFulfillmentOrder']);
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Move did not return movedFulfillmentOrder: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return fulfillmentOrder;
}

function serviceIds(captureResult: CaptureResult): { id: string; locationId: string } {
  const payload = readObject(data(captureResult)['fulfillmentServiceCreate']);
  const service = readObject(payload?.['fulfillmentService']);
  const location = readObject(service?.['location']);
  const id = service?.['id'];
  const locationId = location?.['id'];
  if (typeof id !== 'string' || typeof locationId !== 'string') {
    throw new Error(`Unable to read fulfillment service ids: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return { id, locationId };
}

function orderIdFromCreate(captureResult: CaptureResult): string {
  const orderCreate = readObject(data(captureResult)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  return readStringField(order ?? {}, 'id', 'created order id');
}

function payload(result: CaptureResult): ConformanceGraphqlPayload<JsonRecord> {
  return result.response.payload;
}

function upstreamCallFromHydrate(captureResult: CaptureResult): JsonRecord {
  return {
    operationName: 'ShippingFulfillmentOrderHydrate',
    variables: captureResult.variables,
    query: captureResult.query,
    response: {
      status: captureResult.response.status,
      body: captureResult.response.payload,
    },
  };
}

async function capture(query: string, variables: JsonRecord = {}): Promise<CaptureResult> {
  const response = await client.runGraphqlRequest<JsonRecord>(query, variables);
  return {
    query: trimGraphql(query),
    variables,
    response,
  };
}

async function cleanupOrder(orderId: string): Promise<unknown> {
  const result = await client.runGraphqlRequest(orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
  return result.payload.data;
}

await mkdir(outputDir, { recursive: true });

const capturedAt = new Date().toISOString();
const stamp = capturedAt.replace(/[-:.TZ]/gu, '').slice(0, 14);
let serviceId: string | null = null;
let orderId: string | null = null;
const cleanup: JsonRecord = {};

try {
  const serviceCreate = await capture(fulfillmentServiceCreateMutation, {
    name: `Hermes close state ${Date.now()}`,
    callbackUrl: 'https://mock.shop/fulfillment-order-close-state',
  });
  assertNoUserErrors(serviceCreate, 'fulfillmentServiceCreate', 'fulfillmentServiceCreate');
  const service = serviceIds(serviceCreate);
  serviceId = service.id;

  const orderCreate = await capture(orderCreateMutation, {
    order: {
      email: `fulfillment-order-close-state-${stamp}@example.com`,
      note: `Fulfillment-order close state ${stamp}`,
      tags: ['fulfillment-order-close-state'],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'Hermes',
        lastName: 'Conformance',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          title: `Fulfillment-order close state ${stamp}`,
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
  });
  assertNoUserErrors(orderCreate, 'orderCreate', 'orderCreate');
  orderId = orderIdFromCreate(orderCreate);

  const createdFulfillmentOrder = firstFulfillmentOrder(orderCreate);
  const move = await capture(moveMutation, {
    id: readStringField(createdFulfillmentOrder, 'id', 'created fulfillment order id'),
    newLocationId: service.locationId,
  });
  assertNoUserErrors(move, 'fulfillmentOrderMove', 'fulfillmentOrderMove');
  const movedOrder = movedFulfillmentOrder(move);
  const movedOrderId = readStringField(movedOrder, 'id', 'moved fulfillment order id');

  const upstreamCalls = [upstreamCallFromHydrate(await capture(hydrateQuery, { id: movedOrderId }))];

  const submit = await capture(submitRequestMutation, {
    id: movedOrderId,
    fulfillmentOrderLineItems: null,
    message: 'Close state submit',
    notifyCustomer: false,
  });
  assertNoUserErrors(submit, 'fulfillmentOrderSubmitFulfillmentRequest', 'submit fulfillment request');

  const accept = await capture(acceptRequestMutation, {
    id: movedOrderId,
    message: 'Close state accept',
    estimatedShippedAt: '2026-04-27T00:00:00Z',
  });
  assertNoUserErrors(accept, 'fulfillmentOrderAcceptFulfillmentRequest', 'accept fulfillment request');

  const close = await capture(closeMutation, {
    id: movedOrderId,
    message: 'Close state success',
  });
  assertNoUserErrors(close, 'fulfillmentOrderClose', 'fulfillmentOrderClose');

  const afterClose = await capture(directReadQuery, { id: movedOrderId });

  cleanup[`orderCancel:${orderId}`] = await cleanupOrder(orderId);
  orderId = null;
  cleanup['fulfillmentServiceDelete'] = payload(await capture(fulfillmentServiceDeleteMutation, { id: serviceId }));
  serviceId = null;

  const fixture = {
    capturedAt,
    storeDomain,
    apiVersion,
    setup: {
      fulfillmentServiceCreate: {
        response: payload(serviceCreate),
      },
      orderCreate: {
        response: payload(orderCreate),
      },
      move: {
        response: payload(move),
      },
      notes:
        'Disposable order was created, moved to a temporary API fulfillment-service location, submitted, accepted, then closed.',
    },
    submit: {
      query: submit.query,
      variables: submit.variables,
      response: payload(submit),
    },
    accept: {
      query: accept.query,
      variables: accept.variables,
      response: payload(accept),
    },
    close: {
      query: close.query,
      variables: close.variables,
      response: payload(close),
    },
    afterClose: {
      query: afterClose.query,
      variables: afterClose.variables,
      response: payload(afterClose),
    },
    cleanup,
    upstreamCalls,
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
} finally {
  if (orderId) {
    cleanup[`orderCancel:${orderId}`] = await cleanupOrder(orderId);
  }
  if (serviceId) {
    cleanup['fulfillmentServiceDeleteAfterError'] = await client.runGraphqlRequest(fulfillmentServiceDeleteMutation, {
      id: serviceId,
    });
  }
}
