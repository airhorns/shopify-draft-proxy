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

type CreatedOrder = {
  id: string;
  fulfillmentOrderId: string;
  fulfillmentOrderLineItemId: string;
};

const scenarioId = 'fulfillment-order-mixed-catalog-lifecycle';
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

const requestDir = path.join('config', 'parity-requests', 'shipping-fulfillments');
const requestPaths = {
  hold: path.join(requestDir, 'fulfillment-order-mixed-catalog-hold.graphql'),
  move: path.join(requestDir, 'fulfillment-order-mixed-catalog-move.graphql'),
  split: path.join(requestDir, 'fulfillment-order-mixed-catalog-split.graphql'),
  cancel: path.join(requestDir, 'fulfillment-order-mixed-catalog-cancel.graphql'),
  catalog: path.join(requestDir, 'fulfillment-order-mixed-catalog-read.graphql'),
  page: path.join(requestDir, 'fulfillment-order-mixed-catalog-page.graphql'),
} as const;

function readRequest(name: keyof typeof requestPaths): string {
  return readFileSync(requestPaths[name], 'utf8');
}

const documents = {
  hold: readRequest('hold'),
  move: readRequest('move'),
  split: readRequest('split'),
  cancel: readRequest('cancel'),
  catalog: readRequest('catalog'),
  page: readRequest('page'),
};

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const orderCreateMutation = `#graphql
  mutation FulfillmentOrderMixedCatalogOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
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
                  fulfillableQuantity
                }
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

const orderCancelMutation = `#graphql
  mutation FulfillmentOrderMixedCatalogOrderCancel(
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
  mutation FulfillmentOrderMixedCatalogServiceCreate($name: String!) {
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
  mutation FulfillmentOrderMixedCatalogServiceDelete($id: ID!) {
    fulfillmentServiceDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

// Byte-for-byte copy of SHIPPING_FULFILLMENT_ORDER_HYDRATE_QUERY.
const storeBackedFulfillmentOrderHydrateQuery = `
query ShippingFulfillmentOrderHydrate($id: ID!) {
  node(id: $id) {
    __typename
    ... on FulfillmentOrder {
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
      lineItems(first: 250) {
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
}
`;

// Byte-for-byte copy of ORDERS_FULFILLMENT_ORDER_HYDRATE_QUERY.
const ordersFulfillmentOrderHydrateQuery =
  'query ShippingFulfillmentOrderHydrate($id: ID!) {\n    fulfillmentOrder(id: $id) {\n      id\n      status\n      requestStatus\n      fulfillAt\n      fulfillBy\n      updatedAt\n      supportedActions {\n        action\n      }\n      assignedLocation {\n        name\n        location {\n          id\n          name\n        }\n      }\n      fulfillmentHolds {\n        id\n        handle\n        reason\n        reasonNotes\n        displayReason\n        heldByApp {\n          id\n          title\n        }\n        heldByRequestingApp\n      }\n      merchantRequests(first: 10) {\n        nodes {\n          kind\n          message\n          requestOptions\n        }\n      }\n      lineItems(first: 20) {\n        nodes {\n          id\n          totalQuantity\n          remainingQuantity\n          lineItem {\n            id\n            title\n            quantity\n            fulfillableQuantity\n          }\n        }\n      }\n      order {\n        id\n        name\n        displayFulfillmentStatus\n      }\n    }\n  }';

const locationHydrateQuery =
  'query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }';

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

function payload(captureResult: CaptureResult): ConformanceGraphqlPayload<JsonRecord> {
  return captureResult.response.payload;
}

function data(captureResult: CaptureResult): JsonRecord {
  return readObject(payload(captureResult).data) ?? {};
}

function userErrors(captureResult: CaptureResult, root: string): unknown[] {
  const rootPayload = readObject(data(captureResult)[root]);
  const errors = rootPayload?.['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors(captureResult: CaptureResult, root: string, label: string): void {
  const errors = userErrors(captureResult, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function readString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Expected ${label} to be a non-empty string, got ${JSON.stringify(value)}`);
  }
  return value;
}

function firstFulfillmentOrder(create: CaptureResult): JsonRecord {
  const orderCreate = readObject(data(create)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  const fulfillmentOrder = readNodes(readObject(order?.['fulfillmentOrders']))[0];
  if (!fulfillmentOrder) {
    throw new Error(`Created order has no fulfillment order: ${JSON.stringify(payload(create))}`);
  }
  return fulfillmentOrder;
}

function firstLineItem(fulfillmentOrder: JsonRecord): JsonRecord {
  const lineItem = readNodes(readObject(fulfillmentOrder['lineItems']))[0];
  if (!lineItem) {
    throw new Error(`Fulfillment order has no line item: ${JSON.stringify(fulfillmentOrder)}`);
  }
  return lineItem;
}

function createdOrder(create: CaptureResult): CreatedOrder {
  const orderCreate = readObject(data(create)['orderCreate']);
  const order = readObject(orderCreate?.['order']);
  const fulfillmentOrder = firstFulfillmentOrder(create);
  const lineItem = firstLineItem(fulfillmentOrder);
  return {
    id: readString(order?.['id'], 'created order id'),
    fulfillmentOrderId: readString(fulfillmentOrder['id'], 'created fulfillment order id'),
    fulfillmentOrderLineItemId: readString(lineItem['id'], 'created fulfillment order line item id'),
  };
}

function serviceIds(serviceCreate: CaptureResult): { id: string; locationId: string } {
  const root = readObject(data(serviceCreate)['fulfillmentServiceCreate']);
  const service = readObject(root?.['fulfillmentService']);
  const location = readObject(service?.['location']);
  return {
    id: readString(service?.['id'], 'fulfillment service id'),
    locationId: readString(location?.['id'], 'fulfillment service location id'),
  };
}

function movedFulfillmentOrderId(move: CaptureResult): string {
  const root = readObject(data(move)['fulfillmentOrderMove']);
  const moved = readObject(root?.['movedFulfillmentOrder']);
  return readString(moved?.['id'], 'moved fulfillment order id');
}

function splitFulfillmentOrderId(split: CaptureResult): string {
  const root = readObject(data(split)['fulfillmentOrderSplit']);
  const results = Array.isArray(root?.['fulfillmentOrderSplits']) ? root?.['fulfillmentOrderSplits'] : [];
  const first = readObject(results[0]);
  const fulfillmentOrder = readObject(first?.['fulfillmentOrder']);
  return readString(fulfillmentOrder?.['id'], 'split fulfillment order id');
}

function cancelledFulfillmentOrderId(cancel: CaptureResult): string {
  const root = readObject(data(cancel)['fulfillmentOrderCancel']);
  const fulfillmentOrder = readObject(root?.['fulfillmentOrder']);
  return readString(fulfillmentOrder?.['id'], 'cancelled fulfillment order id');
}

function endCursor(captureResult: CaptureResult, root = 'page'): string {
  const page = readObject(data(captureResult)[root]);
  const pageInfo = readObject(page?.['pageInfo']);
  return readString(pageInfo?.['endCursor'], `${root}.pageInfo.endCursor`);
}

function startCursor(captureResult: CaptureResult, root = 'page'): string {
  const page = readObject(data(captureResult)[root]);
  const pageInfo = readObject(page?.['pageInfo']);
  return readString(pageInfo?.['startCursor'], `${root}.pageInfo.startCursor`);
}

function gidTail(id: string): string {
  return id.split('/').filter(Boolean).at(-1) ?? id;
}

function idQuery(ids: string[]): string {
  return `(${ids.map((id) => `id:${gidTail(id)}`).join(' OR ')})`;
}

function upstreamCallFromCapture(operationName: string, captureResult: CaptureResult): JsonRecord {
  return {
    operationName,
    query: captureResult.query,
    variables: captureResult.variables,
    response: {
      status: captureResult.response.status,
      body: payload(captureResult),
    },
  };
}

async function capture(
  query: string,
  variables: JsonRecord = {},
  options: { preserveQuery?: boolean } = {},
): Promise<CaptureResult> {
  const response = await client.runGraphqlRequest<JsonRecord>(query, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`GraphQL request failed: ${JSON.stringify(response.payload)}`);
  }
  return {
    query: options.preserveQuery ? query : trimGraphql(query),
    variables,
    response,
  };
}

function isOrderCreateThrottle(captureResult: CaptureResult): boolean {
  return userErrors(captureResult, 'orderCreate').some((error) => {
    const record = readObject(error);
    return record?.['message'] === 'Too many attempts. Please try again later.';
  });
}

async function sleep(milliseconds: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}

async function createOrder(label: string, quantity: number): Promise<{ create: CaptureResult; order: CreatedOrder }> {
  const stamp = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
  const variables = {
    order: {
      email: `hermes-mixed-catalog-${label}-${stamp}@example.com`,
      note: `Hermes fulfillment-order mixed catalog ${label} ${stamp}`,
      tags: ['hermes-conformance', 'fulfillment-order-mixed-catalog', label],
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
          title: `Hermes mixed catalog ${label} ${stamp}`,
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
  return { create, order: createdOrder(create) };
}

async function cleanupOrder(orderId: string): Promise<unknown> {
  const result = await client.runGraphqlRequest(orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
  return result.payload;
}

async function cleanupService(serviceId: string): Promise<unknown> {
  const result = await client.runGraphqlRequest(fulfillmentServiceDeleteMutation, { id: serviceId });
  return result.payload;
}

const capturedAt = new Date().toISOString();
const startedAt = new Date(Date.now() - 60_000).toISOString();
const createdOrderIds: string[] = [];
const cleanup: JsonRecord = {};
let serviceId: string | null = null;

try {
  const serviceCreate = await capture(fulfillmentServiceCreateMutation, {
    name: `Hermes Mixed Catalog ${Date.now()}`,
  });
  assertNoUserErrors(serviceCreate, 'fulfillmentServiceCreate', 'fulfillmentServiceCreate');
  const service = serviceIds(serviceCreate);
  serviceId = service.id;

  const baseline = await createOrder('baseline-open', 1);
  const held = await createOrder('held', 1);
  const moved = await createOrder('moved', 1);
  const split = await createOrder('split', 3);
  const cancelled = await createOrder('cancelled', 1);
  createdOrderIds.push(baseline.order.id, held.order.id, moved.order.id, split.order.id, cancelled.order.id);

  const upstreamCalls: JsonRecord[] = [];
  upstreamCalls.push(
    upstreamCallFromCapture(
      'ShippingFulfillmentOrderHydrate',
      await capture(
        storeBackedFulfillmentOrderHydrateQuery,
        { id: held.order.fulfillmentOrderId },
        { preserveQuery: true },
      ),
    ),
  );
  const hold = await capture(documents.hold, {
    id: held.order.fulfillmentOrderId,
    fulfillmentHold: {
      reason: 'OTHER',
      reasonNotes: 'Hermes mixed catalog hold',
      notifyMerchant: false,
      externalId: 'hermes-mixed-catalog-hold',
      handle: 'hermes-mixed-catalog-hold',
    },
  });
  assertNoUserErrors(hold, 'fulfillmentOrderHold', 'fulfillmentOrderHold');

  upstreamCalls.push(
    upstreamCallFromCapture(
      'ShippingFulfillmentOrderHydrate',
      await capture(
        storeBackedFulfillmentOrderHydrateQuery,
        { id: moved.order.fulfillmentOrderId },
        { preserveQuery: true },
      ),
    ),
  );
  upstreamCalls.push(
    upstreamCallFromCapture(
      'StorePropertiesLocationHydrate',
      await capture(locationHydrateQuery, { id: service.locationId }, { preserveQuery: true }),
    ),
  );
  const move = await capture(documents.move, {
    id: moved.order.fulfillmentOrderId,
    newLocationId: service.locationId,
  });
  assertNoUserErrors(move, 'fulfillmentOrderMove', 'fulfillmentOrderMove');
  const movedId = movedFulfillmentOrderId(move);
  if (movedId !== moved.order.fulfillmentOrderId) {
    throw new Error(
      `Full fulfillmentOrderMove returned a new id (${movedId}); local replay expects original id ${moved.order.fulfillmentOrderId}`,
    );
  }

  upstreamCalls.push(
    upstreamCallFromCapture(
      'ShippingFulfillmentOrderHydrate',
      await capture(
        ordersFulfillmentOrderHydrateQuery,
        { id: split.order.fulfillmentOrderId },
        { preserveQuery: true },
      ),
    ),
  );
  const splitMutation = await capture(documents.split, {
    fulfillmentOrderSplits: [
      {
        fulfillmentOrderId: split.order.fulfillmentOrderId,
        fulfillmentOrderLineItems: [
          {
            id: split.order.fulfillmentOrderLineItemId,
            quantity: 1,
          },
        ],
      },
    ],
  });
  assertNoUserErrors(splitMutation, 'fulfillmentOrderSplit', 'fulfillmentOrderSplit');
  const splitId = splitFulfillmentOrderId(splitMutation);
  if (splitId !== split.order.fulfillmentOrderId) {
    throw new Error(
      `fulfillmentOrderSplit returned a different original id (${splitId}); local replay expects ${split.order.fulfillmentOrderId}`,
    );
  }

  upstreamCalls.push(
    upstreamCallFromCapture(
      'ShippingFulfillmentOrderHydrate',
      await capture(
        storeBackedFulfillmentOrderHydrateQuery,
        { id: cancelled.order.fulfillmentOrderId },
        { preserveQuery: true },
      ),
    ),
  );
  const cancel = await capture(documents.cancel, { id: cancelled.order.fulfillmentOrderId });
  assertNoUserErrors(cancel, 'fulfillmentOrderCancel', 'fulfillmentOrderCancel');
  const cancelledId = cancelledFulfillmentOrderId(cancel);
  if (cancelledId !== cancelled.order.fulfillmentOrderId) {
    throw new Error(
      `fulfillmentOrderCancel returned a different original id (${cancelledId}); local replay expects ${cancelled.order.fulfillmentOrderId}`,
    );
  }

  const stableIds = [
    baseline.order.fulfillmentOrderId,
    held.order.fulfillmentOrderId,
    moved.order.fulfillmentOrderId,
    split.order.fulfillmentOrderId,
    cancelled.order.fulfillmentOrderId,
  ];
  const catalogIdQuery = idQuery(stableIds);
  const serviceLocationTail = gidTail(service.locationId);
  const catalogVariables = {
    first: 20,
    idQuery: catalogIdQuery,
    heldQuery: `${catalogIdQuery} AND status:ON_HOLD`,
    closedQuery: `${catalogIdQuery} AND status:CLOSED`,
    locationQuery: `${catalogIdQuery} AND assigned_location_id:${serviceLocationTail}`,
    updatedQuery: `${catalogIdQuery} AND updated_at:>=2026-01-01T00:00:00Z`,
    locationIds: [service.locationId],
  };
  const catalog = await capture(documents.catalog, catalogVariables);
  upstreamCalls.push(upstreamCallFromCapture('FulfillmentOrderMixedCatalogRead', catalog));

  const page1Variables = {
    first: 2,
    last: null,
    after: null,
    before: null,
    idQuery: catalogIdQuery,
  };
  const page1 = await capture(documents.page, page1Variables);
  upstreamCalls.push(upstreamCallFromCapture('FulfillmentOrderMixedCatalogPage', page1));

  const pageAfterVariables = {
    first: 2,
    last: null,
    after: endCursor(page1),
    before: null,
    idQuery: catalogIdQuery,
  };
  const pageAfter = await capture(documents.page, pageAfterVariables);
  upstreamCalls.push(upstreamCallFromCapture('FulfillmentOrderMixedCatalogPageAfter', pageAfter));

  const pageBeforeVariables = {
    first: null,
    last: 1,
    after: null,
    before: startCursor(pageAfter),
    idQuery: catalogIdQuery,
  };
  const pageBefore = await capture(documents.page, pageBeforeVariables);
  upstreamCalls.push(upstreamCallFromCapture('FulfillmentOrderMixedCatalogPageBefore', pageBefore));

  const output = {
    fixtureKind: scenarioId,
    storeDomain,
    apiVersion,
    capturedAt,
    startedAt,
    service,
    createdOrders: {
      baseline: baseline.order,
      held: held.order,
      moved: moved.order,
      split: split.order,
      cancelled: cancelled.order,
    },
    operations: {
      hold,
      move,
      split: splitMutation,
      cancel,
      catalog,
      page1,
      pageAfter,
      pageBefore,
    },
    cleanup,
    upstreamCalls,
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath, scenarioId }, null, 2));
} finally {
  for (const orderId of createdOrderIds) {
    try {
      cleanup[`orderCancel:${orderId}`] = await cleanupOrder(orderId);
    } catch (error) {
      console.error(`Cleanup orderCancel failed for ${orderId}:`, error);
    }
  }
  if (serviceId) {
    try {
      cleanup[`fulfillmentServiceDelete:${serviceId}`] = await cleanupService(serviceId);
    } catch (error) {
      console.error(`Cleanup fulfillmentServiceDelete failed for ${serviceId}:`, error);
    }
  }
}
