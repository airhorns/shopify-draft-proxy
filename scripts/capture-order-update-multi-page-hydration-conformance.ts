/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type Capture = {
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
const requestDir = path.join('config', 'parity-requests', 'orders');
const fixturePath = path.join(fixtureDir, 'orderUpdate-multi-page-hydration.json');

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function trimGraphql(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord): Promise<Capture> {
  return {
    query,
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function asRecord(value: unknown): JsonRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return {};
  }
  return value as JsonRecord;
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function rootData(captureResult: Capture, root: string): JsonRecord {
  return asRecord(asRecord(captureResult.response.payload['data'])[root]);
}

function connectionNodes(connection: unknown): JsonRecord[] {
  return asArray(asRecord(connection)['nodes']).map(asRecord);
}

function assertOk(label: string, captureResult: Capture): void {
  const payload = captureResult.response.payload;
  if (captureResult.response.status < 200 || captureResult.response.status >= 300 || payload['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(payload, null, 2)}`);
  }
}

function assertNoUserErrors(label: string, captureResult: Capture, root: string): void {
  const userErrors = rootData(captureResult, root)['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}

function upstreamCall(operationName: string, captureResult: Capture): JsonRecord {
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

function orderLineItems(stamp: number): JsonRecord[] {
  return Array.from({ length: 12 }, (_, index) => {
    const lineNumber = index + 1;
    return {
      title: `SDP multi-page hydration line ${lineNumber}`,
      quantity: 1,
      priceSet: {
        shopMoney: {
          amount: '1.00',
          currencyCode: 'USD',
        },
      },
      requiresShipping: false,
      taxable: false,
      sku: `sdp-multi-page-${stamp}-${String(lineNumber).padStart(2, '0')}`,
    };
  });
}

const orderCreateMutation = `#graphql
  mutation OrderUpdateMultiPageHydrationCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        lineItems(first: 20) {
          nodes {
            id
            title
            quantity
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
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

const orderCancelCleanupMutation = `#graphql
  mutation OrderUpdateMultiPageHydrationCleanup(
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
      orderCancelUserErrors {
        field
        message
        code
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const hydrateQuery = await readRequest('order-hydrate-pageable.graphql');
const updateMutation = await readRequest('orderUpdate-multi-page-hydration.graphql');
const downstreamReadQuery = await readRequest('orderUpdate-multi-page-hydration-read.graphql');
const stamp = Date.now();
let createdOrderId: string | null = null;
let fixturePayload: JsonRecord | null = null;

async function cleanupOrder(orderId: string): Promise<JsonRecord> {
  const variables = {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  };
  const cleanup = await capture(orderCancelCleanupMutation, variables);
  return {
    query: trimGraphql(cleanup.query),
    variables,
    response: cleanup.response.payload,
  };
}

try {
  const createVariables = {
    order: {
      email: `sdp-order-update-multi-page-${stamp}@example.com`,
      note: 'shopify-draft-proxy multi-page hydration capture setup',
      tags: ['shopify-draft-proxy', 'order-update', 'multi-page-hydration'],
      test: true,
      currency: 'USD',
      lineItems: orderLineItems(stamp),
    },
    options: {
      inventoryBehaviour: 'BYPASS',
      sendReceipt: false,
      sendFulfillmentReceipt: false,
    },
  };
  const create = await capture(orderCreateMutation, createVariables);
  assertOk('orderCreate setup', create);
  assertNoUserErrors('orderCreate setup', create, 'orderCreate');
  const createdOrder = asRecord(rootData(create, 'orderCreate')['order']);
  createdOrderId = String(createdOrder['id'] ?? '');
  if (!createdOrderId) {
    throw new Error(`orderCreate setup did not return an order id: ${JSON.stringify(create.response.payload)}`);
  }
  const createdLineCount = connectionNodes(asRecord(createdOrder)['lineItems']).length;
  if (createdLineCount !== 12) {
    throw new Error(`orderCreate setup returned ${createdLineCount} line items, expected 12`);
  }

  const firstHydrateVariables = { id: createdOrderId, lineItemsAfter: null };
  const firstHydrate = await capture(hydrateQuery, firstHydrateVariables);
  assertOk('first OrdersOrderHydrate page', firstHydrate);
  const firstOrder = asRecord(asRecord(firstHydrate.response.payload['data'])['order']);
  const firstLineItems = asRecord(firstOrder['lineItems']);
  const firstNodes = connectionNodes(firstLineItems);
  const firstPageInfo = asRecord(firstLineItems['pageInfo']);
  const firstEndCursor = String(firstPageInfo['endCursor'] ?? '');
  if (firstNodes.length !== 10 || firstPageInfo['hasNextPage'] !== true || !firstEndCursor) {
    throw new Error(
      `first hydrate page did not expose a 10-line page with a continuation cursor: ${JSON.stringify(
        firstLineItems,
        null,
        2,
      )}`,
    );
  }

  const secondHydrateVariables = { id: createdOrderId, lineItemsAfter: firstEndCursor };
  const secondHydrate = await capture(hydrateQuery, secondHydrateVariables);
  assertOk('second OrdersOrderHydrate page', secondHydrate);
  const secondOrder = asRecord(asRecord(secondHydrate.response.payload['data'])['order']);
  const secondLineItems = asRecord(secondOrder['lineItems']);
  const secondNodes = connectionNodes(secondLineItems);
  const secondPageInfo = asRecord(secondLineItems['pageInfo']);
  if (secondNodes.length !== 2 || secondPageInfo['hasNextPage'] !== false) {
    throw new Error(`second hydrate page did not expose the 2-line tail: ${JSON.stringify(secondLineItems, null, 2)}`);
  }

  const updateVariables = {
    input: {
      id: createdOrderId,
      note: 'order update multi-page hydration parity',
    },
  };
  const mutation = await capture(updateMutation, updateVariables);
  assertOk('orderUpdate multi-page hydration', mutation);
  assertNoUserErrors('orderUpdate multi-page hydration', mutation, 'orderUpdate');
  const mutationOrder = asRecord(rootData(mutation, 'orderUpdate')['order']);
  const mutationLineCount = connectionNodes(mutationOrder['lineItems']).length;
  if (mutationLineCount !== 12) {
    throw new Error(`orderUpdate returned ${mutationLineCount} line items, expected 12`);
  }

  const downstreamRead = await capture(downstreamReadQuery, { id: createdOrderId });
  assertOk('downstream order read', downstreamRead);
  const downstreamOrder = asRecord(asRecord(downstreamRead.response.payload['data'])['order']);
  const downstreamLineCount = connectionNodes(downstreamOrder['lineItems']).length;
  if (downstreamLineCount !== 12) {
    throw new Error(`downstream order read returned ${downstreamLineCount} line items, expected 12`);
  }

  fixturePayload = {
    variables: updateVariables,
    setup: {
      orderCreate: {
        query: trimGraphql(create.query),
        variables: createVariables,
        response: create.response.payload,
      },
    },
    mutation: {
      response: mutation.response.payload,
    },
    downstreamRead: {
      variables: downstreamRead.variables,
      response: downstreamRead.response.payload,
    },
    upstreamCalls: [
      upstreamCall('OrdersOrderHydrate', firstHydrate),
      upstreamCall('OrdersOrderHydrate', secondHydrate),
    ],
  };
} finally {
  if (createdOrderId && fixturePayload) {
    try {
      fixturePayload['cleanup'] = await cleanupOrder(createdOrderId);
    } catch (error) {
      fixturePayload['cleanup'] = { error: error instanceof Error ? error.message : String(error) };
    }
    await writeJson(fixturePath, fixturePayload);
  }
}

console.log(
  JSON.stringify(
    {
      ok: true,
      fixturePath,
      orderId: createdOrderId,
    },
    null,
    2,
  ),
);
