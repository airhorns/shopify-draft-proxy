/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type GraphqlCapture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};
type ReturnQuantitySeed = {
  source: 'created' | 'existing-disposable-order';
  label: string;
  orderId: string;
  fulfillmentLineItemId: string;
  quantity: number;
  orderCreate?: GraphqlCapture;
  fulfillmentCreate?: GraphqlCapture;
  orderRead: GraphqlCapture;
  returnOrderHydrateBeforeReturn: ConformanceGraphqlResult<JsonRecord>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'return-quantity-validation.json');
const requestDir = path.join('config', 'parity-requests', 'orders');

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
    response: await runGraphqlRequest<JsonRecord>(query, variables),
  };
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
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

function requireNoTopLevelErrors(result: ConformanceGraphqlResult<JsonRecord>, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload['errors']) {
    throw new Error(`${context} returned top-level errors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function requireEmptyUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  requireNoTopLevelErrors(captureResult.response, rootName);
  const root = payloadRoot(captureResult, rootName);
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`Unexpected ${rootName} userErrors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function requireUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  requireNoTopLevelErrors(captureResult.response, rootName);
  const root = payloadRoot(captureResult, rootName);
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length === 0) {
    throw new Error(`Expected ${rootName} userErrors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function hasOrderCreateGuard(captureResult: GraphqlCapture): boolean {
  const errors = readArray(payloadRoot(captureResult, 'orderCreate')['userErrors']).map(readRecord);
  return errors.some((error) => error?.['message'] === 'Too many attempts. Please try again later.');
}

function firstFulfillmentOrder(order: JsonRecord): JsonRecord {
  return readNodes(order['fulfillmentOrders'])[0] ?? {};
}

function firstFulfillmentLineItem(order: JsonRecord): JsonRecord {
  return readNodes(readRecord(readArray(order['fulfillments'])[0])?.['fulfillmentLineItems'])[0] ?? {};
}

const orderFields = `#graphql
  fragment ReturnQuantityValidationOrderFields on Order {
    id
    name
    createdAt
    updatedAt
    displayFinancialStatus
    displayFulfillmentStatus
    cancelledAt
    tags
    lineItems(first: 5) {
      nodes {
        id
        title
        quantity
        currentQuantity
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
    fulfillmentOrders(first: 5) {
      nodes {
        id
        status
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
    returns(first: 5) {
      nodes {
        id
        name
        status
        totalQuantity
        returnLineItems(first: 5) {
          nodes {
            ... on ReturnLineItem {
              id
              quantity
              fulfillmentLineItem {
                id
              }
            }
          }
        }
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${orderFields}
  mutation ReturnQuantityValidationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...ReturnQuantityValidationOrderFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation ReturnQuantityValidationFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
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

const orderReadQuery = `#graphql
  ${orderFields}
  query ReturnQuantityValidationOrderRead($id: ID!) {
    order(id: $id) {
      ...ReturnQuantityValidationOrderFields
    }
  }
`;

const candidateOrdersQuery = `#graphql
  ${orderFields}
  query ReturnQuantityValidationCandidateOrders($cursor: String) {
    orders(first: 50, after: $cursor, reverse: true, sortKey: CREATED_AT) {
      pageInfo {
        hasNextPage
        endCursor
      }
      nodes {
        ...ReturnQuantityValidationOrderFields
      }
    }
  }
`;

const returnRequestMutation = await readRequest('return-request-quantity-cap.graphql');
const returnCreateMutation = await readRequest('return-create-quantity-validation.graphql');
const removeFromReturnMutation = await readRequest('remove-from-return-quantity-validation.graphql');
const returnOrderHydrateQuery = await readRequest('return-order-hydrate.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

function orderVariables(label: string, quantity: number): JsonRecord {
  return {
    order: {
      email: `return-quantity-${label}-${stamp}@example.com`,
      note: `return quantity validation capture ${label} ${stamp}`,
      tags: ['return-quantity-validation', label, stamp, 'shopify-draft-proxy'],
      test: true,
      currency: 'USD',
      shippingAddress: {
        firstName: 'Return',
        lastName: 'Quantity',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          variantId: 'gid://shopify/ProductVariant/48540157378793',
          title: `Return quantity ${label} item ${stamp}`,
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
}

function returnInput(seed: ReturnQuantitySeed, quantity: number): JsonRecord {
  return {
    orderId: seed.orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId: seed.fulfillmentLineItemId,
        quantity,
        returnReason: 'UNWANTED',
      },
    ],
  };
}

async function readOrder(orderId: string): Promise<GraphqlCapture> {
  const read = await capture(orderReadQuery, { id: orderId });
  requireNoTopLevelErrors(read.response, `read ${orderId}`);
  return read;
}

function orderFromRead(read: GraphqlCapture): JsonRecord {
  const payload = readRecord(read.response.payload) ?? {};
  const data = readRecord(payload['data']) ?? {};
  return readRecord(data['order']) ?? {};
}

async function hydrateBeforeReturn(orderId: string): Promise<ConformanceGraphqlResult<JsonRecord>> {
  const hydrate = await runGraphqlRequest<JsonRecord>(returnOrderHydrateQuery, { id: orderId });
  requireNoTopLevelErrors(hydrate, `return-order hydrate ${orderId}`);
  return hydrate;
}

async function createFulfilledSeed(label: string, quantity: number): Promise<ReturnQuantitySeed | null> {
  const orderCreate = await capture(orderCreateMutation, orderVariables(label, quantity));
  if (hasOrderCreateGuard(orderCreate)) return null;
  requireEmptyUserErrors(orderCreate, 'orderCreate');

  const createdOrder = readRecord(payloadRoot(orderCreate, 'orderCreate')['order']) ?? {};
  const orderId = requireString(createdOrder['id'], `${label} order id`);
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
    message: `return quantity validation fulfillment ${label} ${stamp}`,
  });
  requireEmptyUserErrors(fulfillmentCreate, 'fulfillmentCreate');

  const orderRead = await readOrder(orderId);
  const fulfillmentLineItem = firstFulfillmentLineItem(orderFromRead(orderRead));
  const fulfillmentLineItemId = requireString(fulfillmentLineItem['id'], `${label} fulfillment line item id`);
  const returnOrderHydrateBeforeReturn = await hydrateBeforeReturn(orderId);

  return {
    source: 'created',
    label,
    orderId,
    fulfillmentLineItemId,
    quantity,
    orderCreate,
    fulfillmentCreate,
    orderRead,
    returnOrderHydrateBeforeReturn,
  };
}

function isReusableDisposableOrder(order: JsonRecord): boolean {
  const tags = readArray(order['tags']);
  if (!tags.includes('shopify-draft-proxy')) return false;
  if (order['cancelledAt']) return false;
  if (readNodes(order['returns']).length > 0) return false;
  return firstFulfillmentLineItem(order)['id'] !== undefined;
}

async function findExistingDisposableSeeds(): Promise<ReturnQuantitySeed[]> {
  const seeds: ReturnQuantitySeed[] = [];
  let cursor: string | null = null;
  for (let page = 0; page < 8 && seeds.length < 2; page += 1) {
    const result = await runGraphqlRequest<JsonRecord>(candidateOrdersQuery, { cursor });
    requireNoTopLevelErrors(result, 'candidate order search');
    const ordersConnection = readRecord(readRecord(result.payload['data'])?.['orders']);
    const orders = readNodes(ordersConnection);
    for (const order of orders) {
      if (!isReusableDisposableOrder(order)) continue;
      const orderId = requireString(order['id'], 'existing order id');
      const orderRead = await readOrder(orderId);
      const fulfillmentLineItem = firstFulfillmentLineItem(orderFromRead(orderRead));
      const fulfillmentLineItemId = requireString(fulfillmentLineItem['id'], 'existing fulfillment line item id');
      const quantity = Number(fulfillmentLineItem['quantity'] ?? 1);
      seeds.push({
        source: 'existing-disposable-order',
        label: `existing-${seeds.length + 1}`,
        orderId,
        fulfillmentLineItemId,
        quantity,
        orderRead,
        returnOrderHydrateBeforeReturn: await hydrateBeforeReturn(orderId),
      });
      if (seeds.length >= 2) break;
    }
    const pageInfo = readRecord(ordersConnection?.['pageInfo']);
    if (pageInfo?.['hasNextPage'] !== true) break;
    cursor = typeof pageInfo['endCursor'] === 'string' ? pageInfo['endCursor'] : null;
  }
  return seeds;
}

async function seedPair(): Promise<{
  quantityCap: ReturnQuantitySeed;
  removal: ReturnQuantitySeed;
  freshCreateBlocked: boolean;
}> {
  const quantityCap = await createFulfilledSeed('quantity-cap', 2);
  const removal = quantityCap ? await createFulfilledSeed('removal', 1) : null;
  if (quantityCap && removal) return { quantityCap, removal, freshCreateBlocked: false };

  const existing = await findExistingDisposableSeeds();
  if (existing.length < 2) {
    throw new Error(
      'Fresh orderCreate was blocked and fewer than two existing disposable fulfilled orders were available.',
    );
  }
  return { quantityCap: existing[0], removal: existing[1], freshCreateBlocked: true };
}

const { quantityCap, removal, freshCreateBlocked } = await seedPair();

const existingReturnCreate = await capture(returnCreateMutation, {
  returnInput: returnInput(quantityCap, 1),
});
requireEmptyUserErrors(existingReturnCreate, 'returnCreate');
const quantityCapHydrateAfterExistingReturn = await runGraphqlRequest<JsonRecord>(returnOrderHydrateQuery, {
  id: quantityCap.orderId,
});
requireNoTopLevelErrors(quantityCapHydrateAfterExistingReturn, 'quantity cap hydrate after existing return');

const returnRequestQuantityCap = await capture(returnRequestMutation, {
  input: returnInput(quantityCap, Math.max(2, quantityCap.quantity)),
});
requireUserErrors(returnRequestQuantityCap, 'returnRequest');
const returnCreateQuantityCap = await capture(returnCreateMutation, {
  returnInput: returnInput(quantityCap, Math.max(2, quantityCap.quantity)),
});
requireUserErrors(returnCreateQuantityCap, 'returnCreate');

const returnCreateForRemoval = await capture(returnCreateMutation, {
  returnInput: {
    ...returnInput(removal, 1),
    returnLineItems: [
      {
        fulfillmentLineItemId: removal.fulfillmentLineItemId,
        quantity: 1,
        returnReason: 'UNWANTED',
        returnReasonNote: 'Testing removal validation',
      },
    ],
  },
});
requireEmptyUserErrors(returnCreateForRemoval, 'returnCreate');
const removalReturn = readRecord(payloadRoot(returnCreateForRemoval, 'returnCreate')['return']) ?? {};
const removalReturnLineItem = readNodes(removalReturn['returnLineItems'])[0] ?? {};
const removalReturnId = requireString(removalReturn['id'], 'removal return id');
const removalReturnLineItemId = requireString(removalReturnLineItem['id'], 'removal return line item id');
const removeFromReturnOverQuantity = await capture(removeFromReturnMutation, {
  returnId: removalReturnId,
  returnLineItems: [{ returnLineItemId: removalReturnLineItemId, quantity: 2 }],
});
requireUserErrors(removeFromReturnOverQuantity, 'removeFromReturn');
const removeFromReturnZeroQuantity = await capture(removeFromReturnMutation, {
  returnId: removalReturnId,
  returnLineItems: [{ returnLineItemId: removalReturnLineItemId, quantity: 0 }],
});
requireUserErrors(removeFromReturnZeroQuantity, 'removeFromReturn');

const setupSeed = ({
  returnOrderHydrateBeforeReturn: _hydrate,
  ...seed
}: ReturnQuantitySeed): Omit<ReturnQuantitySeed, 'returnOrderHydrateBeforeReturn'> => seed;

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  source: 'live-shopify-admin-graphql',
  notes:
    'Live return quantity validation capture. The recorder first tries to create and fulfill fresh disposable orders; this run used existing conformance-tagged disposable fulfilled orders only if Shopify returned the resolver-level orderCreate guard "Too many attempts. Please try again later."',
  freshCreateBlocked,
  setup: {
    quantityCap: setupSeed(quantityCap),
    removal: setupSeed(removal),
    existingReturnCreate,
  },
  returnRequestQuantityCap: {
    variables: returnRequestQuantityCap.variables,
    response: returnRequestQuantityCap.response.payload,
  },
  returnCreateQuantityCap: {
    variables: returnCreateQuantityCap.variables,
    response: returnCreateQuantityCap.response.payload,
  },
  returnCreateForRemoval: {
    variables: returnCreateForRemoval.variables,
    response: returnCreateForRemoval.response.payload,
  },
  expected: {
    returnRequestQuantityCap: returnRequestQuantityCap.response.payload,
    returnCreateQuantityCap: returnCreateQuantityCap.response.payload,
    removeFromReturnOverQuantity: removeFromReturnOverQuantity.response.payload,
    removeFromReturnZeroQuantity: removeFromReturnZeroQuantity.response.payload,
  },
  cleanup: {},
  upstreamCalls: [
    {
      operationName: 'OrdersReturnOrderHydrate',
      variables: { id: quantityCap.orderId },
      query: returnOrderHydrateQuery,
      response: {
        status: quantityCapHydrateAfterExistingReturn.status,
        body: quantityCapHydrateAfterExistingReturn.payload,
      },
    },
    {
      operationName: 'OrdersReturnOrderHydrate',
      variables: { id: removal.orderId },
      query: returnOrderHydrateQuery,
      response: {
        status: removal.returnOrderHydrateBeforeReturn.status,
        body: removal.returnOrderHydrateBeforeReturn.payload,
      },
    },
  ],
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      freshCreateBlocked,
      quantityCapOrderId: quantityCap.orderId,
      removalOrderId: removal.orderId,
      removalReturnId,
    },
    null,
    2,
  ),
);
