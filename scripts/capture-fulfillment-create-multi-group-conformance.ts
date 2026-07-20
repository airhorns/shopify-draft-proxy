/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type GraphqlStep = {
  query: string;
  variables: JsonRecord;
  status: number;
  response: JsonRecord;
};
type FulfillmentOrderGroup = {
  fulfillmentOrderId: string;
  fulfillmentOrderLineItemId: string;
  orderId: string;
  assignedLocationId: string;
};
type OrderSetup = {
  create: GraphqlStep;
  orderId: string;
  groups: FulfillmentOrderGroup[];
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'fulfillment-create-multi-group.json');
const mutationRequestPath = path.join('config', 'parity-requests', 'orders', 'fulfillmentCreate-preconditions.graphql');
const readRequestPath = path.join('config', 'parity-requests', 'orders', 'fulfillmentCreate-multi-group-read.graphql');
const specPath = path.join('config', 'parity-specs', 'orders', 'fulfillmentCreate-multi-group.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphqlRequest: (
    query: string,
    variables?: Record<string, unknown>,
  ) => Promise<{ status: number; payload: unknown }>;
};

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readNodes(value: unknown): JsonRecord[] {
  const nodes = readObject(value)?.['nodes'];
  return Array.isArray(nodes)
    ? nodes.flatMap((node) => (readObject(node) ? [readObject(node) as JsonRecord] : []))
    : [];
}

function readString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

function responseRoot(step: GraphqlStep, root: string): JsonRecord {
  const payload = readObject(readObject(step.response['data'])?.[root]);
  if (!payload) throw new Error(`${root} returned no payload: ${JSON.stringify(step.response)}`);
  return payload;
}

function userErrors(step: GraphqlStep, root: string): unknown[] {
  const errors = responseRoot(step, root)['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function requireNoUserErrors(step: GraphqlStep, root: string): void {
  const errors = userErrors(step, root);
  if (errors.length > 0) {
    throw new Error(`${root} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

async function capture(name: string, query: string, variables: JsonRecord = {}): Promise<GraphqlStep> {
  const result = await runGraphqlRequest(query, variables);
  const response = readObject(result.payload);
  if (result.status < 200 || result.status >= 300 || !response || response['errors']) {
    throw new Error(`${name} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return {
    query: query.replace(/^#graphql\n/u, '').trim(),
    variables,
    status: result.status,
    response,
  };
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, payload);
}

const fulfillmentOrderFields = `#graphql
fragment FulfillmentCreateMultiGroupOrderFields on FulfillmentOrder {
  id
  status
  requestStatus
  assignedLocation {
    name
    location { id name }
  }
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
`;

const orderCreateMutation = `#graphql
${fulfillmentOrderFields}
mutation FulfillmentCreateMultiGroupOrderCreate(
  $order: OrderCreateOrderInput!
  $options: OrderCreateOptionsInput
) {
  orderCreate(order: $order, options: $options) {
    order {
      id
      name
      displayFulfillmentStatus
      fulfillmentOrders(first: 10) {
        nodes { ...FulfillmentCreateMultiGroupOrderFields }
      }
    }
    userErrors { field message }
  }
}
`;

const orderReadQuery = `#graphql
${fulfillmentOrderFields}
query FulfillmentCreateMultiGroupOrderRead($id: ID!) {
  order(id: $id) {
    id
    name
    displayFulfillmentStatus
    fulfillmentOrders(first: 10) {
      nodes { ...FulfillmentCreateMultiGroupOrderFields }
    }
  }
}
`;

// Byte-for-byte copy of ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY. The parity
// cassette records this exact query once for every cold group resolution.
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

// This is the same public request shape used by fulfillmentCreate-preconditions.
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

const downstreamReadQuery = `#graphql
query FulfillmentCreateMultiGroupRead($orderId: ID!) {
  order(id: $orderId) {
    displayFulfillmentStatus
    fulfillments(first: 5) {
      status
      displayStatus
      fulfillmentLineItems(first: 10) {
        nodes {
          quantity
          lineItem { title }
        }
      }
    }
    fulfillmentOrders(first: 10) {
      nodes {
        status
        lineItems(first: 10) {
          nodes {
            remainingQuantity
            lineItem { title }
          }
        }
      }
    }
  }
}
`;

const fulfillmentOrderSplitMutation = `#graphql
${fulfillmentOrderFields}
mutation FulfillmentCreateMultiGroupSplit($fulfillmentOrderSplits: [FulfillmentOrderSplitInput!]!) {
  fulfillmentOrderSplit(fulfillmentOrderSplits: $fulfillmentOrderSplits) {
    fulfillmentOrderSplits {
      fulfillmentOrder { ...FulfillmentCreateMultiGroupOrderFields }
      remainingFulfillmentOrder { ...FulfillmentCreateMultiGroupOrderFields }
      replacementFulfillmentOrder { ...FulfillmentCreateMultiGroupOrderFields }
    }
    userErrors { field message code }
  }
}
`;

const fulfillmentOrderMoveMutation = `#graphql
${fulfillmentOrderFields}
mutation FulfillmentCreateMultiGroupMove(
  $id: ID!
  $newLocationId: ID!
  $fulfillmentOrderLineItems: [FulfillmentOrderLineItemInput!]
) {
  fulfillmentOrderMove(
    id: $id
    newLocationId: $newLocationId
    fulfillmentOrderLineItems: $fulfillmentOrderLineItems
  ) {
    movedFulfillmentOrder { ...FulfillmentCreateMultiGroupOrderFields }
    originalFulfillmentOrder { ...FulfillmentCreateMultiGroupOrderFields }
    remainingFulfillmentOrder { ...FulfillmentCreateMultiGroupOrderFields }
    userErrors { field message code }
  }
}
`;

const locationsQuery = `#graphql
query FulfillmentCreateMultiGroupLocations($first: Int!) {
  locations(first: $first) {
    nodes { id name fulfillsOnlineOrders isFulfillmentService }
  }
}
`;

const orderCancelMutation = `#graphql
mutation FulfillmentCreateMultiGroupOrderCancel(
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
mutation FulfillmentCreateMultiGroupOrderDelete($orderId: ID!) {
  orderDelete(orderId: $orderId) {
    deletedId
    userErrors { field message code }
  }
}
`;

function orderVariables(stamp: number, label: string, quantity: number): JsonRecord {
  return {
    order: {
      email: `fulfillment-create-multi-group-${label}-${stamp}@example.com`,
      note: `fulfillmentCreate multi-group ${label} ${stamp}`,
      tags: ['parity-probe', 'fulfillment-create-multi-group'],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'Conformance',
        lastName: 'Probe',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          title: `fulfillmentCreate multi-group ${label}`,
          quantity,
          priceSet: { shopMoney: { amount: '10.00', currencyCode: 'USD' } },
          requiresShipping: true,
          taxable: false,
          sku: `FULFILLMENT-CREATE-MULTI-GROUP-${label}-${stamp}`,
        },
      ],
      transactions: [
        {
          kind: 'SALE',
          status: 'SUCCESS',
          gateway: 'manual',
          test: true,
          amountSet: { shopMoney: { amount: String(10 * quantity), currencyCode: 'USD' } },
        },
      ],
    },
    options: { inventoryBehaviour: 'BYPASS', sendReceipt: false, sendFulfillmentReceipt: false },
  };
}

function groupsFromOrder(order: JsonRecord): FulfillmentOrderGroup[] {
  const orderId = readString(order['id'], 'order.id');
  return readNodes(order['fulfillmentOrders']).map((fulfillmentOrder, groupIndex) => {
    const lineItem = readNodes(fulfillmentOrder['lineItems'])[0];
    const assignedLocation = readObject(fulfillmentOrder['assignedLocation']);
    const location = readObject(assignedLocation?.['location']);
    return {
      orderId,
      fulfillmentOrderId: readString(fulfillmentOrder['id'], `fulfillmentOrders[${groupIndex}].id`),
      fulfillmentOrderLineItemId: readString(lineItem?.['id'], `fulfillmentOrders[${groupIndex}].lineItems[0].id`),
      assignedLocationId: readString(location?.['id'], `fulfillmentOrders[${groupIndex}].assignedLocation.location.id`),
    };
  });
}

async function readOrder(
  orderId: string,
): Promise<{ step: GraphqlStep; order: JsonRecord; groups: FulfillmentOrderGroup[] }> {
  const step = await capture('order.read', orderReadQuery, { id: orderId });
  const order = readObject(readObject(step.response['data'])?.['order']);
  if (!order) throw new Error(`order(${orderId}) returned null`);
  return { step, order, groups: groupsFromOrder(order) };
}

async function createOrder(stamp: number, label: string, quantity: number): Promise<OrderSetup> {
  let create: GraphqlStep | null = null;
  for (let attempt = 0; attempt < 6; attempt += 1) {
    create = await capture(`${label}.orderCreate`, orderCreateMutation, orderVariables(stamp, label, quantity));
    const errors = userErrors(create, 'orderCreate');
    if (errors.length === 0) break;
    const tooManyAttempts = errors.some((error) =>
      String(readObject(error)?.['message'] ?? '').includes('Too many attempts'),
    );
    if (!tooManyAttempts || attempt === 5) requireNoUserErrors(create, 'orderCreate');
    await new Promise((resolve) => setTimeout(resolve, 30_000 * (attempt + 1)));
  }
  if (!create) throw new Error(`${label}.orderCreate did not return a capture`);
  const order = readObject(responseRoot(create, 'orderCreate')['order']);
  if (!order) throw new Error(`${label}.orderCreate returned no order`);
  const groups = groupsFromOrder(order);
  if (groups.length !== 1) throw new Error(`${label}.orderCreate returned ${groups.length} fulfillment orders`);
  return { create, orderId: readString(order['id'], `${label}.order.id`), groups };
}

function explicitGroup(group: FulfillmentOrderGroup, quantity: number, lineItemId = group.fulfillmentOrderLineItemId) {
  return {
    fulfillmentOrderId: group.fulfillmentOrderId,
    fulfillmentOrderLineItems: [{ id: lineItemId, quantity }],
  };
}

function fulfillmentVariables(label: string, stamp: number, groups: unknown[]): JsonRecord {
  const trackingNumber = `FCMG-${label.toUpperCase()}-${stamp}`;
  return {
    fulfillment: {
      notifyCustomer: false,
      trackingInfo: {
        number: trackingNumber,
        url: `https://example.com/track/${trackingNumber}`,
        company: 'Hermes',
      },
      lineItemsByFulfillmentOrder: groups,
    },
    message: `fulfillmentCreate multi-group ${label}`,
  };
}

async function captureShippingHydrate(id: string): Promise<GraphqlStep> {
  return capture(`ShippingFulfillmentOrderHydrate(${id})`, shippingHydrateQuery, { id });
}

function recordedUpstreamCall(step: GraphqlStep): JsonRecord {
  return {
    method: 'POST',
    path: `/admin/api/${apiVersion}/graphql.json`,
    apiSurface: 'admin',
    apiVersion,
    operationName: 'ShippingFulfillmentOrderHydrate',
    variables: step.variables,
    query: step.query,
    response: { status: step.status, body: step.response },
  };
}

async function cleanupOrder(orderId: string): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};
  cleanup['cancel'] = await capture('cleanup.orderCancel', orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });
  cleanup['delete'] = await capture('cleanup.orderDelete', orderDeleteMutation, { orderId });
  return cleanup;
}

const stamp = Date.now();
const orderIds: string[] = [];
const cleanup: JsonRecord = {};

try {
  const trackedOrder = async (label: string, quantity: number): Promise<OrderSetup> => {
    const order = await createOrder(stamp, label, quantity);
    orderIds.push(order.orderId);
    return order;
  };

  const locations = await capture('locations', locationsQuery, { first: 20 });
  const locationIds = readNodes(readObject(locations.response['data'])?.['locations'])
    .map((location) => location['id'])
    .filter((id): id is string => typeof id === 'string');

  const unknown = await trackedOrder('unknown-later', 1);
  const differentOrderFirst = await trackedOrder('different-order-first', 1);
  const differentOrderSecond = await trackedOrder('different-order-second', 1);
  const differentLocation = await trackedOrder('different-location', 2);
  const valid = await trackedOrder('valid', 2);

  const differentLocationSource = differentLocation.groups[0];
  if (!differentLocationSource) throw new Error('different-location order has no fulfillment order');
  const alternateLocationId = locationIds.find((id) => id !== differentLocationSource.assignedLocationId);
  if (!alternateLocationId) throw new Error('No alternate active location is available for multi-location setup');

  const move = await capture('differentLocation.fulfillmentOrderMove', fulfillmentOrderMoveMutation, {
    id: differentLocationSource.fulfillmentOrderId,
    newLocationId: alternateLocationId,
    fulfillmentOrderLineItems: [{ id: differentLocationSource.fulfillmentOrderLineItemId, quantity: 1 }],
  });
  requireNoUserErrors(move, 'fulfillmentOrderMove');
  const differentLocationAfterMove = await readOrder(differentLocation.orderId);
  const differentLocationGroups = differentLocationAfterMove.groups.filter((group) => group.assignedLocationId);
  if (
    differentLocationGroups.length !== 2 ||
    new Set(differentLocationGroups.map((group) => group.assignedLocationId)).size !== 2
  ) {
    throw new Error(
      `Partial move did not create two active location groups: ${JSON.stringify(differentLocationGroups)}`,
    );
  }

  const validSource = valid.groups[0];
  if (!validSource) throw new Error('valid order has no fulfillment order');
  const split = await capture('valid.fulfillmentOrderSplit', fulfillmentOrderSplitMutation, {
    fulfillmentOrderSplits: [
      {
        fulfillmentOrderId: validSource.fulfillmentOrderId,
        fulfillmentOrderLineItems: [{ id: validSource.fulfillmentOrderLineItemId, quantity: 1 }],
      },
    ],
  });
  requireNoUserErrors(split, 'fulfillmentOrderSplit');
  const validAfterSplit = await readOrder(valid.orderId);
  const validGroups = validAfterSplit.groups;
  if (validGroups.length !== 2 || new Set(validGroups.map((group) => group.assignedLocationId)).size !== 1) {
    throw new Error(`Split did not create two same-location groups: ${JSON.stringify(validGroups)}`);
  }

  const unknownGroup = unknown.groups[0];
  const differentOrderGroupOne = differentOrderFirst.groups[0];
  const differentOrderGroupTwo = differentOrderSecond.groups[0];
  const differentLocationGroupOne = differentLocationGroups[0];
  const differentLocationGroupTwo = differentLocationGroups[1];
  const validGroupOne = validGroups[0];
  const validGroupTwo = validGroups[1];
  if (
    !unknownGroup ||
    !differentOrderGroupOne ||
    !differentOrderGroupTwo ||
    !differentLocationGroupOne ||
    !differentLocationGroupTwo ||
    !validGroupOne ||
    !validGroupTwo
  ) {
    throw new Error('Capture setup did not yield every required fulfillment-order group');
  }

  const missingFulfillmentOrderId = 'gid://shopify/FulfillmentOrder/999999999999999';
  const missingLineItemId = 'gid://shopify/FulfillmentOrderLineItem/999999999999999';
  const upstreamCalls: JsonRecord[] = [];
  const hydrateGroups = async (ids: string[]): Promise<void> => {
    for (const id of ids) upstreamCalls.push(recordedUpstreamCall(await captureShippingHydrate(id)));
  };

  await hydrateGroups([unknownGroup.fulfillmentOrderId, missingFulfillmentOrderId]);
  const unknownLater = await capture(
    'unknownLater.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables('unknown-later', stamp, [
      explicitGroup(unknownGroup, 1),
      { fulfillmentOrderId: missingFulfillmentOrderId },
    ]),
  );

  await hydrateGroups([differentOrderGroupOne.fulfillmentOrderId, differentOrderGroupTwo.fulfillmentOrderId]);
  const differentOrderLater = await capture(
    'differentOrderLater.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables('different-order-later', stamp, [
      explicitGroup(differentOrderGroupOne, 1),
      explicitGroup(differentOrderGroupTwo, 1),
    ]),
  );

  await hydrateGroups([differentLocationGroupOne.fulfillmentOrderId, differentLocationGroupTwo.fulfillmentOrderId]);
  const differentLocationLater = await capture(
    'differentLocationLater.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables('different-location-later', stamp, [
      explicitGroup(differentLocationGroupOne, 1),
      explicitGroup(differentLocationGroupTwo, 1),
    ]),
  );

  await hydrateGroups([validGroupOne.fulfillmentOrderId, validGroupTwo.fulfillmentOrderId]);
  const laterNonPositive = await capture(
    'laterNonPositive.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables('later-non-positive', stamp, [
      explicitGroup(validGroupOne, 1),
      explicitGroup(validGroupTwo, 0),
    ]),
  );

  await hydrateGroups([validGroupOne.fulfillmentOrderId, validGroupTwo.fulfillmentOrderId]);
  const laterMissingLine = await capture(
    'laterMissingLine.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables('later-missing-line', stamp, [
      explicitGroup(validGroupOne, 1),
      explicitGroup(validGroupTwo, 1, missingLineItemId),
    ]),
  );

  await hydrateGroups([validGroupOne.fulfillmentOrderId, validGroupTwo.fulfillmentOrderId]);
  const laterOverQuantity = await capture(
    'laterOverQuantity.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables('later-over-quantity', stamp, [
      explicitGroup(validGroupOne, 1),
      explicitGroup(validGroupTwo, 2),
    ]),
  );

  await hydrateGroups([unknownGroup.fulfillmentOrderId, missingFulfillmentOrderId]);
  const unknownBeforeQuantity = await capture(
    'unknownBeforeQuantity.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables('unknown-before-quantity', stamp, [
      explicitGroup(unknownGroup, 1),
      {
        fulfillmentOrderId: missingFulfillmentOrderId,
        fulfillmentOrderLineItems: [{ id: missingLineItemId, quantity: 0 }],
      },
    ]),
  );

  await hydrateGroups([differentOrderGroupOne.fulfillmentOrderId, differentOrderGroupTwo.fulfillmentOrderId]);
  const differentOrderBeforeQuantity = await capture(
    'differentOrderBeforeQuantity.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables('different-order-before-quantity', stamp, [
      explicitGroup(differentOrderGroupOne, 1),
      explicitGroup(differentOrderGroupTwo, 0),
    ]),
  );

  await hydrateGroups([differentLocationGroupOne.fulfillmentOrderId, differentLocationGroupTwo.fulfillmentOrderId]);
  const differentLocationBeforeQuantity = await capture(
    'differentLocationBeforeQuantity.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables('different-location-before-quantity', stamp, [
      explicitGroup(differentLocationGroupOne, 1),
      explicitGroup(differentLocationGroupTwo, 0),
    ]),
  );

  await hydrateGroups([validGroupOne.fulfillmentOrderId, validGroupTwo.fulfillmentOrderId]);
  const validMultiGroup = await capture(
    'validMultiGroup.fulfillmentCreate',
    fulfillmentCreateMutation,
    fulfillmentVariables('valid', stamp, [explicitGroup(validGroupOne, 1), explicitGroup(validGroupTwo, 1)]),
  );
  requireNoUserErrors(validMultiGroup, 'fulfillmentCreate');
  const downstreamRead = await capture('validMultiGroup.downstreamRead', downstreamReadQuery, {
    orderId: valid.orderId,
  });

  for (const orderId of [...orderIds].reverse()) cleanup[orderId] = await cleanupOrder(orderId);

  const cases = {
    unknownLater: { variables: unknownLater.variables, response: unknownLater.response },
    differentOrderLater: { variables: differentOrderLater.variables, response: differentOrderLater.response },
    differentLocationLater: {
      variables: differentLocationLater.variables,
      response: differentLocationLater.response,
    },
    laterNonPositive: { variables: laterNonPositive.variables, response: laterNonPositive.response },
    laterMissingLine: { variables: laterMissingLine.variables, response: laterMissingLine.response },
    laterOverQuantity: { variables: laterOverQuantity.variables, response: laterOverQuantity.response },
    unknownBeforeQuantity: {
      variables: unknownBeforeQuantity.variables,
      response: unknownBeforeQuantity.response,
    },
    differentOrderBeforeQuantity: {
      variables: differentOrderBeforeQuantity.variables,
      response: differentOrderBeforeQuantity.response,
    },
    differentLocationBeforeQuantity: {
      variables: differentLocationBeforeQuantity.variables,
      response: differentLocationBeforeQuantity.response,
    },
    validMultiGroup: {
      variables: validMultiGroup.variables,
      response: validMultiGroup.response,
      downstreamRead,
    },
  };

  await writeJson(fixturePath, {
    capturedAt: new Date().toISOString(),
    storeDomain,
    adminOrigin,
    apiVersion,
    setup: {
      locations,
      orders: {
        unknown: unknown.create,
        differentOrderFirst: differentOrderFirst.create,
        differentOrderSecond: differentOrderSecond.create,
        differentLocation: differentLocation.create,
        valid: valid.create,
      },
      differentLocation: { move, read: differentLocationAfterMove.step },
      valid: { split, read: validAfterSplit.step },
    },
    cases,
    upstreamCalls,
    cleanup,
  });
  await writeText(readRequestPath, `${downstreamReadQuery.replace(/^#graphql\n/u, '').trim()}\n`);

  const requestFor = (capturePath: string) => ({
    documentPath: mutationRequestPath,
    variables: { fromCapturePath: `${capturePath}.variables` },
    apiVersion,
  });
  const errorTarget = (name: string, capturePath: string, primary = false) => ({
    name,
    capturePath: `${capturePath}.response.data.fulfillmentCreate`,
    proxyPath: '$.data.fulfillmentCreate',
    ...(primary ? {} : { proxyRequest: requestFor(capturePath) }),
  });

  await writeJson(specPath, {
    scenarioId: 'fulfillmentCreate-multi-group',
    operationNames: ['fulfillmentCreate'],
    scenarioStatus: 'captured',
    assertionKinds: [
      'validation-parity',
      'user-errors-parity',
      'mutation-lifecycle',
      'downstream-read-parity',
      'no-upstream-passthrough',
    ],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/orders.rs'],
    proxyRequest: {
      documentPath: mutationRequestPath,
      variablesCapturePath: '$.cases.unknownLater.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        errorTarget('unknown-later-group', '$.cases.unknownLater', true),
        errorTarget('different-order-later-group', '$.cases.differentOrderLater'),
        errorTarget('different-location-later-group', '$.cases.differentLocationLater'),
        errorTarget('later-group-non-positive-quantity', '$.cases.laterNonPositive'),
        errorTarget('later-group-missing-line-item', '$.cases.laterMissingLine'),
        errorTarget('later-group-over-quantity', '$.cases.laterOverQuantity'),
        errorTarget('unknown-group-precedes-quantity', '$.cases.unknownBeforeQuantity'),
        errorTarget('different-order-precedes-quantity', '$.cases.differentOrderBeforeQuantity'),
        errorTarget('different-location-precedes-quantity', '$.cases.differentLocationBeforeQuantity'),
        {
          name: 'valid-multi-group-fulfillment',
          capturePath: '$.cases.validMultiGroup.response.data.fulfillmentCreate',
          proxyPath: '$.data.fulfillmentCreate',
          proxyRequest: requestFor('$.cases.validMultiGroup'),
          expectedDifferences: [
            {
              path: '$.fulfillment.id',
              matcher: 'shopify-gid:Fulfillment',
              reason: 'Shopify and the proxy allocate different fulfillment IDs.',
            },
            {
              path: '$.fulfillment.fulfillmentLineItems.nodes[*].id',
              matcher: 'shopify-gid:FulfillmentLineItem',
              reason: 'Shopify and the proxy allocate different fulfillment line item IDs.',
            },
          ],
        },
        {
          name: 'valid-multi-group-downstream-order',
          capturePath: '$.cases.validMultiGroup.downstreamRead.response.data.order',
          proxyPath: '$.data.order',
          proxyRequest: {
            documentPath: readRequestPath,
            variables: { orderId: { fromCapturePath: '$.setup.valid.read.response.data.order.id' } },
            apiVersion,
          },
        },
      ],
    },
    notes:
      'Live Admin API coverage for complete fulfillmentCreate group resolution: unknown later groups, cross-order and cross-location groups, later-group line-item validation and precedence, plus one same-order/same-location multi-group fulfillment and downstream order state.',
  });

  console.log(`Wrote ${fixturePath}`);
  console.log(`Wrote ${readRequestPath}`);
  console.log(`Wrote ${specPath}`);
} catch (error) {
  for (const orderId of [...orderIds].reverse()) {
    if (cleanup[orderId]) continue;
    try {
      cleanup[orderId] = await cleanupOrder(orderId);
    } catch (cleanupError) {
      console.error(`Cleanup failed for ${orderId}:`, cleanupError);
    }
  }
  throw error;
}
