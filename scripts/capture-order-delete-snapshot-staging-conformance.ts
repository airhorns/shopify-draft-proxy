/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
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

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'order-delete-snapshot-staging.json');
const specPath = path.join('config', 'parity-specs', 'orders', 'orderDelete-snapshot-staging.json');
const createRequestPath = path.join(
  'config',
  'parity-requests',
  'orders',
  'orderDelete-snapshot-staging-create.graphql',
);
const deleteRequestPath = path.join(
  'config',
  'parity-requests',
  'orders',
  'orderDelete-snapshot-staging-delete.graphql',
);
const readRequestPath = path.join('config', 'parity-requests', 'orders', 'orderDelete-snapshot-staging-read.graphql');

const orderCreateDocument = `#graphql
  mutation OrderDeleteSnapshotStagingCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        email
        displayFinancialStatus
        displayFulfillmentStatus
        fulfillmentOrders(first: 5) {
          nodes {
            id
            status
            requestStatus
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

const orderDeleteDocument = `#graphql
  mutation OrderDeleteSnapshotStagingDelete($orderId: ID!) {
    orderDelete(orderId: $orderId) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const fulfillmentCreateDocument = `#graphql
  mutation OrderDeleteSnapshotStagingFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        id
        status
        displayStatus
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const downstreamReadDocument = `#graphql
  query OrderDeleteSnapshotStagingRead($id: ID!, $query: String!) {
    order(id: $id) {
      id
      email
      displayFinancialStatus
      displayFulfillmentStatus
    }
    orders(first: 5, query: $query) {
      nodes {
        id
        email
        displayFinancialStatus
        displayFulfillmentStatus
      }
    }
    ordersCount(query: $query) {
      count
      precision
    }
  }
`;

const orderCancelDocument = `#graphql
  mutation OrderDeleteSnapshotStagingCleanupCancel(
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

function readNumber(value: unknown, key: string): number | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'number' ? fieldValue : null;
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

async function writeText(filePath: string, payload: string): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${payload.trim()}\n`, 'utf8');
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function capture(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  const response = await runGraphqlRequest<JsonRecord>(trimGraphql(query), variables);
  assertNoTopLevelErrors(response, context);
  return { query: trimGraphql(query), variables, response };
}

function orderCreatePayload(step: CaptureStep): JsonRecord {
  const payload = readRecord(step.response.payload.data, 'orderCreate');
  if (!payload) {
    throw new Error(`orderCreate response is missing payload: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return payload;
}

function orderDeletePayload(step: CaptureStep): JsonRecord {
  const payload = readRecord(step.response.payload.data, 'orderDelete');
  if (!payload) {
    throw new Error(`orderDelete response is missing payload: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return payload;
}

function orderFromCreate(step: CaptureStep): JsonRecord {
  const payload = orderCreatePayload(step);
  const order = readRecord(payload, 'order');
  const userErrors = readArray(payload, 'userErrors');
  if (!order || userErrors.length > 0) {
    throw new Error(`orderCreate did not produce a usable order: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return order;
}

function orderIdFromCreate(step: CaptureStep): string {
  const id = readString(orderFromCreate(step), 'id');
  if (!id) {
    throw new Error(`orderCreate response is missing order id: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
  return id;
}

function fulfillmentOrderIdFromCreate(step: CaptureStep): string {
  const fulfillmentOrder = asRecord(readArray(readRecord(orderFromCreate(step), 'fulfillmentOrders'), 'nodes')[0]);
  const id = readString(fulfillmentOrder, 'id');
  if (!id) {
    throw new Error(`orderCreate response is missing fulfillment order id: ${JSON.stringify(step.response.payload)}`);
  }
  return id;
}

function userErrors(payload: JsonRecord): unknown[] {
  return readArray(payload, 'userErrors');
}

function userErrorCodes(payload: JsonRecord): string[] {
  return userErrors(payload)
    .map((error) => readString(error, 'code'))
    .filter((code): code is string => code !== null);
}

function assertOrderDeleteSuccess(step: CaptureStep, orderId: string): void {
  const payload = orderDeletePayload(step);
  if (readString(payload, 'deletedId') !== orderId || userErrors(payload).length > 0) {
    throw new Error(
      `Expected successful orderDelete for ${orderId}: ${JSON.stringify(step.response.payload, null, 2)}`,
    );
  }
}

function assertOrderDeleteUserError(step: CaptureStep, expectedCode: string): void {
  const payload = orderDeletePayload(step);
  const codes = userErrorCodes(payload);
  if (readString(payload, 'deletedId') !== null || !codes.includes(expectedCode)) {
    throw new Error(`Expected orderDelete code ${expectedCode}: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
}

function readCascadeComplete(step: CaptureStep): boolean {
  const data = asRecord(step.response.payload.data);
  const orders = readRecord(data, 'orders');
  const ordersCount = readRecord(data, 'ordersCount');
  return data?.['order'] === null && readArray(orders, 'nodes').length === 0 && readNumber(ordersCount, 'count') === 0;
}

async function waitForDeletedRead(orderId: string, query: string): Promise<CaptureStep> {
  let last: CaptureStep | null = null;
  for (let attempt = 0; attempt < 8; attempt += 1) {
    last = await capture(
      downstreamReadDocument,
      { id: orderId, query },
      `downstream deleted order read attempt ${attempt}`,
    );
    if (readCascadeComplete(last)) {
      return last;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(`Timed out waiting for deleted order cascade: ${JSON.stringify(last?.response.payload, null, 2)}`);
}

async function cleanupOrder(orderId: string): Promise<JsonRecord> {
  const cancelVariables = {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  };
  const cancel = await runGraphqlRequest<JsonRecord>(trimGraphql(orderCancelDocument), cancelVariables);
  const deleteAfterCancel = await runGraphqlRequest<JsonRecord>(trimGraphql(orderDeleteDocument), { orderId });
  return {
    cancel: {
      query: trimGraphql(orderCancelDocument),
      variables: cancelVariables,
      response: cancel,
    },
    deleteAfterCancel: {
      query: trimGraphql(orderDeleteDocument),
      variables: { orderId },
      response: deleteAfterCancel,
    },
  };
}

function moneyBag(amount: string, currencyCode: string): JsonRecord {
  return {
    shopMoney: {
      amount,
      currencyCode,
    },
  };
}

function createOrderVariables(
  stamp: number,
  scenario: string,
  options: { amount: string; fulfillmentStatus?: string; paid?: boolean; requiresShipping?: boolean },
): JsonRecord {
  const priceSet = moneyBag(options.amount, 'USD');
  const order: JsonRecord = {
    email: `order-delete-${scenario}-${stamp}@example.com`,
    note: `orderDelete snapshot staging ${scenario} live capture ${stamp}`,
    tags: ['order-delete-snapshot-staging', 'conformance', scenario],
    test: true,
    currency: 'USD',
    lineItems: [
      {
        title: `Order delete ${scenario}`,
        quantity: 1,
        priceSet,
        requiresShipping: options.requiresShipping ?? false,
        taxable: false,
        sku: `order-delete-${scenario}-${stamp}`,
      },
    ],
  };
  if (options.fulfillmentStatus) {
    order['fulfillmentStatus'] = options.fulfillmentStatus;
  }
  if (options.paid) {
    order['transactions'] = [
      {
        kind: 'SALE',
        status: 'SUCCESS',
        gateway: 'manual',
        test: true,
        amountSet: priceSet,
      },
    ];
  }
  return {
    order,
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
}

function localFulfilledOrderVariables(): JsonRecord {
  const priceSet = moneyBag('12.00', 'USD');
  return {
    order: {
      email: 'order-delete-local-fulfilled@example.com',
      note: 'orderDelete snapshot staging local fulfilled setup',
      tags: ['order-delete-snapshot-staging', 'conformance', 'local-fulfilled'],
      test: true,
      currency: 'USD',
      financialStatus: 'PAID',
      fulfillmentStatus: 'FULFILLED',
      lineItems: [
        {
          title: 'Order delete local fulfilled setup',
          quantity: 1,
          priceSet,
          requiresShipping: false,
          taxable: false,
          sku: 'order-delete-local-fulfilled',
        },
      ],
      transactions: [
        {
          kind: 'SALE',
          status: 'SUCCESS',
          gateway: 'manual',
          test: true,
          amountSet: priceSet,
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

const stamp = Date.now();
const remainingOrderIds = new Set<string>();
const cleanup: Record<string, unknown> = {};

try {
  const deletableVariables = createOrderVariables(stamp, 'deletable', { amount: '10.00' });
  const deletableCreate = await capture(
    orderCreateDocument,
    {
      ...deletableVariables,
      order: {
        ...(deletableVariables['order'] as JsonRecord),
        financialStatus: 'PENDING',
      },
    },
    'deletable orderCreate',
  );
  const deletableOrderId = orderIdFromCreate(deletableCreate);
  remainingOrderIds.add(deletableOrderId);
  const deletableEmail = readString(orderFromCreate(deletableCreate), 'email');
  if (!deletableEmail) {
    throw new Error(`Deletable order did not include email: ${JSON.stringify(deletableCreate.response.payload)}`);
  }
  const deletableQuery = `email:${deletableEmail}`;

  const successDelete = await capture(orderDeleteDocument, { orderId: deletableOrderId }, 'successful orderDelete');
  assertOrderDeleteSuccess(successDelete, deletableOrderId);
  remainingOrderIds.delete(deletableOrderId);

  const downstreamAfterDelete = await waitForDeletedRead(deletableOrderId, deletableQuery);
  const repeatDelete = await capture(orderDeleteDocument, { orderId: deletableOrderId }, 'repeat orderDelete');
  assertOrderDeleteUserError(repeatDelete, 'NOT_FOUND');

  const fulfilledSetupCreate = await capture(
    orderCreateDocument,
    createOrderVariables(stamp, 'fulfilled-invalid', { amount: '12.00', paid: true, requiresShipping: true }),
    'fulfilled setup orderCreate',
  );
  const fulfilledOrderId = orderIdFromCreate(fulfilledSetupCreate);
  remainingOrderIds.add(fulfilledOrderId);
  const fulfillmentOrderId = fulfillmentOrderIdFromCreate(fulfilledSetupCreate);
  const fulfillmentCreate = await capture(
    fulfillmentCreateDocument,
    {
      fulfillment: {
        notifyCustomer: false,
        trackingInfo: {
          number: `ORDER-DELETE-FULFILLED-${stamp}`,
          url: `https://example.com/track/ORDER-DELETE-FULFILLED-${stamp}`,
          company: 'Hermes',
        },
        lineItemsByFulfillmentOrder: [{ fulfillmentOrderId }],
      },
      message: 'orderDelete snapshot staging fulfilled setup',
    },
    'fulfilled setup fulfillmentCreate',
  );
  if (userErrors(readRecord(fulfillmentCreate.response.payload.data, 'fulfillmentCreate') ?? {}).length > 0) {
    throw new Error(`fulfillmentCreate returned userErrors: ${JSON.stringify(fulfillmentCreate.response.payload)}`);
  }
  const fulfilledEmail = readString(orderFromCreate(fulfilledSetupCreate), 'email');
  if (!fulfilledEmail) {
    throw new Error(`Fulfilled order did not include email: ${JSON.stringify(fulfilledSetupCreate.response.payload)}`);
  }
  const fulfilledQuery = `email:${fulfilledEmail}`;

  const fulfilledDelete = await capture(
    orderDeleteDocument,
    { orderId: fulfilledOrderId },
    'fulfilled orderDelete success branch',
  );
  assertOrderDeleteSuccess(fulfilledDelete, fulfilledOrderId);
  remainingOrderIds.delete(fulfilledOrderId);
  const fulfilledReadAfterDelete = await waitForDeletedRead(fulfilledOrderId, fulfilledQuery);

  const unknownDelete = await capture(
    orderDeleteDocument,
    { orderId: 'gid://shopify/Order/999999999999999999' },
    'unknown orderDelete',
  );
  assertOrderDeleteUserError(unknownDelete, 'NOT_FOUND');

  for (const orderId of remainingOrderIds) {
    cleanup[orderId] = await cleanupOrder(orderId);
    remainingOrderIds.delete(orderId);
  }

  await writeText(createRequestPath, trimGraphql(orderCreateDocument));
  await writeText(deleteRequestPath, trimGraphql(orderDeleteDocument));
  await writeText(readRequestPath, trimGraphql(downstreamReadDocument));
  await writeJson(fixturePath, {
    scenarioId: 'orderDelete-snapshot-staging',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    operations: {
      deletableCreate,
      successDelete,
      downstreamAfterDelete,
      repeatDelete,
      fulfilledSetupCreate,
      fulfillmentCreate,
      fulfilledDelete,
      fulfilledReadAfterDelete,
      unknownDelete,
    },
    cleanup,
    upstreamCalls: [],
  });
  await writeJson(specPath, {
    scenarioId: 'orderDelete-snapshot-staging',
    operationNames: ['orderCreate', 'orderDelete', 'order', 'orders', 'ordersCount'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'downstream-read-parity', 'runtime-staging'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.operations.deletableCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'deletable-order-create-baseline',
          capturePath: '$.operations.deletableCreate.response.payload.data.orderCreate',
          proxyPath: '$.data.orderCreate',
          selectedPaths: [
            '$.order.email',
            '$.order.displayFinancialStatus',
            '$.order.displayFulfillmentStatus',
            '$.userErrors',
          ],
        },
        {
          name: 'successful-delete',
          capturePath: '$.operations.successDelete.response.payload.data.orderDelete',
          proxyPath: '$.data.orderDelete',
          expectedDifferences: [
            {
              path: '$.deletedId',
              matcher: 'shopify-gid:Order',
              reason: 'Live Shopify and the local draft proxy necessarily allocate different order ids.',
            },
          ],
          proxyRequest: {
            documentPath: deleteRequestPath,
            variables: {
              orderId: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
            },
            apiVersion,
          },
        },
        {
          name: 'downstream-read-after-delete',
          capturePath: '$.operations.downstreamAfterDelete.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: readRequestPath,
            variables: {
              id: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
              query: { fromCapturePath: '$.operations.downstreamAfterDelete.variables.query' },
            },
            apiVersion,
          },
        },
        {
          name: 'repeated-delete-not-found',
          capturePath: '$.operations.repeatDelete.response.payload.data.orderDelete',
          proxyPath: '$.data.orderDelete',
          proxyRequest: {
            documentPath: deleteRequestPath,
            variables: {
              orderId: { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
            },
            apiVersion,
          },
        },
        {
          name: 'fulfilled-order-create-baseline',
          capturePath: '$.operations.fulfilledSetupCreate.response.payload.data.orderCreate',
          proxyPath: '$.data.orderCreate',
          selectedPaths: [
            '$.order.email',
            '$.order.displayFinancialStatus',
            '$.order.displayFulfillmentStatus',
            '$.userErrors',
          ],
          proxyRequest: {
            documentPath: createRequestPath,
            variables: localFulfilledOrderVariables(),
            apiVersion,
          },
        },
        {
          name: 'fulfilled-order-delete-success',
          capturePath: '$.operations.fulfilledDelete.response.payload.data.orderDelete',
          proxyPath: '$.data.orderDelete',
          expectedDifferences: [
            {
              path: '$.deletedId',
              matcher: 'shopify-gid:Order',
              reason: 'Live Shopify and the local draft proxy necessarily allocate different order ids.',
            },
          ],
          proxyRequest: {
            documentPath: deleteRequestPath,
            variables: {
              orderId: {
                fromProxyResponse: 'fulfilled-order-create-baseline',
                path: '$.data.orderCreate.order.id',
              },
            },
            apiVersion,
          },
        },
        {
          name: 'fulfilled-order-read-after-delete',
          capturePath: '$.operations.fulfilledReadAfterDelete.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: readRequestPath,
            variables: {
              id: {
                fromProxyResponse: 'fulfilled-order-create-baseline',
                path: '$.data.orderCreate.order.id',
              },
              query: { fromCapturePath: '$.operations.fulfilledReadAfterDelete.variables.query' },
            },
            apiVersion,
          },
        },
        {
          name: 'unknown-order-not-found',
          capturePath: '$.operations.unknownDelete.response.payload.data.orderDelete',
          proxyPath: '$.data.orderDelete',
          proxyRequest: {
            documentPath: deleteRequestPath,
            variablesCapturePath: '$.operations.unknownDelete.variables',
            apiVersion,
          },
        },
      ],
    },
    notes:
      'Live 2026-04 capture for orderDelete success, read-after-delete cascade, repeat-delete NOT_FOUND, paid fulfilled-order delete success, and unknown-order NOT_FOUND. The ticket expected a paid/fulfilled INVALID branch, but live probes against disposable direct-order, draft-completed, fulfilled, open-fulfillment-order, requested-return, refunded, cancelled, test, and non-test orders all returned successful orderDelete payloads and downstream null reads. The proxy replay stages an equivalent fulfilled order directly through orderCreate input because fulfillmentCreate is not part of this orderDelete local replay path, then compares orderDelete payloads plus downstream order/order connection/count effects without runtime Shopify writes.',
  });

  console.log(
    JSON.stringify(
      {
        ok: true,
        fixturePath,
        specPath,
        requestPaths: [createRequestPath, deleteRequestPath, readRequestPath],
        deletableOrderId,
        fulfilledOrderId,
        fulfilledDeleteDeletedId: readString(orderDeletePayload(fulfilledDelete), 'deletedId'),
      },
      null,
      2,
    ),
  );
} finally {
  for (const orderId of remainingOrderIds) {
    try {
      cleanup[orderId] = await cleanupOrder(orderId);
    } catch (error) {
      cleanup[orderId] = { error: error instanceof Error ? error.message : String(error) };
    }
  }
  if (remainingOrderIds.size > 0) {
    await writeJson(path.join(fixtureDir, 'order-delete-snapshot-staging-cleanup-failure.json'), {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cleanup,
    });
  }
}
