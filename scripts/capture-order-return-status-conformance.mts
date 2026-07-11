/* oxlint-disable no-console -- CLI capture scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { spawnSync } from 'node:child_process';
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
type ReturnSeed = {
  label: string;
  tag: string;
  orderName: string;
  orderQuery: string;
  orderId: string;
  fulfillmentLineItemId: string;
  orderCreate: GraphqlCapture;
  fulfillmentCreate: GraphqlCapture;
  orderReadAfterFulfillment: GraphqlCapture;
  returnOrderHydrate: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
if (apiVersion !== '2026-04') {
  throw new Error(`order returnStatus capture must run against Admin API 2026-04, got ${apiVersion}`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'order-return-status-lifecycle.json');
const requestDir = path.join('config', 'parity-requests', 'orders');
const specPath = path.join('config', 'parity-specs', 'orders', 'order-return-status-lifecycle.json');

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

function formatGeneratedJson(paths: string[]): void {
  const result = spawnSync('corepack', ['pnpm', 'exec', 'oxfmt', ...paths], { stdio: 'inherit' });
  if (result.status !== 0) {
    throw new Error(`Failed to format generated JSON files: ${paths.join(', ')}`);
  }
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

function payloadRoot(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  const payload = readRecord(captureResult.response.payload) ?? {};
  const data = readRecord(payload['data']) ?? {};
  return readRecord(data[rootName]) ?? {};
}

function returnFromPayload(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  return readRecord(payloadRoot(captureResult, rootName)['return']) ?? {};
}

function orderFromCreate(captureResult: GraphqlCapture): JsonRecord {
  return readRecord(payloadRoot(captureResult, 'orderCreate')['order']) ?? {};
}

function requireEmptyUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  const payload = readRecord(captureResult.response.payload) ?? {};
  const errors = payload['errors'];
  const root = payloadRoot(captureResult, rootName);
  const userErrors = readArray(root['userErrors']);
  if (errors || userErrors.length > 0) {
    throw new Error(`Unexpected ${rootName} errors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function requireMutationOrderStatus(captureResult: GraphqlCapture, rootName: string, expected: string): void {
  const actual = readRecord(returnFromPayload(captureResult, rootName)['order'])?.['returnStatus'];
  if (actual !== expected) {
    throw new Error(`Expected ${rootName}.return.order.returnStatus ${expected}, got ${JSON.stringify(actual)}`);
  }
}

function requireOrderCreateStatus(captureResult: GraphqlCapture, expected: string): void {
  const actual = orderFromCreate(captureResult)['returnStatus'];
  if (actual !== expected) {
    throw new Error(`Expected orderCreate.order.returnStatus ${expected}, got ${JSON.stringify(actual)}`);
  }
}

function requireReadStatuses(captureResult: GraphqlCapture, expected: string): void {
  const payload = readRecord(captureResult.response.payload) ?? {};
  const data = readRecord(payload['data']) ?? {};
  const detail = readRecord(data['detail']) ?? {};
  const listNode = readNodes(data['list'])[0] ?? {};
  const node = readRecord(data['node']) ?? {};
  const actual = [detail['returnStatus'], listNode['returnStatus'], node['returnStatus']];
  if (actual.some((status) => status !== expected)) {
    throw new Error(`Expected read returnStatus ${expected}, got ${JSON.stringify(actual)}`);
  }
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function captureOrderStatusRead(orderId: string, query: string, expected: string): Promise<GraphqlCapture> {
  let lastRead: GraphqlCapture | null = null;
  for (let attempt = 0; attempt < 10; attempt += 1) {
    lastRead = await capture(orderReadQuery, { orderId, query });
    const payload = readRecord(lastRead.response.payload) ?? {};
    const data = readRecord(payload['data']) ?? {};
    const listNode = readNodes(data['list'])[0] ?? null;
    if (listNode !== null) {
      requireReadStatuses(lastRead, expected);
      return lastRead;
    }
    await sleep(1_000);
  }
  if (lastRead !== null) requireReadStatuses(lastRead, expected);
  throw new Error(`Failed to capture order returnStatus read for ${orderId}`);
}

function firstFulfillmentOrder(order: JsonRecord): JsonRecord {
  return readNodes(order['fulfillmentOrders'])[0] ?? {};
}

function firstFulfillmentLineItem(order: JsonRecord): JsonRecord {
  return readNodes(readRecord(readArray(order['fulfillments'])[0])?.['fulfillmentLineItems'])[0] ?? {};
}

function firstReturnLineItemId(captureResult: GraphqlCapture, rootName: string): string {
  const lineItem = readNodes(returnFromPayload(captureResult, rootName)['returnLineItems'])[0] ?? {};
  return requireString(lineItem['id'], `${rootName} return line item id`);
}

function orderVariables(label: string, tag: string, quantity: number): JsonRecord {
  return {
    order: {
      email: `order-return-status-${label}-${stamp}@example.com`,
      note: `order return status ${label} ${stamp}`,
      tags: ['ors', tag],
      test: true,
      currency: 'USD',
      lineItems: [
        {
          title: `Order return status ${label} item ${stamp}`,
          quantity,
          priceSet: {
            shopMoney: {
              amount: '18.00',
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
}

function returnRequestVariables(seed: ReturnSeed): JsonRecord {
  return {
    input: {
      orderId: seed.orderId,
      returnLineItems: [
        {
          fulfillmentLineItemId: seed.fulfillmentLineItemId,
          quantity: 1,
          returnReason: 'OTHER',
        },
      ],
    },
  };
}

function orderNameQuery(orderName: string): string {
  return `name:${orderName}`;
}

const setupOrderFields = `#graphql
  fragment OrderReturnStatusSetupOrderFields on Order {
    id
    name
    tags
    fulfillmentOrders(first: 5) {
      nodes {
        id
        status
        requestStatus
        lineItems(first: 5) {
          nodes {
            id
            totalQuantity
            remainingQuantity
            lineItem {
              id
              title
            }
          }
        }
      }
    }
    fulfillments(first: 5) {
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
          }
        }
      }
    }
  }
`;

const setupOrderCreateMutation = `#graphql
  ${setupOrderFields}
  mutation OrderReturnStatusSetupOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...OrderReturnStatusSetupOrderFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation OrderReturnStatusFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
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

const setupOrderReadQuery = `#graphql
  ${setupOrderFields}
  query OrderReturnStatusSetupOrderRead($id: ID!) {
    order(id: $id) {
      ...OrderReturnStatusSetupOrderFields
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation OrderReturnStatusOrderCancel(
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

const orderCreateMutation = await readRequest('order-return-status-order-create.graphql');
const orderReadQuery = await readRequest('order-return-status-read.graphql');
const returnRequestMutation = await readRequest('order-return-status-return-request.graphql');
const returnApproveMutation = await readRequest('order-return-status-return-approve.graphql');
const returnDeclineMutation = await readRequest('order-return-status-return-decline.graphql');
const returnCloseMutation = await readRequest('order-return-status-return-close.graphql');
const returnReopenMutation = await readRequest('order-return-status-return-reopen.graphql');
const returnCancelMutation = await readRequest('order-return-status-return-cancel.graphql');
const returnProcessMutation = await readRequest('order-return-status-return-process.graphql');
const returnCreateMutation = await readRequest('order-return-status-return-create.graphql');
const removeFromReturnMutation = await readRequest('order-return-status-remove-from-return.graphql');
const returnOrderHydrateQuery = await readRequest('return-order-hydrate.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

async function createFulfilledSeed(label: string, quantity: number): Promise<ReturnSeed> {
  const tag = `ors-${label}-${stamp}`;
  const orderCreate = await capture(setupOrderCreateMutation, orderVariables(label, tag, quantity));
  requireEmptyUserErrors(orderCreate, 'orderCreate');
  const createdOrder = orderFromCreate(orderCreate);
  const orderId = requireString(createdOrder['id'], `${label} order id`);
  const orderName = requireString(createdOrder['name'], `${label} order name`);
  const fulfillmentOrder = firstFulfillmentOrder(createdOrder);
  const fulfillmentOrderId = requireString(fulfillmentOrder['id'], `${label} fulfillment order id`);
  const fulfillmentOrderLineItem = readNodes(fulfillmentOrder['lineItems'])[0] ?? {};
  const fulfillmentOrderLineItemId = requireString(
    fulfillmentOrderLineItem['id'],
    `${label} fulfillment order line item id`,
  );

  const fulfillmentCreate = await capture(fulfillmentCreateMutation, {
    fulfillment: {
      notifyCustomer: false,
      trackingInfo: {
        number: `ORDER-RETURN-STATUS-${label.toUpperCase()}-${stamp}`,
        url: `https://example.com/track/ORDER-RETURN-STATUS-${label}-${stamp}`,
        company: 'Hermes Carrier',
      },
      lineItemsByFulfillmentOrder: [
        {
          fulfillmentOrderId,
          fulfillmentOrderLineItems: [
            {
              id: fulfillmentOrderLineItemId,
              quantity,
            },
          ],
        },
      ],
    },
    message: `order return status ${label} fulfillment ${stamp}`,
  });
  requireEmptyUserErrors(fulfillmentCreate, 'fulfillmentCreate');

  const orderReadAfterFulfillment = await capture(setupOrderReadQuery, { id: orderId });
  const orderAfterFulfillment = readRecord(readRecord(orderReadAfterFulfillment.response.payload)?.['data'])?.['order'];
  const fulfillmentLineItem = firstFulfillmentLineItem(readRecord(orderAfterFulfillment) ?? {});
  const fulfillmentLineItemId = requireString(fulfillmentLineItem['id'], `${label} fulfillment line item id`);

  const returnOrderHydrate = await runGraphqlRequest(returnOrderHydrateQuery, { id: orderId });
  if (returnOrderHydrate.payload['errors']) {
    throw new Error(`${label} return-order hydrate returned errors: ${JSON.stringify(returnOrderHydrate.payload)}`);
  }

  return {
    label,
    tag,
    orderName,
    orderQuery: orderNameQuery(orderName),
    orderId,
    fulfillmentLineItemId,
    orderCreate,
    fulfillmentCreate,
    orderReadAfterFulfillment,
    returnOrderHydrate,
  };
}

const declineInput = {
  declineReason: 'OTHER',
  declineNote: `order return status decline ${stamp}`,
  notifyCustomer: false,
};

const zeroTag = `ors-zero-${stamp}`;
const zeroOrderCreate = await capture(orderCreateMutation, orderVariables('zero', zeroTag, 1));
requireEmptyUserErrors(zeroOrderCreate, 'orderCreate');
requireOrderCreateStatus(zeroOrderCreate, 'NO_RETURN');
const zeroOrderId = requireString(orderFromCreate(zeroOrderCreate)['id'], 'zero order id');
const zeroOrderName = requireString(orderFromCreate(zeroOrderCreate)['name'], 'zero order name');
const zeroRead = await captureOrderStatusRead(zeroOrderId, orderNameQuery(zeroOrderName), 'NO_RETURN');

const mixedSeed = await createFulfilledSeed('mixed', 6);
const mixedFirstRequest = await capture(returnRequestMutation, returnRequestVariables(mixedSeed));
requireEmptyUserErrors(mixedFirstRequest, 'returnRequest');
requireMutationOrderStatus(mixedFirstRequest, 'returnRequest', 'RETURN_REQUESTED');
const mixedFirstReturnId = requireString(
  returnFromPayload(mixedFirstRequest, 'returnRequest')['id'],
  'mixed first return id',
);
const mixedReadAfterFirstRequest = await captureOrderStatusRead(
  mixedSeed.orderId,
  mixedSeed.orderQuery,
  'RETURN_REQUESTED',
);

const mixedApproveFirst = await capture(returnApproveMutation, { input: { id: mixedFirstReturnId } });
requireEmptyUserErrors(mixedApproveFirst, 'returnApproveRequest');
requireMutationOrderStatus(mixedApproveFirst, 'returnApproveRequest', 'IN_PROGRESS');
const mixedFirstReturnLineItemId = firstReturnLineItemId(mixedApproveFirst, 'returnApproveRequest');
const mixedReadAfterApprove = await captureOrderStatusRead(mixedSeed.orderId, mixedSeed.orderQuery, 'IN_PROGRESS');

const mixedSecondRequest = await capture(returnRequestMutation, returnRequestVariables(mixedSeed));
requireEmptyUserErrors(mixedSecondRequest, 'returnRequest');
requireMutationOrderStatus(mixedSecondRequest, 'returnRequest', 'RETURN_REQUESTED');
const mixedSecondReturnId = requireString(
  returnFromPayload(mixedSecondRequest, 'returnRequest')['id'],
  'mixed second return id',
);
const mixedReadAfterSecondRequest = await captureOrderStatusRead(
  mixedSeed.orderId,
  mixedSeed.orderQuery,
  'RETURN_REQUESTED',
);

const mixedDeclineSecond = await capture(returnDeclineMutation, {
  input: {
    id: mixedSecondReturnId,
    ...declineInput,
  },
});
requireEmptyUserErrors(mixedDeclineSecond, 'returnDeclineRequest');
requireMutationOrderStatus(mixedDeclineSecond, 'returnDeclineRequest', 'IN_PROGRESS');
const mixedReadAfterDecline = await captureOrderStatusRead(mixedSeed.orderId, mixedSeed.orderQuery, 'IN_PROGRESS');

const mixedCloseFirst = await capture(returnCloseMutation, { id: mixedFirstReturnId });
requireEmptyUserErrors(mixedCloseFirst, 'returnClose');
requireMutationOrderStatus(mixedCloseFirst, 'returnClose', 'RETURNED');
const mixedReadAfterClose = await captureOrderStatusRead(mixedSeed.orderId, mixedSeed.orderQuery, 'RETURNED');

const mixedReopenFirst = await capture(returnReopenMutation, { id: mixedFirstReturnId });
requireEmptyUserErrors(mixedReopenFirst, 'returnReopen');
requireMutationOrderStatus(mixedReopenFirst, 'returnReopen', 'IN_PROGRESS');
const mixedReadAfterReopen = await captureOrderStatusRead(mixedSeed.orderId, mixedSeed.orderQuery, 'IN_PROGRESS');

const mixedProcessFirst = await capture(returnProcessMutation, {
  input: {
    returnId: mixedFirstReturnId,
    returnLineItems: [
      {
        id: mixedFirstReturnLineItemId,
        quantity: 1,
      },
    ],
    notifyCustomer: false,
  },
});
requireEmptyUserErrors(mixedProcessFirst, 'returnProcess');
requireMutationOrderStatus(mixedProcessFirst, 'returnProcess', 'IN_PROGRESS');
const mixedReadAfterProcess = await captureOrderStatusRead(mixedSeed.orderId, mixedSeed.orderQuery, 'IN_PROGRESS');

const declinedSeed = await createFulfilledSeed('declined-only', 1);
const declinedRequest = await capture(returnRequestMutation, returnRequestVariables(declinedSeed));
requireEmptyUserErrors(declinedRequest, 'returnRequest');
const declinedReturnId = requireString(returnFromPayload(declinedRequest, 'returnRequest')['id'], 'declined return id');
const declinedOnly = await capture(returnDeclineMutation, {
  input: {
    id: declinedReturnId,
    ...declineInput,
  },
});
requireEmptyUserErrors(declinedOnly, 'returnDeclineRequest');
requireMutationOrderStatus(declinedOnly, 'returnDeclineRequest', 'NO_RETURN');
const declinedRead = await captureOrderStatusRead(declinedSeed.orderId, declinedSeed.orderQuery, 'NO_RETURN');

const canceledSeed = await createFulfilledSeed('canceled-only', 1);
const canceledRequest = await capture(returnRequestMutation, returnRequestVariables(canceledSeed));
requireEmptyUserErrors(canceledRequest, 'returnRequest');
const canceledReturnId = requireString(returnFromPayload(canceledRequest, 'returnRequest')['id'], 'canceled return id');
const canceledApprove = await capture(returnApproveMutation, { input: { id: canceledReturnId } });
requireEmptyUserErrors(canceledApprove, 'returnApproveRequest');
const canceledOnly = await capture(returnCancelMutation, { id: canceledReturnId });
requireEmptyUserErrors(canceledOnly, 'returnCancel');
requireMutationOrderStatus(canceledOnly, 'returnCancel', 'NO_RETURN');
const canceledRead = await captureOrderStatusRead(canceledSeed.orderId, canceledSeed.orderQuery, 'NO_RETURN');

const removedSeed = await createFulfilledSeed('removed-lines', 1);
const removedCreate = await capture(returnCreateMutation, {
  returnInput: {
    orderId: removedSeed.orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId: removedSeed.fulfillmentLineItemId,
        quantity: 1,
        returnReason: 'OTHER',
        returnReasonNote: 'removed all lines',
      },
    ],
  },
});
requireEmptyUserErrors(removedCreate, 'returnCreate');
requireMutationOrderStatus(removedCreate, 'returnCreate', 'IN_PROGRESS');
const removedReturnId = requireString(returnFromPayload(removedCreate, 'returnCreate')['id'], 'removed return id');
const removedReturnLineItemId = firstReturnLineItemId(removedCreate, 'returnCreate');
const removedAllLines = await capture(removeFromReturnMutation, {
  returnId: removedReturnId,
  returnLineItems: [
    {
      returnLineItemId: removedReturnLineItemId,
      quantity: 1,
    },
  ],
});
requireEmptyUserErrors(removedAllLines, 'removeFromReturn');
requireMutationOrderStatus(removedAllLines, 'removeFromReturn', 'RETURNED');
const removedRead = await captureOrderStatusRead(removedSeed.orderId, removedSeed.orderQuery, 'RETURNED');

async function cleanupOrder(orderId: string): Promise<GraphqlCapture> {
  return capture(orderCancelMutation, {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });
}

const cleanup = {
  zero: await cleanupOrder(zeroOrderId),
  mixed: await cleanupOrder(mixedSeed.orderId),
  declined: await cleanupOrder(declinedSeed.orderId),
  canceled: await cleanupOrder(canceledSeed.orderId),
  removed: await cleanupOrder(removedSeed.orderId),
};

const setupSeed = ({ returnOrderHydrate: _hydrate, ...rest }: ReturnSeed): Omit<ReturnSeed, 'returnOrderHydrate'> =>
  rest;
const returnSeeds = [mixedSeed, declinedSeed, canceledSeed, removedSeed];

const orderIdDifferences = [
  {
    path: '$.order.id',
    matcher: 'shopify-gid:Order',
    reason: 'Shopify and the proxy allocate independent order IDs for orderCreate.',
  },
  {
    path: '$.order.name',
    matcher: 'non-empty-string',
    reason: 'Shopify and the proxy allocate independent order names for orderCreate.',
  },
];
const zeroReadOrderIdDifferences = [
  {
    path: '$.detail.id',
    matcher: 'shopify-gid:Order',
    reason: 'Shopify and the proxy allocate independent order IDs for orderCreate.',
  },
  {
    path: '$.list.nodes[*].id',
    matcher: 'shopify-gid:Order',
    reason: 'Shopify and the proxy allocate independent order IDs for orderCreate.',
  },
  {
    path: '$.node.id',
    matcher: 'shopify-gid:Order',
    reason: 'Shopify and the proxy allocate independent order IDs for orderCreate.',
  },
  {
    path: '$.detail.name',
    matcher: 'non-empty-string',
    reason: 'Shopify and the proxy allocate independent order names for orderCreate.',
  },
  {
    path: '$.list.nodes[*].name',
    matcher: 'non-empty-string',
    reason: 'Shopify and the proxy allocate independent order names for orderCreate.',
  },
  {
    path: '$.node.name',
    matcher: 'non-empty-string',
    reason: 'Shopify and the proxy allocate independent order names for orderCreate.',
  },
];
const mutationClosedAtDifferences = [
  {
    path: '$.return.closedAt',
    matcher: 'iso-timestamp',
    reason: 'Shopify and the proxy stamp independent return close timestamps.',
  },
  {
    path: '$.return.order.returns.nodes[0].closedAt',
    matcher: 'iso-timestamp',
    reason: 'Shopify and the proxy stamp independent return close timestamps.',
  },
];
const readClosedAtDifferences = [
  {
    path: '$.detail.returns.nodes[0].closedAt',
    matcher: 'iso-timestamp',
    reason: 'Shopify and the proxy stamp independent return close timestamps.',
  },
  {
    path: '$.list.nodes[0].returns.nodes[0].closedAt',
    matcher: 'iso-timestamp',
    reason: 'Shopify and the proxy stamp independent return close timestamps.',
  },
  {
    path: '$.node.returns.nodes[0].closedAt',
    matcher: 'iso-timestamp',
    reason: 'Shopify and the proxy stamp independent return close timestamps.',
  },
];

function mutationTarget(
  name: string,
  capturePath: string,
  proxyPath: string,
  documentPath: string,
  variables: JsonRecord,
  expectedDifferences?: unknown[],
): JsonRecord {
  const target: JsonRecord = {
    name,
    capturePath,
    proxyPath,
    proxyRequest: {
      documentPath,
      variables,
    },
  };
  if (expectedDifferences) target['expectedDifferences'] = expectedDifferences;
  return target;
}

function readTarget(
  name: string,
  capturePath: string,
  orderId: JsonRecord,
  orderName: JsonRecord,
  expectedDifferences?: unknown[],
): JsonRecord {
  const target: JsonRecord = {
    name,
    capturePath,
    proxyPath: '$.data',
    proxyRequest: {
      documentPath: 'config/parity-requests/orders/order-return-status-read.graphql',
      variables: {
        orderId,
        query: { ...orderName, prefix: 'name:' },
      },
    },
  };
  if (expectedDifferences) target['expectedDifferences'] = expectedDifferences;
  return target;
}

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  source: 'live-shopify-admin-graphql',
  notes:
    'Live Admin GraphQL 2026-04 capture for Order.returnStatus aggregation across zero, requested, open, mixed requested/open/declined, closed/reopened, processed, canceled, declined-only, and removed-line return states.',
  setup: {
    mixed: setupSeed(mixedSeed),
    declined: setupSeed(declinedSeed),
    canceled: setupSeed(canceledSeed),
    removed: setupSeed(removedSeed),
  },
  declineInput,
  zeroCase: {
    orderQuery: orderNameQuery(zeroOrderName),
    orderCreate: zeroOrderCreate,
    readAfterCreate: zeroRead,
  },
  mixedCase: {
    orderQuery: mixedSeed.orderQuery,
    firstReturnRequest: mixedFirstRequest,
    readAfterFirstRequest: mixedReadAfterFirstRequest,
    approveFirst: mixedApproveFirst,
    readAfterApprove: mixedReadAfterApprove,
    secondReturnRequest: mixedSecondRequest,
    readAfterSecondRequest: mixedReadAfterSecondRequest,
    declineSecond: mixedDeclineSecond,
    readAfterDecline: mixedReadAfterDecline,
    closeFirst: mixedCloseFirst,
    readAfterClose: mixedReadAfterClose,
    reopenFirst: mixedReopenFirst,
    readAfterReopen: mixedReadAfterReopen,
    processFirst: mixedProcessFirst,
    readAfterProcess: mixedReadAfterProcess,
  },
  declinedCase: {
    orderQuery: declinedSeed.orderQuery,
    returnRequest: declinedRequest,
    declineRequest: declinedOnly,
    readAfterDecline: declinedRead,
  },
  canceledCase: {
    orderQuery: canceledSeed.orderQuery,
    returnRequest: canceledRequest,
    approveRequest: canceledApprove,
    cancel: canceledOnly,
    readAfterCancel: canceledRead,
  },
  removedCase: {
    orderQuery: removedSeed.orderQuery,
    returnCreate: removedCreate,
    removeFromReturn: removedAllLines,
    readAfterRemove: removedRead,
  },
  cleanup,
  upstreamCalls: returnSeeds.map((seed) => ({
    operationName: 'OrdersReturnOrderHydrate',
    variables: { id: seed.orderId },
    query: returnOrderHydrateQuery,
    response: {
      status: seed.returnOrderHydrate.status,
      body: seed.returnOrderHydrate.payload,
    },
  })),
});

await writeJson(specPath, {
  scenarioId: 'order-return-status-lifecycle',
  operationNames: [
    'node',
    'order',
    'orders',
    'orderCreate',
    'returnCreate',
    'returnRequest',
    'returnApproveRequest',
    'returnDeclineRequest',
    'returnClose',
    'returnReopen',
    'returnCancel',
    'removeFromReturn',
    'returnProcess',
  ],
  scenarioStatus: 'captured',
  assertionKinds: ['payload-shape', 'lifecycle-transition-parity', 'downstream-read-parity'],
  liveCaptureFiles: [fixturePath],
  proxyRequest: {
    documentPath: 'config/parity-requests/orders/order-return-status-order-create.graphql',
    variablesCapturePath: '$.zeroCase.orderCreate.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  notes:
    'Live Shopify 2026-04 evidence for Order.returnStatus aggregation. Targets compare zero returns, requested, open, requested-over-open precedence, declined-only, canceled-only, closed, reopened, processed-open, and removed-all-line states through mutation payload order selections plus order/list/node reads.',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [
      {
        path: '$.return.id',
        matcher: 'shopify-gid:Return',
        reason: 'Shopify and the proxy allocate independent return IDs.',
      },
      {
        path: '$.return.returnLineItems.nodes[*].id',
        matcher: 'shopify-gid:ReturnLineItem',
        reason: 'Shopify and the proxy allocate independent return line item IDs.',
      },
      {
        path: '$.return.order.returns.nodes[*].id',
        matcher: 'shopify-gid:Return',
        reason: 'Shopify and the proxy allocate independent return IDs.',
      },
      {
        path: '$.return.order.returns.nodes[*].returnLineItems.nodes[*].id',
        matcher: 'shopify-gid:ReturnLineItem',
        reason: 'Shopify and the proxy allocate independent return line item IDs.',
      },
      {
        path: '$.detail.returns.nodes[*].id',
        matcher: 'shopify-gid:Return',
        reason: 'Shopify and the proxy allocate independent return IDs.',
      },
      {
        path: '$.detail.returns.nodes[*].returnLineItems.nodes[*].id',
        matcher: 'shopify-gid:ReturnLineItem',
        reason: 'Shopify and the proxy allocate independent return line item IDs.',
      },
      {
        path: '$.list.nodes[*].returns.nodes[*].id',
        matcher: 'shopify-gid:Return',
        reason: 'Shopify and the proxy allocate independent return IDs.',
      },
      {
        path: '$.list.nodes[*].returns.nodes[*].returnLineItems.nodes[*].id',
        matcher: 'shopify-gid:ReturnLineItem',
        reason: 'Shopify and the proxy allocate independent return line item IDs.',
      },
      {
        path: '$.node.returns.nodes[*].id',
        matcher: 'shopify-gid:Return',
        reason: 'Shopify and the proxy allocate independent return IDs.',
      },
      {
        path: '$.node.returns.nodes[*].returnLineItems.nodes[*].id',
        matcher: 'shopify-gid:ReturnLineItem',
        reason: 'Shopify and the proxy allocate independent return line item IDs.',
      },
    ],
    targets: [
      {
        name: 'zero-order-create-payload',
        capturePath: '$.zeroCase.orderCreate.response.payload.data.orderCreate',
        proxyPath: '$.data.orderCreate',
        expectedDifferences: orderIdDifferences,
      },
      readTarget(
        'zero-order-projections-read',
        '$.zeroCase.readAfterCreate.response.payload.data',
        { fromPrimaryProxyPath: '$.data.orderCreate.order.id' },
        { fromPrimaryProxyPath: '$.data.orderCreate.order.name' },
        zeroReadOrderIdDifferences,
      ),
      mutationTarget(
        'mixed-requested-payload',
        '$.mixedCase.firstReturnRequest.response.payload.data.returnRequest',
        '$.data.returnRequest',
        'config/parity-requests/orders/order-return-status-return-request.graphql',
        { fromCapturePath: '$.mixedCase.firstReturnRequest.variables' },
      ),
      readTarget(
        'mixed-requested-projections-read',
        '$.mixedCase.readAfterFirstRequest.response.payload.data',
        { fromProxyResponse: 'mixed-requested-payload', path: '$.data.returnRequest.return.order.id' },
        { fromProxyResponse: 'mixed-requested-payload', path: '$.data.returnRequest.return.order.name' },
      ),
      mutationTarget(
        'mixed-open-approve-payload',
        '$.mixedCase.approveFirst.response.payload.data.returnApproveRequest',
        '$.data.returnApproveRequest',
        'config/parity-requests/orders/order-return-status-return-approve.graphql',
        {
          input: {
            id: { fromProxyResponse: 'mixed-requested-payload', path: '$.data.returnRequest.return.id' },
          },
        },
      ),
      readTarget(
        'mixed-open-projections-read',
        '$.mixedCase.readAfterApprove.response.payload.data',
        { fromProxyResponse: 'mixed-open-approve-payload', path: '$.data.returnApproveRequest.return.order.id' },
        { fromProxyResponse: 'mixed-open-approve-payload', path: '$.data.returnApproveRequest.return.order.name' },
      ),
      mutationTarget(
        'mixed-requested-precedence-payload',
        '$.mixedCase.secondReturnRequest.response.payload.data.returnRequest',
        '$.data.returnRequest',
        'config/parity-requests/orders/order-return-status-return-request.graphql',
        { fromCapturePath: '$.mixedCase.secondReturnRequest.variables' },
      ),
      readTarget(
        'mixed-requested-precedence-projections-read',
        '$.mixedCase.readAfterSecondRequest.response.payload.data',
        { fromProxyResponse: 'mixed-requested-precedence-payload', path: '$.data.returnRequest.return.order.id' },
        { fromProxyResponse: 'mixed-requested-precedence-payload', path: '$.data.returnRequest.return.order.name' },
      ),
      mutationTarget(
        'mixed-declined-open-payload',
        '$.mixedCase.declineSecond.response.payload.data.returnDeclineRequest',
        '$.data.returnDeclineRequest',
        'config/parity-requests/orders/order-return-status-return-decline.graphql',
        {
          input: {
            id: { fromProxyResponse: 'mixed-requested-precedence-payload', path: '$.data.returnRequest.return.id' },
            declineReason: { fromCapturePath: '$.declineInput.declineReason' },
            declineNote: { fromCapturePath: '$.declineInput.declineNote' },
            notifyCustomer: { fromCapturePath: '$.declineInput.notifyCustomer' },
          },
        },
      ),
      readTarget(
        'mixed-declined-open-projections-read',
        '$.mixedCase.readAfterDecline.response.payload.data',
        { fromProxyResponse: 'mixed-declined-open-payload', path: '$.data.returnDeclineRequest.return.order.id' },
        { fromProxyResponse: 'mixed-declined-open-payload', path: '$.data.returnDeclineRequest.return.order.name' },
      ),
      mutationTarget(
        'mixed-closed-returned-payload',
        '$.mixedCase.closeFirst.response.payload.data.returnClose',
        '$.data.returnClose',
        'config/parity-requests/orders/order-return-status-return-close.graphql',
        {
          id: { fromProxyResponse: 'mixed-open-approve-payload', path: '$.data.returnApproveRequest.return.id' },
        },
        mutationClosedAtDifferences,
      ),
      readTarget(
        'mixed-closed-projections-read',
        '$.mixedCase.readAfterClose.response.payload.data',
        { fromProxyResponse: 'mixed-closed-returned-payload', path: '$.data.returnClose.return.order.id' },
        { fromProxyResponse: 'mixed-closed-returned-payload', path: '$.data.returnClose.return.order.name' },
        readClosedAtDifferences,
      ),
      mutationTarget(
        'mixed-reopened-in-progress-payload',
        '$.mixedCase.reopenFirst.response.payload.data.returnReopen',
        '$.data.returnReopen',
        'config/parity-requests/orders/order-return-status-return-reopen.graphql',
        {
          id: { fromProxyResponse: 'mixed-closed-returned-payload', path: '$.data.returnClose.return.id' },
        },
      ),
      readTarget(
        'mixed-reopened-projections-read',
        '$.mixedCase.readAfterReopen.response.payload.data',
        { fromProxyResponse: 'mixed-reopened-in-progress-payload', path: '$.data.returnReopen.return.order.id' },
        { fromProxyResponse: 'mixed-reopened-in-progress-payload', path: '$.data.returnReopen.return.order.name' },
      ),
      mutationTarget(
        'mixed-processed-open-payload',
        '$.mixedCase.processFirst.response.payload.data.returnProcess',
        '$.data.returnProcess',
        'config/parity-requests/orders/order-return-status-return-process.graphql',
        {
          input: {
            returnId: {
              fromProxyResponse: 'mixed-reopened-in-progress-payload',
              path: '$.data.returnReopen.return.id',
            },
            returnLineItems: [
              {
                id: {
                  fromProxyResponse: 'mixed-open-approve-payload',
                  path: '$.data.returnApproveRequest.return.returnLineItems.nodes[0].id',
                },
                quantity: 1,
              },
            ],
            notifyCustomer: false,
          },
        },
      ),
      readTarget(
        'mixed-processed-projections-read',
        '$.mixedCase.readAfterProcess.response.payload.data',
        { fromProxyResponse: 'mixed-processed-open-payload', path: '$.data.returnProcess.return.order.id' },
        { fromProxyResponse: 'mixed-processed-open-payload', path: '$.data.returnProcess.return.order.name' },
      ),
      mutationTarget(
        'declined-only-requested-payload',
        '$.declinedCase.returnRequest.response.payload.data.returnRequest',
        '$.data.returnRequest',
        'config/parity-requests/orders/order-return-status-return-request.graphql',
        { fromCapturePath: '$.declinedCase.returnRequest.variables' },
      ),
      mutationTarget(
        'declined-only-declined-payload',
        '$.declinedCase.declineRequest.response.payload.data.returnDeclineRequest',
        '$.data.returnDeclineRequest',
        'config/parity-requests/orders/order-return-status-return-decline.graphql',
        {
          input: {
            id: { fromProxyResponse: 'declined-only-requested-payload', path: '$.data.returnRequest.return.id' },
            declineReason: { fromCapturePath: '$.declineInput.declineReason' },
            declineNote: { fromCapturePath: '$.declineInput.declineNote' },
            notifyCustomer: { fromCapturePath: '$.declineInput.notifyCustomer' },
          },
        },
      ),
      readTarget(
        'declined-only-projections-read',
        '$.declinedCase.readAfterDecline.response.payload.data',
        { fromProxyResponse: 'declined-only-declined-payload', path: '$.data.returnDeclineRequest.return.order.id' },
        { fromProxyResponse: 'declined-only-declined-payload', path: '$.data.returnDeclineRequest.return.order.name' },
      ),
      mutationTarget(
        'canceled-only-requested-payload',
        '$.canceledCase.returnRequest.response.payload.data.returnRequest',
        '$.data.returnRequest',
        'config/parity-requests/orders/order-return-status-return-request.graphql',
        { fromCapturePath: '$.canceledCase.returnRequest.variables' },
      ),
      mutationTarget(
        'canceled-only-open-payload',
        '$.canceledCase.approveRequest.response.payload.data.returnApproveRequest',
        '$.data.returnApproveRequest',
        'config/parity-requests/orders/order-return-status-return-approve.graphql',
        {
          input: {
            id: { fromProxyResponse: 'canceled-only-requested-payload', path: '$.data.returnRequest.return.id' },
          },
        },
      ),
      mutationTarget(
        'canceled-only-canceled-payload',
        '$.canceledCase.cancel.response.payload.data.returnCancel',
        '$.data.returnCancel',
        'config/parity-requests/orders/order-return-status-return-cancel.graphql',
        {
          id: { fromProxyResponse: 'canceled-only-open-payload', path: '$.data.returnApproveRequest.return.id' },
        },
      ),
      readTarget(
        'canceled-only-projections-read',
        '$.canceledCase.readAfterCancel.response.payload.data',
        { fromProxyResponse: 'canceled-only-canceled-payload', path: '$.data.returnCancel.return.order.id' },
        { fromProxyResponse: 'canceled-only-canceled-payload', path: '$.data.returnCancel.return.order.name' },
      ),
      mutationTarget(
        'removed-open-create-payload',
        '$.removedCase.returnCreate.response.payload.data.returnCreate',
        '$.data.returnCreate',
        'config/parity-requests/orders/order-return-status-return-create.graphql',
        { fromCapturePath: '$.removedCase.returnCreate.variables' },
      ),
      mutationTarget(
        'removed-all-lines-payload',
        '$.removedCase.removeFromReturn.response.payload.data.removeFromReturn',
        '$.data.removeFromReturn',
        'config/parity-requests/orders/order-return-status-remove-from-return.graphql',
        {
          returnId: { fromProxyResponse: 'removed-open-create-payload', path: '$.data.returnCreate.return.id' },
          returnLineItems: [
            {
              returnLineItemId: {
                fromProxyResponse: 'removed-open-create-payload',
                path: '$.data.returnCreate.return.returnLineItems.nodes[0].id',
              },
              quantity: 1,
            },
          ],
        },
        mutationClosedAtDifferences,
      ),
      readTarget(
        'removed-projections-read',
        '$.removedCase.readAfterRemove.response.payload.data',
        { fromProxyResponse: 'removed-all-lines-payload', path: '$.data.removeFromReturn.return.order.id' },
        { fromProxyResponse: 'removed-all-lines-payload', path: '$.data.removeFromReturn.return.order.name' },
        readClosedAtDifferences,
      ),
    ],
  },
});

formatGeneratedJson([fixturePath, specPath]);

console.log(
  JSON.stringify(
    {
      fixturePath,
      specPath,
      zeroOrderId,
      mixedFirstReturnId,
      mixedSecondReturnId,
      declinedReturnId,
      canceledReturnId,
      removedReturnId,
      cleanupUserErrors: {
        zero: readArray(payloadRoot(cleanup.zero, 'orderCancel')['userErrors']),
        mixed: readArray(payloadRoot(cleanup.mixed, 'orderCancel')['userErrors']),
        declined: readArray(payloadRoot(cleanup.declined, 'orderCancel')['userErrors']),
        canceled: readArray(payloadRoot(cleanup.canceled, 'orderCancel')['userErrors']),
        removed: readArray(payloadRoot(cleanup.removed, 'orderCancel')['userErrors']),
      },
    },
    null,
    2,
  ),
);
