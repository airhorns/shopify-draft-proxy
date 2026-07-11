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

const scenarioId = 'order-live-hybrid-mixed-catalog';
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
const specPath = path.join('config', 'parity-specs', 'orders', `${scenarioId}.json`);
const createRequestPath = path.join(requestDir, `${scenarioId}-create.graphql`);
const updateRequestPath = path.join(requestDir, `${scenarioId}-update.graphql`);
const deleteRequestPath = path.join(requestDir, `${scenarioId}-delete.graphql`);
const readRequestPath = path.join(requestDir, `${scenarioId}-read.graphql`);
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, `${scenarioId}.json`);

const orderCreateDocument = `#graphql
  mutation OrderLiveHybridMixedCatalogCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        email
        note
        tags
        processedAt
        displayFinancialStatus
        displayFulfillmentStatus
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderUpdateDocument = `#graphql
  mutation OrderLiveHybridMixedCatalogUpdate($input: OrderInput!) {
    orderUpdate(input: $input) {
      order {
        id
        email
        note
        tags
        processedAt
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderDeleteDocument = `#graphql
  mutation OrderLiveHybridMixedCatalogDelete($orderId: ID!) {
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

const mixedCatalogReadDocument = `#graphql
  query OrderLiveHybridMixedCatalogRead($existingId: ID!, $query: String!, $first: Int!) {
    existing: order(id: $existingId) {
      email
      note
      tags
      processedAt
    }
    visible: orders(first: $first, query: $query, sortKey: PROCESSED_AT, reverse: false) {
      nodes {
        email
        note
        tags
        processedAt
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    total: ordersCount(query: $query, limit: null) {
      count
      precision
    }
  }
`;

const orderHydrateDocument = `
    query OrdersOrderHydrate($id: ID!) {
      order(id: $id) {
        id
        name
        email
        note
        tags
        customAttributes { key value }
        customer { id email displayName }
        billingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        shippingAddress { firstName lastName address1 city provinceCode countryCodeV2 zip }
        currencyCode
        presentmentCurrencyCode
        displayFinancialStatus
        displayFulfillmentStatus
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } }
        totalTaxSet { shopMoney { amount currencyCode } }
        totalDiscountsSet { shopMoney { amount currencyCode } }
        discountCodes
        lineItems(first: 10) {
          nodes {
            id
            title
            name
            quantity
            currentQuantity
            sku
            variantTitle
            requiresShipping
            taxable
            customAttributes { key value }
            originalUnitPriceSet { shopMoney { amount currencyCode } }
            originalTotalSet { shopMoney { amount currencyCode } }
            variant { id title sku }
            taxLines { title rate priceSet { shopMoney { amount currencyCode } } }
          }
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

function readPath(value: unknown, pathSegments: (string | number)[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (current === null || current === undefined) return undefined;
    current = (current as Record<string | number, unknown>)[segment];
  }
  return current;
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

async function captureRaw(query: string, variables: JsonRecord, context: string): Promise<CaptureStep> {
  const response = await runGraphqlRequest<JsonRecord>(query, variables);
  assertNoTopLevelErrors(response, context);
  return { query, variables, response };
}

async function sleep(milliseconds: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
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

function assertOrderDeleteSuccess(step: CaptureStep, orderId: string): void {
  const payload = readRecord(step.response.payload.data, 'orderDelete');
  if (payload?.['deletedId'] !== orderId || readArray(payload, 'userErrors').length > 0) {
    throw new Error(`orderDelete did not delete ${orderId}: ${JSON.stringify(step.response.payload, null, 2)}`);
  }
}

function orderVariables(stamp: string, role: 'existing' | 'candidate', processedAt: string, tag: string): JsonRecord {
  return {
    order: {
      email: `har-2272-${role}-${stamp}@example.com`,
      note: `HAR-2272 ${role} mixed catalog ${stamp}`,
      tags: ['har-2272-live-hybrid-mixed-catalog', tag, role],
      test: true,
      currency: 'USD',
      processedAt,
      lineItems: [
        {
          title: `HAR-2272 ${role} mixed catalog`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: role === 'existing' ? '11.00' : '12.00',
              currencyCode: 'USD',
            },
          },
          requiresShipping: false,
          taxable: false,
          sku: `har-2272-${role}-${stamp}`,
        },
      ],
      financialStatus: 'PENDING',
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
}

function specPayload(): JsonRecord {
  return {
    scenarioId,
    operationNames: ['orderCreate', 'orderUpdate', 'orderDelete', 'order', 'orders', 'ordersCount'],
    scenarioStatus: 'captured',
    assertionKinds: [
      'downstream-read-parity',
      'search-filtering',
      'sort-order',
      'count-limit',
      'pagination-shape',
      'runtime-staging',
    ],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: createRequestPath,
      variablesCapturePath: '$.operations.candidateCreate.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live 2025-01 Shopify capture for a mixed order catalog: a real pre-existing tagged order, a second disposable order created with the same tag, an update to the existing order, and a delete tombstone. Proxy replay stages only the candidate order locally and uses recorded upstream baseline/hydration calls for the untouched existing order, proving LiveHybrid orders/order/orderCount merge upstream plus staged state without runtime Shopify writes.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'candidate-create-baseline',
          capturePath: '$.operations.candidateCreate.response.payload.data.orderCreate',
          proxyPath: '$.data.orderCreate',
          selectedPaths: [
            '$.order.email',
            '$.order.note',
            '$.order.tags',
            '$.order.processedAt',
            '$.order.displayFinancialStatus',
            '$.order.displayFulfillmentStatus',
            '$.userErrors',
          ],
        },
        {
          name: 'mixed-read-after-staged-create',
          capturePath: '$.operations.mixedAfterCreateRead.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: readRequestPath,
            variablesCapturePath: '$.operations.mixedAfterCreateRead.variables',
            apiVersion,
          },
        },
        {
          name: 'existing-order-update-wins-by-id',
          capturePath: '$.operations.existingUpdate.response.payload.data.orderUpdate',
          proxyPath: '$.data.orderUpdate',
          proxyRequest: {
            documentPath: updateRequestPath,
            variablesCapturePath: '$.operations.existingUpdate.variables',
            apiVersion,
          },
        },
        {
          name: 'mixed-read-after-existing-update',
          capturePath: '$.operations.afterUpdateRead.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: readRequestPath,
            variablesCapturePath: '$.operations.afterUpdateRead.variables',
            apiVersion,
          },
        },
        {
          name: 'existing-order-delete-tombstone',
          capturePath: '$.operations.existingDelete.response.payload.data.orderDelete',
          proxyPath: '$.data.orderDelete',
          proxyRequest: {
            documentPath: deleteRequestPath,
            variablesCapturePath: '$.operations.existingDelete.variables',
            apiVersion,
          },
        },
        {
          name: 'mixed-read-after-existing-delete',
          capturePath: '$.operations.afterDeleteRead.response.payload.data',
          proxyPath: '$.data',
          proxyRequest: {
            documentPath: readRequestPath,
            variablesCapturePath: '$.operations.afterDeleteRead.variables',
            apiVersion,
          },
        },
      ],
    },
  };
}

async function captureReadWithRetry(
  variables: JsonRecord,
  expectedCount: number,
  expectedExistingNull: boolean,
  context: string,
): Promise<CaptureStep> {
  let latest: CaptureStep | null = null;
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    latest = await capture(mixedCatalogReadDocument, variables, `${context} attempt ${attempt}`);
    const count = readPath(latest.response.payload, ['data', 'total', 'count']);
    const nodes = readPath(latest.response.payload, ['data', 'visible', 'nodes']);
    const existing = readPath(latest.response.payload, ['data', 'existing']);
    if (
      count === expectedCount &&
      Array.isArray(nodes) &&
      nodes.length === expectedCount &&
      (expectedExistingNull ? existing === null : asRecord(existing) !== null)
    ) {
      if (attempt > 1) console.log(`${context} indexed after ${attempt} attempts`);
      return latest;
    }
    if (attempt < 12) await sleep(2_000);
  }
  throw new Error(
    `${context} did not reach expected count ${expectedCount}: ${JSON.stringify(latest?.response.payload, null, 2)}`,
  );
}

await writeText(createRequestPath, trimGraphql(orderCreateDocument));
await writeText(updateRequestPath, trimGraphql(orderUpdateDocument));
await writeText(deleteRequestPath, trimGraphql(orderDeleteDocument));
await writeText(readRequestPath, trimGraphql(mixedCatalogReadDocument));
await writeJson(specPath, specPayload());

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const tag = `har-2272-mixed-${stamp}`;
const query = `tag:${tag}`;
const existingProcessedAt = '2024-01-01T00:00:00Z';
const candidateProcessedAt = '2024-01-02T00:00:00Z';
let existingOrderId: string | null = null;
let candidateOrderId: string | null = null;
let existingDeleted = false;
let candidateDeleted = false;
const cleanup: CaptureStep[] = [];

try {
  const existingCreate = await capture(
    orderCreateDocument,
    orderVariables(stamp, 'existing', existingProcessedAt, tag),
    'existing orderCreate',
  );
  existingOrderId = orderIdFromCreate(existingCreate);

  const readVariables = { existingId: existingOrderId, query, first: 5 };
  const baselineRead = await captureReadWithRetry(readVariables, 1, false, 'baseline mixed catalog read');
  const baselineHydrate = await captureRaw(
    orderHydrateDocument,
    { id: existingOrderId },
    'baseline existing order hydrate',
  );

  const candidateCreate = await capture(
    orderCreateDocument,
    orderVariables(stamp, 'candidate', candidateProcessedAt, tag),
    'candidate orderCreate',
  );
  candidateOrderId = orderIdFromCreate(candidateCreate);

  const mixedAfterCreateRead = await captureReadWithRetry(readVariables, 2, false, 'mixed read after candidate create');

  const existingUpdate = await capture(
    orderUpdateDocument,
    {
      input: {
        id: existingOrderId,
        note: `HAR-2272 edited existing mixed catalog ${stamp}`,
        tags: ['har-2272-live-hybrid-mixed-catalog', tag, 'existing', 'edited'],
      },
    },
    'existing orderUpdate',
  );

  const afterUpdateRead = await captureReadWithRetry(readVariables, 2, false, 'mixed read after existing update');

  const existingDelete = await capture(orderDeleteDocument, { orderId: existingOrderId }, 'existing orderDelete');
  assertOrderDeleteSuccess(existingDelete, existingOrderId);
  existingDeleted = true;

  const afterDeleteRead = await captureReadWithRetry(readVariables, 1, true, 'mixed read after existing delete');

  const candidateCleanup = await capture(
    orderDeleteDocument,
    { orderId: candidateOrderId },
    'candidate cleanup delete',
  );
  assertOrderDeleteSuccess(candidateCleanup, candidateOrderId);
  candidateDeleted = true;
  cleanup.push(candidateCleanup);

  const upstreamReadCall = {
    operationName: 'OrderLiveHybridMixedCatalogRead',
    variables: readVariables,
    query: trimGraphql(mixedCatalogReadDocument),
    response: { status: baselineRead.response.status, body: baselineRead.response.payload },
  };

  await writeJson(fixturePath, {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    notes:
      'Captured from live Shopify Admin GraphQL. The live expected operations create/update/delete disposable orders. The upstreamCalls cassette intentionally records the pre-local-write existing-order baseline that LiveHybrid proxy replay hydrates while supported mutations remain staged locally.',
    operations: {
      existingCreate,
      baselineRead,
      baselineHydrate,
      candidateCreate,
      mixedAfterCreateRead,
      existingUpdate,
      afterUpdateRead,
      existingDelete,
      afterDeleteRead,
    },
    upstreamCalls: [
      upstreamReadCall,
      {
        operationName: 'OrdersOrderHydrate',
        variables: { id: existingOrderId },
        query: orderHydrateDocument,
        response: { status: baselineHydrate.response.status, body: baselineHydrate.response.payload },
      },
      upstreamReadCall,
      upstreamReadCall,
    ],
    cleanup,
  });

  console.log(`Wrote ${fixturePath}`);
  console.log(`Wrote ${specPath}`);
  console.log(`Wrote ${createRequestPath}`);
  console.log(`Wrote ${updateRequestPath}`);
  console.log(`Wrote ${deleteRequestPath}`);
  console.log(`Wrote ${readRequestPath}`);
} catch (error) {
  for (const [orderId, alreadyDeleted, label] of [
    [existingOrderId, existingDeleted, 'existing'],
    [candidateOrderId, candidateDeleted, 'candidate'],
  ] as const) {
    if (!orderId || alreadyDeleted) continue;
    try {
      const cleanupDelete = await capture(orderDeleteDocument, { orderId }, `${label} cleanup after failure`);
      cleanup.push(cleanupDelete);
    } catch (cleanupError) {
      console.error(`Cleanup failed for ${label} ${orderId}:`, cleanupError);
    }
  }
  throw error;
}
