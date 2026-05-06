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

const scenarioId = 'fulfillment-order-request-lifecycle';
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
const submitCancellationRequestMutation = readRequest(
  'fulfillment-order-submit-cancellation-request-lifecycle.graphql',
);
const acceptCancellationRequestMutation = readRequest(
  'fulfillment-order-accept-cancellation-request-lifecycle.graphql',
);
const rejectRequestMutation = readRequest('fulfillment-order-reject-request-lifecycle.graphql');
const rejectCancellationRequestMutation = readRequest(
  'fulfillment-order-reject-cancellation-request-lifecycle.graphql',
);

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderRequestLifecycleFields on FulfillmentOrder {
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
    merchantRequests(first: 10) {
      nodes {
        kind
        message
        requestOptions
        responseData
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
`;

const orderCreateMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderRequestLifecycleOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
        fulfillmentOrders(first: 5) {
          nodes {
            ...FulfillmentOrderRequestLifecycleFields
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
  mutation FulfillmentOrderRequestLifecycleOrderCancel(
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
  mutation FulfillmentOrderRequestLifecycleServiceCreate($name: String!) {
    fulfillmentServiceCreate(
      name: $name
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
  mutation FulfillmentOrderRequestLifecycleServiceDelete($id: ID!) {
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
  mutation FulfillmentOrderRequestLifecycleMove($id: ID!, $newLocationId: ID!) {
    fulfillmentOrderMove(id: $id, newLocationId: $newLocationId) {
      movedFulfillmentOrder {
        ...FulfillmentOrderRequestLifecycleFields
      }
      originalFulfillmentOrder {
        ...FulfillmentOrderRequestLifecycleFields
      }
      remainingFulfillmentOrder {
        ...FulfillmentOrderRequestLifecycleFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const hydrateQuery = `#graphql
  query ShippingFulfillmentOrderHydrate($id: ID!) {
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
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

function isJsonRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readObject(value: unknown): JsonRecord | null {
  return isJsonRecord(value) ? value : null;
}

function readStringField(record: JsonRecord, field: string, label: string): string {
  const value = record[field];
  if (typeof value !== 'string') {
    throw new Error(`Unable to read ${label}: ${JSON.stringify(record)}`);
  }

  return value;
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
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

function isOrderCreateThrottle(captureResult: CaptureResult): boolean {
  return userErrors(captureResult, 'orderCreate').some((error) => {
    const record = readObject(error);
    return record?.['message'] === 'Too many attempts. Please try again later.';
  });
}

async function capture(query: string, variables: JsonRecord = {}): Promise<CaptureResult> {
  const response = await client.runGraphqlRequest<JsonRecord>(query, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`GraphQL request failed: ${JSON.stringify(response.payload)}`);
  }
  return {
    query: trimGraphql(query),
    variables,
    response,
  };
}

function firstFulfillmentOrder(captureResult: CaptureResult): JsonRecord {
  const orderCreate = readObject(data(captureResult)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  const fulfillmentOrder = readNodes(readObject(order?.['fulfillmentOrders']))[0];
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(captureResult.response)}`);
  }
  return fulfillmentOrder;
}

function firstLineItem(fulfillmentOrder: JsonRecord): JsonRecord {
  const lineItem = readNodes(readObject(fulfillmentOrder['lineItems']))[0];
  if (!lineItem || typeof lineItem['id'] !== 'string') {
    throw new Error(`Fulfillment order has no line item: ${JSON.stringify(fulfillmentOrder)}`);
  }
  return lineItem;
}

function movedFulfillmentOrder(captureResult: CaptureResult): JsonRecord {
  const payload = readObject(data(captureResult)['fulfillmentOrderMove']);
  const fulfillmentOrder = readObject(payload?.['movedFulfillmentOrder']);
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Move did not return movedFulfillmentOrder: ${JSON.stringify(captureResult.response)}`);
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
    throw new Error(`Unable to read fulfillment service ids: ${JSON.stringify(captureResult.response)}`);
  }
  return { id, locationId };
}

function orderFromCreate(captureResult: CaptureResult): { id: string; fulfillmentOrder: JsonRecord } {
  const orderCreate = readObject(data(captureResult)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  const id = order?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`Unable to create order: ${JSON.stringify(captureResult.response)}`);
  }
  return { id, fulfillmentOrder: firstFulfillmentOrder(captureResult) };
}

function orderResultFromCreate(captureResult: CaptureResult): { orderId: string; fulfillmentOrder: JsonRecord } {
  const order = orderFromCreate(captureResult);
  return { orderId: order.id, fulfillmentOrder: order.fulfillmentOrder };
}

function payload(captureResult: CaptureResult): ConformanceGraphqlPayload<JsonRecord> {
  return captureResult.response.payload;
}

function upstreamCallFromHydrate(captureResult: CaptureResult): JsonRecord {
  return {
    operationName: 'ShippingFulfillmentOrderHydrate',
    variables: captureResult.variables,
    query: captureResult.query,
    response: {
      status: captureResult.response.status,
      body: payload(captureResult),
    },
  };
}

async function createOrder(
  label: string,
  quantity: number,
): Promise<{ orderId: string; fulfillmentOrder: JsonRecord }> {
  const stamp = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
  const variables: JsonRecord = {
    order: {
      email: `hermes-${label}-${stamp}@example.com`,
      note: `Hermes fulfillment-order request lifecycle ${label} ${stamp}`,
      tags: ['hermes-conformance', 'fulfillment-order-request-lifecycle', label],
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
          title: `Hermes fulfillment-order request ${label} ${stamp}`,
          quantity,
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
  let create = await capture(orderCreateMutation, variables);
  for (let attempt = 1; isOrderCreateThrottle(create) && attempt <= 4; attempt += 1) {
    console.log(`orderCreate ${label} throttled; retrying after backoff (${attempt}/4)`);
    await sleep(15_000 * attempt);
    create = await capture(orderCreateMutation, variables);
  }
  assertNoUserErrors(create, 'orderCreate', `orderCreate ${label}`);
  return orderResultFromCreate(create);
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
const serviceCreate = await capture(fulfillmentServiceCreateMutation, {
  name: `Hermes Request Lifecycle ${Date.now()}`,
});
assertNoUserErrors(serviceCreate, 'fulfillmentServiceCreate', 'fulfillmentServiceCreate');
const service = serviceIds(serviceCreate);
const createdOrderIds: string[] = [];
const cleanup: JsonRecord = {};

try {
  const partial = await createOrder('partial', 2);
  const reject = await createOrder('reject', 1);
  const rejectCancellation = await createOrder('reject-cancellation', 1);
  createdOrderIds.push(partial.orderId, reject.orderId, rejectCancellation.orderId);

  const partialMove = await capture(moveMutation, {
    id: readStringField(partial.fulfillmentOrder, 'id', 'partial fulfillment order id'),
    newLocationId: service.locationId,
  });
  assertNoUserErrors(partialMove, 'fulfillmentOrderMove', 'move partial order');
  const partialOrder = movedFulfillmentOrder(partialMove);
  const partialLine = firstLineItem(partialOrder);

  const rejectMove = await capture(moveMutation, {
    id: readStringField(reject.fulfillmentOrder, 'id', 'reject fulfillment order id'),
    newLocationId: service.locationId,
  });
  assertNoUserErrors(rejectMove, 'fulfillmentOrderMove', 'move reject order');
  const rejectOrder = movedFulfillmentOrder(rejectMove);

  const rejectCancellationMove = await capture(moveMutation, {
    id: readStringField(rejectCancellation.fulfillmentOrder, 'id', 'reject-cancellation fulfillment order id'),
    newLocationId: service.locationId,
  });
  assertNoUserErrors(rejectCancellationMove, 'fulfillmentOrderMove', 'move reject-cancellation order');
  const rejectCancellationOrder = movedFulfillmentOrder(rejectCancellationMove);

  const upstreamCalls: JsonRecord[] = [];
  const partialOrderId = readStringField(partialOrder, 'id', 'moved partial fulfillment order id');
  const rejectOrderId = readStringField(rejectOrder, 'id', 'moved reject fulfillment order id');
  const rejectCancellationOrderId = readStringField(
    rejectCancellationOrder,
    'id',
    'moved reject-cancellation fulfillment order id',
  );
  upstreamCalls.push(upstreamCallFromHydrate(await capture(hydrateQuery, { id: partialOrderId })));
  upstreamCalls.push(upstreamCallFromHydrate(await capture(hydrateQuery, { id: rejectOrderId })));
  upstreamCalls.push(upstreamCallFromHydrate(await capture(hydrateQuery, { id: rejectCancellationOrderId })));

  const partialVariables = {
    id: partialOrderId,
    fulfillmentOrderLineItems: [
      {
        id: readStringField(partialLine, 'id', 'partial fulfillment order line item id'),
        quantity: 1,
      },
    ],
    message: 'Hermes partial submit',
    notifyCustomer: false,
  };
  const partialSubmit = await capture(submitRequestMutation, partialVariables);
  assertNoUserErrors(partialSubmit, 'fulfillmentOrderSubmitFulfillmentRequest', 'partial submit');

  const acceptVariables = {
    id: partialOrderId,
    message: 'Hermes accepted',
    estimatedShippedAt: '2026-04-27T00:00:00Z',
  };
  const acceptFulfillmentRequest = await capture(acceptRequestMutation, acceptVariables);
  assertNoUserErrors(
    acceptFulfillmentRequest,
    'fulfillmentOrderAcceptFulfillmentRequest',
    'accept fulfillment request',
  );

  const submitCancellationVariables = {
    id: partialOrderId,
    message: 'Hermes cancel requested',
  };
  const submitCancellationRequest = await capture(submitCancellationRequestMutation, submitCancellationVariables);
  assertNoUserErrors(
    submitCancellationRequest,
    'fulfillmentOrderSubmitCancellationRequest',
    'submit cancellation request',
  );

  const acceptCancellationVariables = {
    id: partialOrderId,
    message: 'Hermes accepted cancellation',
  };
  const acceptCancellationRequest = await capture(acceptCancellationRequestMutation, acceptCancellationVariables);
  assertNoUserErrors(
    acceptCancellationRequest,
    'fulfillmentOrderAcceptCancellationRequest',
    'accept cancellation request',
  );

  const rejectSubmitVariables = {
    id: rejectOrderId,
    fulfillmentOrderLineItems: null,
    message: 'Hermes submit then reject',
    notifyCustomer: false,
  };
  const rejectSubmit = await capture(submitRequestMutation, rejectSubmitVariables);
  assertNoUserErrors(rejectSubmit, 'fulfillmentOrderSubmitFulfillmentRequest', 'submit before reject');
  const rejectVariables = {
    id: rejectOrderId,
    message: 'Hermes rejected',
  };
  const rejectFulfillmentRequest = await capture(rejectRequestMutation, rejectVariables);
  assertNoUserErrors(
    rejectFulfillmentRequest,
    'fulfillmentOrderRejectFulfillmentRequest',
    'reject fulfillment request',
  );

  const rejectCancellationSubmitVariables = {
    id: rejectCancellationOrderId,
    fulfillmentOrderLineItems: null,
    message: 'Hermes submit cancel reject',
    notifyCustomer: false,
  };
  const rejectCancellationSubmit = await capture(submitRequestMutation, rejectCancellationSubmitVariables);
  assertNoUserErrors(
    rejectCancellationSubmit,
    'fulfillmentOrderSubmitFulfillmentRequest',
    'submit before cancellation reject',
  );
  const rejectCancellationAcceptVariables = {
    id: rejectCancellationOrderId,
    message: 'Hermes accepted before cancellation reject',
    estimatedShippedAt: '2026-04-27T00:00:00Z',
  };
  const rejectCancellationAccept = await capture(acceptRequestMutation, rejectCancellationAcceptVariables);
  assertNoUserErrors(
    rejectCancellationAccept,
    'fulfillmentOrderAcceptFulfillmentRequest',
    'accept before cancellation reject',
  );
  const rejectCancellationSubmitCancelVariables = {
    id: rejectCancellationOrderId,
    message: 'Hermes cancellation to reject',
  };
  const rejectCancellationSubmitCancel = await capture(
    submitCancellationRequestMutation,
    rejectCancellationSubmitCancelVariables,
  );
  assertNoUserErrors(
    rejectCancellationSubmitCancel,
    'fulfillmentOrderSubmitCancellationRequest',
    'submit cancellation before reject',
  );
  const rejectCancellationVariables = {
    id: rejectCancellationOrderId,
    message: 'Hermes cancel rejected',
  };
  const rejectCancellationRequest = await capture(rejectCancellationRequestMutation, rejectCancellationVariables);
  assertNoUserErrors(
    rejectCancellationRequest,
    'fulfillmentOrderRejectCancellationRequest',
    'reject cancellation request',
  );

  for (const orderId of createdOrderIds) {
    cleanup[`orderCancel:${orderId}`] = await cleanupOrder(orderId);
  }
  cleanup['fulfillmentServiceDelete'] = payload(await capture(fulfillmentServiceDeleteMutation, { id: service.id }));

  const fixture = {
    capturedAt,
    storeDomain,
    apiVersion,
    setup: {
      fulfillmentServiceCreate: {
        response: payload(serviceCreate),
      },
      notes:
        'Disposable orders were created with orderCreate and moved to a temporary API fulfillment-service location before request/cancellation lifecycle probes.',
    },
    invalidIdBranches: {
      id: 'gid://shopify/FulfillmentOrder/0',
      expectedError: {
        messagePattern: 'Invalid id: gid://shopify/FulfillmentOrder/0',
        extensions: {
          code: 'RESOURCE_NOT_FOUND',
        },
      },
      operations: [
        'fulfillmentOrderSubmitFulfillmentRequest',
        'fulfillmentOrderAcceptFulfillmentRequest',
        'fulfillmentOrderRejectFulfillmentRequest',
        'fulfillmentOrderSubmitCancellationRequest',
        'fulfillmentOrderAcceptCancellationRequest',
        'fulfillmentOrderRejectCancellationRequest',
      ],
    },
    partialSubmit: {
      operationName: 'fulfillmentOrderSubmitFulfillmentRequest',
      variables: partialVariables,
      response: payload(partialSubmit),
    },
    acceptFulfillmentRequest: {
      operationName: 'fulfillmentOrderAcceptFulfillmentRequest',
      variables: acceptVariables,
      response: payload(acceptFulfillmentRequest),
    },
    submitCancellationRequest: {
      operationName: 'fulfillmentOrderSubmitCancellationRequest',
      variables: submitCancellationVariables,
      response: payload(submitCancellationRequest),
    },
    acceptCancellationRequest: {
      operationName: 'fulfillmentOrderAcceptCancellationRequest',
      variables: acceptCancellationVariables,
      response: payload(acceptCancellationRequest),
    },
    rejectFulfillmentRequest: {
      operationName: 'fulfillmentOrderRejectFulfillmentRequest',
      variables: rejectVariables,
      response: payload(rejectFulfillmentRequest),
    },
    rejectCancellationRequest: {
      operationName: 'fulfillmentOrderRejectCancellationRequest',
      variables: rejectCancellationVariables,
      response: payload(rejectCancellationRequest),
    },
    cleanup,
    upstreamCalls,
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} catch (error) {
  for (const orderId of createdOrderIds) {
    cleanup[`orderCancelAfterError:${orderId}`] = await cleanupOrder(orderId);
  }
  cleanup['fulfillmentServiceDeleteAfterError'] = await client.runGraphqlRequest(fulfillmentServiceDeleteMutation, {
    id: service.id,
  });
  throw error;
}
