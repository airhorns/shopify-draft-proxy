/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'order-edit-commit-history-and-fulfillment-orders.json');
const paritySpecPath = path.join(
  'config',
  'parity-specs',
  'orders',
  'orderEditCommit-history-and-fulfillment-orders.json',
);
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
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
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

function responseData(captureResult: GraphqlCapture): JsonRecord {
  const data = readRecord(captureResult.response.payload.data);
  if (!data) {
    throw new Error(`Expected GraphQL data for capture: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
  return data;
}

function mutationPayload(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  const root = readRecord(responseData(captureResult)[rootName]);
  if (!root) {
    throw new Error(`Expected ${rootName} payload: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
  return root;
}

function requireEmptyUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  const payload = captureResult.response.payload;
  const root = mutationPayload(captureResult, rootName);
  const userErrors = readArray(root['userErrors']);
  if (payload.errors || userErrors.length > 0) {
    throw new Error(`Unexpected ${rootName} errors: ${JSON.stringify(payload, null, 2)}`);
  }
}

function firstActiveLocationId(locations: GraphqlCapture): string {
  const nodes = readNodes(responseData(locations)['locations']);
  const location = nodes.find((node) => node['isActive'] !== false) ?? nodes[0];
  return requireString(location?.['id'], 'active location id');
}

function firstProductVariant(seed: GraphqlCapture): JsonRecord {
  const products = readNodes(responseData(seed)['products']);
  for (const product of products) {
    const variants = readNodes(product['variants']);
    const variant = variants.find((node) => typeof node['id'] === 'string');
    if (variant) {
      return variant;
    }
  }
  throw new Error('No product variant available for order edit capture');
}

function orderFromPayload(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  const order = readRecord(mutationPayload(captureResult, rootName)['order']);
  if (!order) {
    throw new Error(`Expected ${rootName}.order payload: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
  return order;
}

function calculatedOrderId(captureResult: GraphqlCapture): string {
  const calculatedOrder = readRecord(mutationPayload(captureResult, 'orderEditBegin')['calculatedOrder']);
  return requireString(calculatedOrder?.['id'], 'calculated order id');
}

function firstCalculatedLineItemId(captureResult: GraphqlCapture): string {
  const calculatedOrder = readRecord(mutationPayload(captureResult, 'orderEditBegin')['calculatedOrder']);
  const lineItem = readNodes(readRecord(calculatedOrder?.['lineItems']))[0];
  return requireString(lineItem?.['id'], 'calculated line item id');
}

const beginDocument = await readRequest('orderEditCommit-history-fulfillment-begin.graphql');
const setQuantityDocument = await readRequest('orderEditCommit-history-fulfillment-setQuantity.graphql');
const addVariantDocument = await readRequest('orderEditCommit-history-fulfillment-addVariant.graphql');
const commitDocument = await readRequest('orderEditCommit-history-fulfillment-commit.graphql');
const downstreamReadDocument = await readRequest('orderEditCommit-history-fulfillment-downstream-read.graphql');

const orderFields = `#graphql
  fragment OrderEditCommitHistoryFulfillmentOrderFields on Order {
    id
    name
    email
    phone
    poNumber
    createdAt
    updatedAt
    closed
    closedAt
    cancelledAt
    cancelReason
    displayFinancialStatus
    displayFulfillmentStatus
    presentmentCurrencyCode
    note
    tags
    customAttributes {
      key
      value
    }
    currentSubtotalLineItemsQuantity
    currentSubtotalPriceSet {
      shopMoney {
        amount
        currencyCode
      }
      presentmentMoney {
        amount
        currencyCode
      }
    }
    currentTotalPriceSet {
      shopMoney {
        amount
        currencyCode
      }
      presentmentMoney {
        amount
        currencyCode
      }
    }
    currentTotalTaxSet {
      shopMoney {
        amount
        currencyCode
      }
      presentmentMoney {
        amount
        currencyCode
      }
    }
    currentTaxLines {
      title
      rate
      priceSet {
        shopMoney {
          amount
          currencyCode
        }
        presentmentMoney {
          amount
          currencyCode
        }
      }
    }
    totalPriceSet {
      shopMoney {
        amount
        currencyCode
      }
      presentmentMoney {
        amount
        currencyCode
      }
    }
    lineItems(first: 10) {
      nodes {
        id
        title
        name
        quantity
        currentQuantity
        sku
        variantTitle
        originalUnitPriceSet {
          shopMoney {
            amount
            currencyCode
          }
          presentmentMoney {
            amount
            currencyCode
          }
        }
        originalTotalSet {
          shopMoney {
            amount
            currencyCode
          }
          presentmentMoney {
            amount
            currencyCode
          }
        }
        variant {
          id
          title
          sku
        }
      }
    }
    fulfillmentOrders(first: 10) {
      nodes {
        id
        status
        requestStatus
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
    }
  }
`;

const seedQuery = `#graphql
  query OrderEditCommitHistoryFulfillmentSeed($first: Int!) {
    locations(first: 10) {
      nodes {
        id
        name
        isActive
      }
    }
    products(first: $first) {
      nodes {
        id
        title
        variants(first: 5) {
          nodes {
            id
            title
            sku
            price
            product {
              id
              title
            }
          }
        }
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${orderFields}
  mutation OrderEditCommitHistoryFulfillmentCreate(
    $order: OrderCreateOrderInput!
    $options: OrderCreateOptionsInput
  ) {
    orderCreate(order: $order, options: $options) {
      order {
        ...OrderEditCommitHistoryFulfillmentOrderFields
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
  query OrderEditCommitHistoryFulfillmentHydrate($id: ID!) {
    order(id: $id) {
      ...OrderEditCommitHistoryFulfillmentOrderFields
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation OrderEditCommitHistoryFulfillmentCancel(
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

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

const seed = await capture(seedQuery, { first: 20 });
const locationId = firstActiveLocationId(seed);
const variant = firstProductVariant(seed);
const variantId = requireString(variant['id'], 'variant id');

const orderCreateVariables = {
  order: {
    email: `order-edit-commit-${stamp}@example.com`,
    note: `order edit commit history fulfillment capture ${stamp}`,
    tags: ['order-edit-commit-history-fulfillment', stamp],
    test: true,
    currency: 'CAD',
    shippingAddress: {
      firstName: 'Conformance',
      lastName: 'OrderEdit',
      address1: '123 Queen St W',
      city: 'Toronto',
      provinceCode: 'ON',
      countryCode: 'CA',
      zip: 'M5H 2M9',
    },
    lineItems: [
      {
        title: `Order edit source item ${stamp}`,
        quantity: 3,
        priceSet: {
          shopMoney: {
            amount: '10.00',
            currencyCode: 'CAD',
          },
        },
        requiresShipping: true,
        taxable: true,
        sku: `order-edit-source-${stamp}`,
      },
    ],
  },
  options: {
    inventoryBehaviour: 'BYPASS',
    sendReceipt: false,
    sendFulfillmentReceipt: false,
  },
};

const orderCreate = await capture(orderCreateMutation, orderCreateVariables);
requireEmptyUserErrors(orderCreate, 'orderCreate');
const createdOrder = orderFromPayload(orderCreate, 'orderCreate');
const orderId = requireString(createdOrder['id'], 'created order id');

const orderReadBeforeEdit = await capture(orderReadQuery, { id: orderId });
const seedOrder = readRecord(responseData(orderReadBeforeEdit)['order']);
if (!seedOrder) {
  throw new Error(`Expected order read before edit: ${JSON.stringify(orderReadBeforeEdit.response.payload, null, 2)}`);
}

const begin = await capture(beginDocument, { id: orderId });
requireEmptyUserErrors(begin, 'orderEditBegin');
const calculatedOrderIdForCapture = calculatedOrderId(begin);
const calculatedLineItemIdForCapture = firstCalculatedLineItemId(begin);

const setQuantityVariables = {
  id: calculatedOrderIdForCapture,
  lineItemId: calculatedLineItemIdForCapture,
  quantity: 1,
  restock: false,
};
const setQuantity = await capture(setQuantityDocument, setQuantityVariables);
requireEmptyUserErrors(setQuantity, 'orderEditSetQuantity');

const addVariantVariables = {
  id: calculatedOrderIdForCapture,
  variantId,
  quantity: 2,
  allowDuplicates: true,
};
const addVariant = await capture(addVariantDocument, addVariantVariables);
requireEmptyUserErrors(addVariant, 'orderEditAddVariant');

const commitVariables = {
  id: calculatedOrderIdForCapture,
  notifyCustomer: false,
  staffNote: 'order edit commit history fulfillment capture',
};
const commit = await capture(commitDocument, commitVariables);
requireEmptyUserErrors(commit, 'orderEditCommit');

const downstreamRead = await capture(downstreamReadDocument, { id: orderId });
const downstreamOrder = readRecord(responseData(downstreamRead)['order']);
if (!downstreamOrder) {
  throw new Error(`Expected downstream order read: ${JSON.stringify(downstreamRead.response.payload, null, 2)}`);
}

const cleanup = await capture(orderCancelMutation, {
  orderId,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: true,
});

const upstreamCalls = [
  {
    operationName: 'OrdersOrderHydrate',
    variables: { id: orderId },
    query: 'hand-synthesized from live setup order read for orderEditCommit history and fulfillment-order replay',
    response: {
      status: 200,
      body: {
        data: {
          order: seedOrder,
        },
      },
    },
  },
  {
    operationName: 'OrdersProductVariantHydrate',
    variables: { id: variantId },
    query: 'hand-synthesized from live product variant seed for orderEditAddVariant replay',
    response: {
      status: 200,
      body: {
        data: {
          productVariant: variant,
        },
      },
    },
  },
];

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  source: 'live-shopify-admin-graphql',
  notes:
    'Live order edit commit capture covering a disposable order with one existing shippable line item decremented from 3 to 1, one variant line item added at quantity 2, the edited order event, current totals/tax lines, and fulfillment-order remaining quantities.',
  setupReferences: {
    selectedLocationId: locationId,
    selectedVariant: variant,
  },
  setup: {
    orderCreate,
    orderReadBeforeEdit,
  },
  begin,
  setQuantity,
  addVariant,
  commit,
  downstreamRead,
  cleanup,
  upstreamCalls,
});

await writeJson(paritySpecPath, {
  scenarioId: 'order-edit-commit-history-and-fulfillment-orders',
  operationNames: ['orderEditBegin', 'orderEditSetQuantity', 'orderEditAddVariant', 'orderEditCommit', 'order'],
  scenarioStatus: 'captured',
  assertionKinds: ['payload-shape', 'selected-fields', 'downstream-read-parity'],
  liveCaptureFiles: [fixturePath],
  proxyRequest: {
    documentPath: 'config/parity-requests/orders/orderEditCommit-history-fulfillment-begin.graphql',
    variablesCapturePath: '$.begin.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  notes:
    'Live captured order edit commit scenario proving the edited order event, fulfillment-order remaining quantities, and current order totals/tax lines after a set-quantity plus add-variant edit. Volatile Shopify/proxy allocated IDs and event timestamps are excluded; selected payload values are otherwise compared strictly.',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'begin-user-errors',
        capturePath: '$.begin.response.payload.data.orderEditBegin.userErrors',
        proxyPath: '$.data.orderEditBegin.userErrors',
      },
      {
        name: 'set-quantity-user-errors',
        capturePath: '$.setQuantity.response.payload.data.orderEditSetQuantity.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-history-fulfillment-setQuantity.graphql',
          variables: {
            id: {
              fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.id',
            },
            lineItemId: {
              fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.lineItems.nodes[0].id',
            },
            quantity: 1,
            restock: false,
          },
        },
        proxyPath: '$.data.orderEditSetQuantity.userErrors',
      },
      {
        name: 'add-variant-user-errors',
        capturePath: '$.addVariant.response.payload.data.orderEditAddVariant.userErrors',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-history-fulfillment-addVariant.graphql',
          variables: {
            id: {
              fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.id',
            },
            variantId,
            quantity: 2,
            allowDuplicates: true,
          },
        },
        proxyPath: '$.data.orderEditAddVariant.userErrors',
      },
      {
        name: 'commit-success',
        capturePath: '$.commit.response.payload.data.orderEditCommit.successMessages',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-history-fulfillment-commit.graphql',
          variables: {
            id: {
              fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.id',
            },
            notifyCustomer: false,
            staffNote: 'order edit commit history fulfillment capture',
          },
        },
        proxyPath: '$.data.orderEditCommit.successMessages',
      },
      {
        name: 'downstream-history-fulfillment-orders-and-totals',
        capturePath: '$.downstreamRead.response.payload.data.order',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/orderEditCommit-history-fulfillment-downstream-read.graphql',
          variables: {
            id: {
              fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.originalOrder.id',
            },
          },
        },
        proxyPath: '$.data.order',
        excludedPaths: [
          '$.updatedAt',
          '$.lineItems.nodes[*].id',
          '$.events.nodes[*].id',
          '$.events.nodes[*].createdAt',
          '$.fulfillmentOrders.nodes[*].id',
          '$.fulfillmentOrders.nodes[*].lineItems.nodes[*].id',
          '$.fulfillmentOrders.nodes[*].lineItems.nodes[*].lineItem.id',
        ],
      },
    ],
  },
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      paritySpecPath,
      orderId,
      variantId,
      locationId,
      cleanupUserErrors: readArray(mutationPayload(cleanup, 'orderCancel')['userErrors']),
    },
    null,
    2,
  ),
);
