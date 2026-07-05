/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CaptureStep = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const scenarioId = 'order-create-fulfillment-assigned-location';
const expectedApiVersion = '2025-01';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: expectedApiVersion,
  exitOnMissing: true,
});

if (apiVersion !== expectedApiVersion) {
  throw new Error(`${scenarioId} requires SHOPIFY_CONFORMANCE_API_VERSION=${expectedApiVersion}, got ${apiVersion}`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const requestDir = path.join('config', 'parity-requests', 'orders');
const specPath = path.join('config', 'parity-specs', 'orders', 'orderCreate-fulfillment-assigned-location.json');
const locationsRequestPath = path.join(requestDir, 'orderCreate-fulfillment-assigned-location-locations.graphql');
const createRequestPath = path.join(requestDir, 'orderCreate-fulfillment-assigned-location.graphql');
const readRequestPath = path.join(requestDir, 'orderCreate-fulfillment-assigned-location-read.graphql');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'order-create-fulfillment-assigned-location.json');

const fulfillmentOrderFields = `#graphql
  fragment OrderCreateFulfillmentAssignedLocationFields on FulfillmentOrder {
    id
    status
    assignedLocation {
      name
      location {
        id
        name
      }
    }
  }
`;

const locationsDocument = `#graphql
  query OrderCreateFulfillmentAssignedLocationLocations($first: Int!) {
    locationsAvailableForDeliveryProfilesConnection(first: $first) {
      nodes {
        id
        name
        isActive
        isFulfillmentService
      }
    }
  }
`;

const orderCreateDocument = `#graphql
  ${fulfillmentOrderFields}
  mutation OrderCreateFulfillmentAssignedLocation($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        fulfillmentOrders(first: 5) {
          nodes {
            ...OrderCreateFulfillmentAssignedLocationFields
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

const orderReadDocument = `#graphql
  ${fulfillmentOrderFields}
  query OrderCreateFulfillmentAssignedLocationRead($id: ID!) {
    order(id: $id) {
      id
      name
      fulfillmentOrders(first: 5) {
        nodes {
          ...OrderCreateFulfillmentAssignedLocationFields
        }
      }
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation OrderCreateFulfillmentAssignedLocationCleanup(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job {
        id
        done
      }
      userErrors {
        field
        message
      }
    }
  }
`;

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${payload.trim()}\n`, 'utf8');
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readArray(value: unknown, key: string): unknown[] {
  const fieldValue = asRecord(value)?.[key];
  return Array.isArray(fieldValue) ? fieldValue : [];
}

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function capture(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  const trimmed = trimGraphql(query);
  const response = await runGraphqlRequest<JsonRecord>(trimmed, variables);
  assertNoTopLevelErrors(response, context);
  return { query: trimmed, variables, response };
}

function orderCreatePayload(step: CaptureStep): JsonRecord {
  const payload = readRecord(step.response.payload.data, 'orderCreate');
  if (!payload) {
    throw new Error(`orderCreate response is missing payload: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return payload;
}

function orderFromCreate(step: CaptureStep): JsonRecord {
  const payload = orderCreatePayload(step);
  const order = readRecord(payload, 'order');
  const userErrors = readArray(payload, 'userErrors');
  if (!order || userErrors.length > 0) {
    throw new Error(`orderCreate did not create an order: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return order;
}

function orderIdFromCreate(step: CaptureStep): string {
  const id = readString(orderFromCreate(step), 'id');
  if (!id) {
    throw new Error(`orderCreate did not return an order id: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return id;
}

function firstFulfillmentOrder(order: JsonRecord | null): JsonRecord {
  const nodes = readArray(readRecord(order, 'fulfillmentOrders'), 'nodes');
  const fulfillmentOrder = asRecord(nodes[0]);
  if (!fulfillmentOrder) {
    throw new Error(`order payload did not include a fulfillment order: ${JSON.stringify(order, null, 2)}`);
  }
  return fulfillmentOrder;
}

function assignedLocation(fulfillmentOrder: JsonRecord): JsonRecord {
  const assigned = readRecord(fulfillmentOrder, 'assignedLocation');
  if (!assigned) {
    throw new Error(`fulfillment order did not include assignedLocation: ${JSON.stringify(fulfillmentOrder, null, 2)}`);
  }
  return assigned;
}

function assignedLocationId(fulfillmentOrder: JsonRecord): string {
  const location = readRecord(assignedLocation(fulfillmentOrder), 'location');
  const id = readString(location, 'id');
  if (!id) {
    throw new Error(`assignedLocation did not include location.id: ${JSON.stringify(fulfillmentOrder, null, 2)}`);
  }
  return id;
}

function assertCapturedAssignedLocation(step: CaptureStep, context: string): void {
  const id = assignedLocationId(firstFulfillmentOrder(orderFromCreate(step)));
  if (id === 'gid://shopify/Location/1') {
    throw new Error(`${context} unexpectedly used the placeholder Location/1 id`);
  }
}

function assertObservedLocations(step: CaptureStep): void {
  const nodes = readArray(
    readRecord(step.response.payload.data, 'locationsAvailableForDeliveryProfilesConnection'),
    'nodes',
  );
  const assignable = nodes
    .map((node) => asRecord(node))
    .filter(
      (node): node is JsonRecord =>
        !!node &&
        readString(node, 'id') !== null &&
        node['isActive'] !== false &&
        node['isFulfillmentService'] !== true,
    );
  if (assignable.length === 0) {
    throw new Error(`no assignable delivery-profile locations were captured: ${JSON.stringify(step.response.payload)}`);
  }
}

function specPayload(): JsonRecord {
  return {
    scenarioId,
    operationNames: ['orderCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'assigned-location-parity', 'downstream-read-parity'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: locationsRequestPath,
      variablesCapturePath: '$.operations.locations.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live 2025-01 Shopify capture for orderCreate with a shippable line item after a public delivery-profile locations read. The replay observes the store location through the same public GraphQL surface, then proves the created fulfillment order uses that real assigned location in both the mutation payload and downstream order(id:) read instead of a fixed Location/1 placeholder.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'observed-delivery-profile-locations',
          capturePath: '$.operations.locations.response.payload.data',
          proxyPath: '$.data',
        },
        {
          name: 'mutation-fulfillment-order-assigned-location',
          capturePath: '$.operations.create.response.payload.data.orderCreate',
          proxyPath: '$.data.orderCreate',
          selectedPaths: ['$.order.fulfillmentOrders.nodes[*].assignedLocation', '$.userErrors'],
          proxyRequest: {
            documentPath: createRequestPath,
            variablesCapturePath: '$.operations.create.variables',
            apiVersion,
          },
        },
        {
          name: 'downstream-fulfillment-order-assigned-location',
          capturePath: '$.operations.downstreamRead.response.payload.data.order',
          proxyPath: '$.data.order',
          selectedPaths: ['$.fulfillmentOrders.nodes[*].assignedLocation'],
          proxyRequest: {
            documentPath: readRequestPath,
            variables: {
              id: {
                fromPreviousProxyPath: '$.data.orderCreate.order.id',
              },
            },
            apiVersion,
          },
        },
      ],
    },
  };
}

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const createdOrderIds: string[] = [];
const cleanup: CaptureStep[] = [];

await writeText(locationsRequestPath, trimGraphql(locationsDocument));
await writeText(createRequestPath, trimGraphql(orderCreateDocument));
await writeText(readRequestPath, trimGraphql(orderReadDocument));
await writeJson(specPath, specPayload());

try {
  const locations = await capture(locationsDocument, { first: 1 }, 'delivery-profile locations read');
  assertObservedLocations(locations);

  const create = await capture(
    orderCreateDocument,
    {
      order: {
        email: `assigned-location-${stamp}@example.com`,
        note: `assigned location parity ${stamp}`,
        tags: ['assigned-location-parity'],
        test: true,
        currency: 'USD',
        lineItems: [
          {
            title: `Assigned location shippable item ${stamp}`,
            quantity: 1,
            sku: `ASSIGNED-LOCATION-${stamp}`,
            requiresShipping: true,
            taxable: false,
            priceSet: {
              shopMoney: {
                amount: '12.50',
                currencyCode: 'USD',
              },
            },
            taxLines: [],
          },
        ],
      },
      options: {
        inventoryBehaviour: 'BYPASS',
        sendReceipt: false,
        sendFulfillmentReceipt: false,
      },
    },
    'orderCreate',
  );
  assertCapturedAssignedLocation(create, 'orderCreate');
  const orderId = orderIdFromCreate(create);
  createdOrderIds.push(orderId);

  const downstreamRead = await capture(orderReadDocument, { id: orderId }, 'downstream order read');
  const downstreamOrder = asRecord(downstreamRead.response.payload.data?.['order']);
  const createAssignedLocationId = assignedLocationId(firstFulfillmentOrder(orderFromCreate(create)));
  const downstreamAssignedLocationId = assignedLocationId(firstFulfillmentOrder(downstreamOrder));
  if (downstreamAssignedLocationId !== createAssignedLocationId) {
    throw new Error(
      `downstream assigned location ${downstreamAssignedLocationId} did not match mutation ${createAssignedLocationId}`,
    );
  }

  for (const cleanupOrderId of createdOrderIds) {
    cleanup.push(
      await capture(
        orderCancelDocument,
        {
          orderId: cleanupOrderId,
          reason: 'OTHER',
          notifyCustomer: false,
          restock: true,
        },
        `cleanup ${cleanupOrderId}`,
      ),
    );
  }

  await writeJson(fixturePath, {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes:
      'Captured from live Shopify Admin GraphQL. A delivery-profile locations read is recorded before creating a shippable test order so parity replay can observe location state through the public request surface; the order is cancelled in cleanup.',
    operations: {
      locations,
      create,
      downstreamRead,
    },
    cleanup,
  });

  console.log(`Wrote ${fixturePath}`);
  console.log(`Wrote ${specPath}`);
  console.log(`Wrote ${locationsRequestPath}`);
  console.log(`Wrote ${createRequestPath}`);
  console.log(`Wrote ${readRequestPath}`);
} catch (error) {
  for (const cleanupOrderId of createdOrderIds) {
    try {
      cleanup.push(
        await capture(
          orderCancelDocument,
          {
            orderId: cleanupOrderId,
            reason: 'OTHER',
            notifyCustomer: false,
            restock: true,
          },
          `cleanup after failure ${cleanupOrderId}`,
        ),
      );
    } catch (cleanupError) {
      console.error(`Failed to clean up ${cleanupOrderId}:`, cleanupError);
    }
  }
  if (cleanup.length > 0) {
    await writeJson(path.join(fixtureDir, 'order-create-fulfillment-assigned-location-cleanup.json'), {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cleanup,
    });
  }
  throw error;
}
