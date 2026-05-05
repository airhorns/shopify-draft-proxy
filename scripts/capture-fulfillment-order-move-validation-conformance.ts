/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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

type CreatedOrder = {
  id: string;
  name: string | null;
  fulfillmentOrder: JsonRecord;
  fulfillmentOrderId: string;
  assignedLocationId: string | null;
};

const scenarioId = 'fulfillment-order-move-validation';
const invalidLocationId = 'gid://shopify/Location/999999999';

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

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const fulfillmentOrderFields = `#graphql
  fragment FulfillmentOrderMoveValidationFields on FulfillmentOrder {
    id
    status
    requestStatus
    updatedAt
    assignedLocation {
      name
      location {
        id
        name
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
          quantity
          fulfillableQuantity
        }
      }
    }
  }
`;

const hydrateQuery = `#graphql
  query FulfillmentOrderMoveValidationHydrate($first: Int!) {
    locationsAvailableForDeliveryProfilesConnection(first: $first) {
      nodes {
        id
        name
        localPickupSettingsV2 {
          pickupTime
          instructions
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderMoveValidationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        fulfillmentOrders(first: 5) {
          nodes {
            ...FulfillmentOrderMoveValidationFields
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
  mutation FulfillmentOrderMoveValidationOrderCancel(
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

const fulfillmentServiceCreateMutation = `#graphql
  mutation FulfillmentOrderMoveValidationServiceCreate($name: String!) {
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
        location {
          id
          name
          isFulfillmentService
          fulfillsOnlineOrders
          shipsInventory
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
  mutation FulfillmentOrderMoveValidationServiceDelete($id: ID!) {
    fulfillmentServiceDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentOrderCancelMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderMoveValidationCancel($id: ID!) {
    fulfillmentOrderCancel(id: $id) {
      fulfillmentOrder {
        ...FulfillmentOrderMoveValidationFields
      }
      replacementFulfillmentOrder {
        id
        status
        requestStatus
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const reportProgressMutation = `#graphql
  mutation FulfillmentOrderMoveValidationReportProgress(
    $id: ID!
    $progressReport: FulfillmentOrderReportProgressInput
  ) {
    fulfillmentOrderReportProgress(id: $id, progressReport: $progressReport) {
      fulfillmentOrder {
        id
        status
        requestStatus
        assignedLocation {
          name
          location {
            id
            name
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

const submitRequestMutation = `#graphql
  mutation FulfillmentOrderMoveValidationSubmitRequest(
    $id: ID!
    $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]
    $message: String
    $notifyCustomer: Boolean
  ) {
    fulfillmentOrderSubmitFulfillmentRequest(
      id: $id
      fulfillmentOrderLineItems: $fulfillmentOrderLineItems
      message: $message
      notifyCustomer: $notifyCustomer
    ) {
      originalFulfillmentOrder {
        id
        status
        requestStatus
      }
      submittedFulfillmentOrder {
        id
        status
        requestStatus
      }
      unsubmittedFulfillmentOrder {
        id
        status
        requestStatus
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const rejectRequestMutation = `#graphql
  mutation FulfillmentOrderMoveValidationRejectRequest($id: ID!, $message: String) {
    fulfillmentOrderRejectFulfillmentRequest(id: $id, message: $message) {
      fulfillmentOrder {
        id
        status
        requestStatus
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const moveMutation = `#graphql
  ${fulfillmentOrderFields}
  mutation FulfillmentOrderMoveValidationMove(
    $id: ID!
    $newLocationId: ID!
    $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]
  ) {
    fulfillmentOrderMove(
      id: $id
      newLocationId: $newLocationId
      fulfillmentOrderLineItems: $fulfillmentOrderLineItems
    ) {
      movedFulfillmentOrder {
        ...FulfillmentOrderMoveValidationFields
      }
      originalFulfillmentOrder {
        ...FulfillmentOrderMoveValidationFields
      }
      remainingFulfillmentOrder {
        ...FulfillmentOrderMoveValidationFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  const response = await runGraphqlRequest(query, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`GraphQL request failed: ${JSON.stringify(response.payload)}`);
  }
  return {
    query: trimGraphql(query),
    variables,
    response,
  };
}

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null ? (value as JsonRecord) : null;
}

function readNodes(value: unknown): JsonRecord[] {
  const record = readObject(value);
  const nodes = record?.['nodes'];
  return Array.isArray(nodes) ? nodes.filter((node): node is JsonRecord => readObject(node) !== null) : [];
}

function data(captureResult: GraphqlCapture): JsonRecord {
  return readObject(captureResult.response.payload.data) ?? {};
}

function userErrors(captureResult: GraphqlCapture, root: string): unknown[] {
  const payload = readObject(data(captureResult)[root]);
  const errors = payload?.['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors(captureResult: GraphqlCapture, root: string, label: string): void {
  const errors = userErrors(captureResult, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function firstFulfillmentOrderFromCreate(captureResult: GraphqlCapture): JsonRecord {
  const orderCreate = readObject(data(captureResult)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  const fulfillmentOrder = readNodes(readObject(order?.['fulfillmentOrders']))[0];
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return fulfillmentOrder;
}

function createdOrderFromCapture(captureResult: GraphqlCapture): CreatedOrder {
  const orderCreate = readObject(data(captureResult)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  if (!order || typeof order['id'] !== 'string') {
    throw new Error(`Unable to create disposable order: ${JSON.stringify(captureResult.response.payload)}`);
  }
  const fulfillmentOrder = firstFulfillmentOrderFromCreate(captureResult);
  return {
    id: order['id'],
    name: typeof order['name'] === 'string' ? order['name'] : null,
    fulfillmentOrder,
    fulfillmentOrderId: fulfillmentOrder['id'] as string,
    assignedLocationId: assignedLocationId(fulfillmentOrder),
  };
}

function assignedLocationId(fulfillmentOrder: JsonRecord): string | null {
  const assignedLocation = readObject(fulfillmentOrder['assignedLocation']);
  const location = readObject(assignedLocation?.['location']);
  const id = location?.['id'];
  return typeof id === 'string' ? id : null;
}

function locationIdsFromHydrate(captureResult: GraphqlCapture): string[] {
  const locations = readObject(data(captureResult)['locationsAvailableForDeliveryProfilesConnection']);
  return readNodes(locations)
    .map((location) => location['id'])
    .filter((id): id is string => typeof id === 'string');
}

function alternateLocationId(locationIds: string[], current: string | null): string {
  const alternate = locationIds.find((id) => id !== current);
  if (!alternate) {
    throw new Error(`Need at least two active locations to capture ${scenarioId}; saw ${JSON.stringify(locationIds)}`);
  }
  return alternate;
}

function fulfillmentServiceIds(captureResult: GraphqlCapture): { id: string; locationId: string } {
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

function movedFulfillmentOrder(captureResult: GraphqlCapture): JsonRecord {
  const payload = readObject(data(captureResult)['fulfillmentOrderMove']);
  const moved = readObject(payload?.['movedFulfillmentOrder']);
  if (!moved || typeof moved['id'] !== 'string') {
    throw new Error(`Move did not return movedFulfillmentOrder: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return moved;
}

function payloadFulfillmentOrder(captureResult: GraphqlCapture, root: string): JsonRecord {
  const payload = readObject(data(captureResult)[root]);
  const fulfillmentOrder = readObject(payload?.['fulfillmentOrder']);
  if (!fulfillmentOrder || typeof fulfillmentOrder['id'] !== 'string') {
    throw new Error(`${root} did not return fulfillmentOrder: ${JSON.stringify(captureResult.response.payload)}`);
  }
  return fulfillmentOrder;
}

function fulfillmentOrderHydrateCall(fulfillmentOrder: JsonRecord): JsonRecord {
  return {
    operationName: 'ShippingFulfillmentOrderHydrate',
    variables: { id: fulfillmentOrder['id'] },
    query: 'sha:hand-synthesized-from-har-555-capture',
    response: {
      status: 200,
      body: {
        data: {
          fulfillmentOrder,
        },
      },
    },
  };
}

function upstreamCallFromCapture(operationName: string, captureResult: GraphqlCapture): JsonRecord {
  return {
    operationName,
    variables: captureResult.variables,
    query: captureResult.query,
    response: {
      status: captureResult.response.status,
      body: captureResult.response.payload,
    },
  };
}

function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

function isOrderCreateThrottle(captureResult: GraphqlCapture): boolean {
  return userErrors(captureResult, 'orderCreate').some((error) => {
    const record = readObject(error);
    return record?.['message'] === 'Too many attempts. Please try again later.';
  });
}

async function createMoveValidationOrder(label: string): Promise<{ order: CreatedOrder; create: GraphqlCapture }> {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:.TZ]/gu, '')
    .slice(0, 14);
  const variables = {
    order: {
      email: `har-555-${label}-${stamp}@example.com`,
      note: `HAR-555 fulfillment-order move validation ${label} ${stamp}`,
      tags: ['har-555', 'fulfillment-order-move-validation', label],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'HAR',
        lastName: 'MoveValidation',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          title: `HAR-555 fulfillment item ${label} ${stamp}`,
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
  let create = await capture(orderCreateMutation, variables);
  for (let attempt = 1; isOrderCreateThrottle(create) && attempt <= 4; attempt += 1) {
    console.log(`orderCreate ${label} throttled; retrying after backoff (${attempt}/4)`);
    await sleep(15_000 * attempt);
    create = await capture(orderCreateMutation, variables);
  }
  assertNoUserErrors(create, 'orderCreate', `orderCreate ${label}`);
  return {
    order: createdOrderFromCapture(create),
    create,
  };
}

async function cleanupOrder(order: CreatedOrder): Promise<GraphqlCapture> {
  return capture(orderCancelMutation, {
    orderId: order.id,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
}

const startedAt = new Date().toISOString();
const createdOrders: CreatedOrder[] = [];
const cleanup: Record<string, unknown> = {};
let fulfillmentServiceId: string | null = null;

try {
  const hydrate = await capture(hydrateQuery, { first: 10 });
  const locationIds = locationIdsFromHydrate(hydrate);
  if (locationIds.length < 2) {
    throw new Error(`Need at least two active locations to capture ${scenarioId}; saw ${JSON.stringify(locationIds)}`);
  }

  const closed = await createMoveValidationOrder('closed');
  createdOrders.push(closed.order);
  const progress = await createMoveValidationOrder('progress');
  createdOrders.push(progress.order);
  const requestStatus = await createMoveValidationOrder('request-status');
  createdOrders.push(requestStatus.order);
  const happyPath = await createMoveValidationOrder('happy-path');
  createdOrders.push(happyPath.order);
  const unknownLocation = await createMoveValidationOrder('unknown-location');
  createdOrders.push(unknownLocation.order);

  const destinationLocationId = alternateLocationId(locationIds, happyPath.order.assignedLocationId);
  const serviceCreate = await capture(fulfillmentServiceCreateMutation, {
    name: `HAR555 Move Validation ${Date.now()}`,
  });
  assertNoUserErrors(serviceCreate, 'fulfillmentServiceCreate', 'fulfillmentServiceCreate');
  const service = fulfillmentServiceIds(serviceCreate);
  fulfillmentServiceId = service.id;

  const moveToService = await capture(moveMutation, {
    id: requestStatus.order.fulfillmentOrderId,
    newLocationId: service.locationId,
    fulfillmentOrderLineItems: null,
  });
  assertNoUserErrors(moveToService, 'fulfillmentOrderMove', 'move request-status order to service location');
  const requestStatusFulfillmentOrder = movedFulfillmentOrder(moveToService);

  const closedCancel = await capture(fulfillmentOrderCancelMutation, {
    id: closed.order.fulfillmentOrderId,
  });
  assertNoUserErrors(closedCancel, 'fulfillmentOrderCancel', 'fulfillmentOrderCancel setup');
  const closedFulfillmentOrder = payloadFulfillmentOrder(closedCancel, 'fulfillmentOrderCancel');
  const closedMove = await capture(moveMutation, {
    id: closedFulfillmentOrder['id'],
    newLocationId: destinationLocationId,
    fulfillmentOrderLineItems: null,
  });

  const reportProgress = await capture(reportProgressMutation, {
    id: progress.order.fulfillmentOrderId,
    progressReport: {
      reasonNotes: 'HAR-555 report progress before move validation',
    },
  });
  assertNoUserErrors(reportProgress, 'fulfillmentOrderReportProgress', 'fulfillmentOrderReportProgress setup');
  const progressMove = await capture(moveMutation, {
    id: progress.order.fulfillmentOrderId,
    newLocationId: destinationLocationId,
    fulfillmentOrderLineItems: null,
  });

  const submitRequest = await capture(submitRequestMutation, {
    id: requestStatusFulfillmentOrder['id'],
    fulfillmentOrderLineItems: null,
    message: 'HAR-555 submit request before move validation',
    notifyCustomer: false,
  });
  assertNoUserErrors(
    submitRequest,
    'fulfillmentOrderSubmitFulfillmentRequest',
    'fulfillmentOrderSubmitFulfillmentRequest setup',
  );
  const requestStatusMove = await capture(moveMutation, {
    id: requestStatusFulfillmentOrder['id'],
    newLocationId: destinationLocationId,
    fulfillmentOrderLineItems: null,
  });

  const happyMove = await capture(moveMutation, {
    id: happyPath.order.fulfillmentOrderId,
    newLocationId: destinationLocationId,
    fulfillmentOrderLineItems: null,
  });
  assertNoUserErrors(happyMove, 'fulfillmentOrderMove', 'happy-path fulfillmentOrderMove');

  const unknownMove = await capture(moveMutation, {
    id: unknownLocation.order.fulfillmentOrderId,
    newLocationId: invalidLocationId,
    fulfillmentOrderLineItems: null,
  });

  cleanup['rejectSubmittedRequest'] = await capture(rejectRequestMutation, {
    id: requestStatusFulfillmentOrder['id'],
    message: 'HAR-555 cleanup reject submitted request',
  });

  for (const order of createdOrders) {
    cleanup[`cancelOrder:${order.id}`] = await cleanupOrder(order);
  }
  if (fulfillmentServiceId) {
    cleanup['deleteFulfillmentService'] = await capture(fulfillmentServiceDeleteMutation, {
      id: fulfillmentServiceId,
    });
  }

  const output = {
    metadata: {
      issue: 'HAR-555',
      scenarioId,
      capturedAt: new Date().toISOString(),
      startedAt,
      storeDomain,
      apiVersion,
      destinationLocationId,
      invalidLocationId,
      scopedRoots: [
        'fulfillmentOrderReportProgress',
        'fulfillmentOrderSubmitFulfillmentRequest',
        'fulfillmentOrderMove',
      ],
      createdOrders,
    },
    setup: {
      hydrate,
      serviceCreate,
      moveRequestOrderToService: moveToService,
    },
    closed: {
      create: closed.create,
      serviceFulfillmentOrder: closedFulfillmentOrder,
      cancel: closedCancel,
      moveAfterClose: closedMove,
    },
    progress: {
      create: progress.create,
      reportProgress,
      moveAfterProgress: progressMove,
    },
    requestStatus: {
      create: requestStatus.create,
      serviceFulfillmentOrder: requestStatusFulfillmentOrder,
      submitRequest,
      moveAfterSubmit: requestStatusMove,
    },
    happyPath: {
      create: happyPath.create,
      move: happyMove,
    },
    unknownLocation: {
      create: unknownLocation.create,
      move: unknownMove,
    },
    cleanup,
    upstreamCalls: [
      upstreamCallFromCapture('FulfillmentOrderMoveValidationHydrate', hydrate),
      fulfillmentOrderHydrateCall(closedFulfillmentOrder),
      fulfillmentOrderHydrateCall(progress.order.fulfillmentOrder),
      fulfillmentOrderHydrateCall(requestStatusFulfillmentOrder),
      fulfillmentOrderHydrateCall(happyPath.order.fulfillmentOrder),
      fulfillmentOrderHydrateCall(unknownLocation.order.fulfillmentOrder),
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  console.log(`Captured fulfillment-order move validation fixture: ${outputPath}`);
} catch (error) {
  console.error((error as Error).message);
  for (const order of createdOrders) {
    try {
      cleanup[`cancelOrderAfterError:${order.id}`] = await cleanupOrder(order);
    } catch (cleanupError) {
      console.error(`Cleanup order ${order.id} failed: ${(cleanupError as Error).message}`);
    }
  }
  if (fulfillmentServiceId) {
    try {
      cleanup['deleteFulfillmentServiceAfterError'] = await capture(fulfillmentServiceDeleteMutation, {
        id: fulfillmentServiceId,
      });
    } catch (cleanupError) {
      console.error(`Cleanup fulfillment service ${fulfillmentServiceId} failed: ${(cleanupError as Error).message}`);
    }
  }
  process.exitCode = 1;
}
