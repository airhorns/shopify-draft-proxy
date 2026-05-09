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
  response: ConformanceGraphqlResult;
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
const fixturePath = path.join(fixtureDir, 'return-shipping-fee-recorded.json');
const requestDir = path.join('config', 'parity-requests', 'orders');
const specPath = path.join('config', 'parity-specs', 'orders', 'return-shipping-fee-recorded.json');

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

function returnPayload(captureResult: GraphqlCapture, rootName: string): JsonRecord {
  return readRecord(readRecord(captureResult.response.payload as JsonRecord)['data'])?.[rootName] as JsonRecord;
}

function requireEmptyUserErrors(captureResult: GraphqlCapture, rootName: string): void {
  const payload = captureResult.response.payload as JsonRecord;
  const errors = payload['errors'];
  const root = readRecord(readRecord(payload['data'])?.[rootName]);
  const userErrors = readArray(root?.['userErrors']);
  if (errors || userErrors.length > 0) {
    throw new Error(`Unexpected ${rootName} errors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function firstFulfillmentOrder(order: JsonRecord): JsonRecord {
  return readNodes(order['fulfillmentOrders'])[0] ?? {};
}

function firstFulfillmentLineItem(order: JsonRecord): JsonRecord {
  return readNodes(readRecord(readArray(order['fulfillments'])[0])?.['fulfillmentLineItems'])[0] ?? {};
}

function firstActiveReturnReasonDefinitionId(captureResult: GraphqlCapture): string {
  const data = readRecord(captureResult.response.payload as JsonRecord)?.['data'];
  const definitions = readNodes(readRecord(data)?.['returnReasonDefinitions']);
  const preferredDefinition =
    definitions.find((definition) => definition['handle'] === 'changed-my-mind' && definition['deleted'] !== true) ??
    definitions.find((definition) => definition['deleted'] !== true);
  return requireString(preferredDefinition?.['id'], 'active return reason definition id');
}

const orderFields = `#graphql
  fragment ReturnShippingFeeOrderFields on Order {
    id
    name
    createdAt
    updatedAt
    displayFinancialStatus
    displayFulfillmentStatus
    totalPriceSet { shopMoney { amount currencyCode } }
    currentTotalPriceSet { shopMoney { amount currencyCode } }
    fulfillments(first: 5) {
      id
      status
      displayStatus
      createdAt
      updatedAt
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
    returns(first: 5) {
      nodes {
        id
        name
        status
        totalQuantity
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${orderFields}
  mutation ReturnShippingFeeOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...ReturnShippingFeeOrderFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation ReturnShippingFeeFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        id
        status
        displayStatus
        createdAt
        updatedAt
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
  query ReturnShippingFeeOrderRead($id: ID!) {
    order(id: $id) {
      ...ReturnShippingFeeOrderFields
    }
  }
`;

const schemaQuery = `#graphql
  query ReturnShippingFeeSchemaEvidence {
    returnInput: __type(name: "ReturnInput") {
      inputFields(includeDeprecated: true) {
        name
        isDeprecated
        deprecationReason
      }
    }
    returnRequestInput: __type(name: "ReturnRequestInput") {
      inputFields(includeDeprecated: true) {
        name
        isDeprecated
        deprecationReason
      }
    }
    returnType: __type(name: "Return") {
      fields(includeDeprecated: true) {
        name
        type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
              }
            }
          }
        }
      }
    }
    returnShippingFee: __type(name: "ReturnShippingFee") {
      fields(includeDeprecated: true) {
        name
      }
    }
  }
`;

const returnReasonDefinitionsQuery = `#graphql
  query ReturnShippingFeeReturnReasonDefinitions {
    returnReasonDefinitions(first: 10) {
      nodes {
        id
        name
        handle
        deleted
      }
    }
  }
`;

const hiddenInputProbeMutation = `#graphql
  mutation ReturnCreateHiddenInputProbe($returnInput: ReturnInput!) {
    returnCreate(returnInput: $returnInput) {
      return {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation ReturnShippingFeeOrderCancel(
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

const orderDeleteMutation = `#graphql
  mutation ReturnShippingFeeOrderDelete($orderId: ID!) {
    orderDelete(orderId: $orderId) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const returnCreateMutation = await readRequest('return-create-shipping-fee-recorded.graphql');
const downstreamReadQuery = await readRequest('return-shipping-fee-read-recorded.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const orderVariables = {
  order: {
    email: `return-shipping-fee-${stamp}@example.com`,
    note: `return shipping fee capture ${stamp}`,
    tags: ['return-shipping-fee', stamp],
    test: true,
    currency: 'USD',
    lineItems: [
      {
        variantId: 'gid://shopify/ProductVariant/48540157378793',
        title: `Return shipping fee item ${stamp}`,
        quantity: 2,
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

const schema = await capture(schemaQuery);
const returnReasonDefinitions = await capture(returnReasonDefinitionsQuery);
const returnReasonDefinitionId = firstActiveReturnReasonDefinitionId(returnReasonDefinitions);
const orderCreate = await capture(orderCreateMutation, orderVariables);
requireEmptyUserErrors(orderCreate, 'orderCreate');

const createdOrder = readRecord(returnPayload(orderCreate, 'orderCreate')['order']) ?? {};
const orderId = requireString(createdOrder['id'], 'created order id');
const fulfillmentOrder = firstFulfillmentOrder(createdOrder);
const fulfillmentOrderId = requireString(fulfillmentOrder['id'], 'created fulfillment order id');
const fulfillmentOrderLineItem = readNodes(fulfillmentOrder['lineItems'])[0] ?? {};
const fulfillmentOrderLineItemId = requireString(
  fulfillmentOrderLineItem['id'],
  'created fulfillment order line item id',
);

const fulfillmentCreate = await capture(fulfillmentCreateMutation, {
  fulfillment: {
    notifyCustomer: false,
    trackingInfo: {
      number: `RETURN-SHIPPING-FULFILL-${stamp}`,
      url: `https://example.com/track/RETURN-SHIPPING-FULFILL-${stamp}`,
      company: 'Hermes Carrier',
    },
    lineItemsByFulfillmentOrder: [
      {
        fulfillmentOrderId,
        fulfillmentOrderLineItems: [
          {
            id: fulfillmentOrderLineItemId,
            quantity: 2,
          },
        ],
      },
    ],
  },
  message: `return shipping fee fulfillment ${stamp}`,
});
requireEmptyUserErrors(fulfillmentCreate, 'fulfillmentCreate');

const orderReadAfterFulfillment = await capture(orderReadQuery, { id: orderId });
const seedOrder = readRecord(readRecord(orderReadAfterFulfillment.response.payload)['data'])?.['order'];
const fulfillmentLineItem = firstFulfillmentLineItem(readRecord(seedOrder) ?? {});
const fulfillmentLineItemId = requireString(fulfillmentLineItem['id'], 'fulfilled fulfillment line item id');

const returnCreate = await capture(returnCreateMutation, {
  returnInput: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
        returnReasonDefinitionId,
      },
    ],
    returnShippingFee: {
      amount: {
        amount: '7.50',
        currencyCode: 'USD',
      },
    },
    unprocessed: true,
  },
});
requireEmptyUserErrors(returnCreate, 'returnCreate');

const createdReturn = readRecord(returnPayload(returnCreate, 'returnCreate')['return']) ?? {};
const returnId = requireString(createdReturn['id'], 'created return id');
const downstreamRead = await capture(downstreamReadQuery, {
  returnId,
  orderId,
});

const hiddenInputProbe = await capture(hiddenInputProbeMutation, {
  returnInput: {
    orderId,
    returnLineItems: [],
    returnDelivery: {
      trackingInfo: {
        number: `RETURN-SHIPPING-PROBE-${stamp}`,
      },
    },
    note: 'hidden input probe',
    refundIntent: 'RECOMMENDED',
    locationId: 'gid://shopify/Location/0',
    retailAttribution: {
      deviceId: 'probe-device',
    },
    unprocessed: true,
  },
});

const cleanupCancel = await capture(orderCancelMutation, {
  orderId,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: true,
});
const cleanupDelete = await capture(orderDeleteMutation, { orderId });

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  source: 'live-shopify-admin-graphql',
  notes:
    'Live public Admin GraphQL capture for returnCreate with ReturnInput.returnShippingFee and deprecated unprocessed. The same fixture records that returnDelivery, note, refundIntent, locationId, and retailAttribution are not public ReturnInput fields on this schema; hidden-field local behavior is therefore runtime-backed.',
  schema,
  setup: {
    returnReasonDefinitions,
    orderCreate,
    fulfillmentCreate,
    orderReadAfterFulfillment,
  },
  seedOrder,
  returnCreate,
  downstreamRead,
  hiddenInputProbe,
  cleanup: {
    orderCancel: cleanupCancel,
    orderDelete: cleanupDelete,
  },
  upstreamCalls: [
    {
      operationName: 'OrdersOrderHydrate',
      variables: { id: orderId },
      query: 'hand-synthesized from checked-in seedOrder for return shipping fee Pattern 2 order hydration',
      response: {
        status: 200,
        body: {
          data: {
            order: seedOrder,
          },
        },
      },
    },
  ],
});

await writeJson(specPath, {
  scenarioId: 'return-shipping-fee-recorded',
  operationNames: ['return', 'order', 'returnCreate'],
  scenarioStatus: 'captured',
  assertionKinds: ['lifecycle-transition-parity', 'downstream-read-parity', 'schema-introspection'],
  liveCaptureFiles: [fixturePath],
  proxyRequest: {
    documentPath: 'config/parity-requests/orders/return-create-shipping-fee-recorded.graphql',
    variablesCapturePath: '$.returnCreate.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  notes:
    'Live public Admin GraphQL evidence for returnCreate returnShippingFee and read-after-write Return.returnShippingFees. Public schema evidence in the fixture shows returnDelivery, note, refundIntent, locationId, and retailAttribution are rejected before resolver execution on this Admin API version.',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [
      {
        path: '$.return.id',
        matcher: 'shopify-gid:Return',
        reason: 'Shopify and the proxy allocate independent return IDs.',
      },
      {
        path: '$.return.returnShippingFees[*].id',
        matcher: 'shopify-gid:ReturnShippingFee',
        reason: 'Shopify and the proxy allocate independent return shipping fee IDs.',
      },
      {
        path: '$.return.returnLineItems.nodes[*].id',
        matcher: 'shopify-gid:ReturnLineItem',
        reason: 'Shopify and the proxy allocate independent return line item IDs.',
      },
      {
        path: '$.return.reverseFulfillmentOrders.nodes[*].id',
        matcher: 'shopify-gid:ReverseFulfillmentOrder',
        reason: 'Shopify and the proxy allocate independent reverse fulfillment order IDs.',
      },
      {
        path: '$.return.reverseFulfillmentOrders.nodes[*].lineItems.nodes[*].id',
        matcher: 'shopify-gid:ReverseFulfillmentOrderLineItem',
        reason: 'Shopify and the proxy allocate independent reverse fulfillment order line item IDs.',
      },
      {
        path: '$.return.id',
        matcher: 'shopify-gid:Return',
        reason: 'Downstream return(id:) reads use the proxy-created synthetic return ID.',
      },
      {
        path: '$.return.returnShippingFees[*].id',
        matcher: 'shopify-gid:ReturnShippingFee',
        reason: 'Downstream return(id:) reads use proxy-created synthetic return shipping fee IDs.',
      },
      {
        path: '$.return.returnLineItems.nodes[*].id',
        matcher: 'shopify-gid:ReturnLineItem',
        reason: 'Downstream return(id:) reads use proxy-created synthetic return line item IDs.',
      },
      {
        path: '$.order.returns.nodes[*].id',
        matcher: 'shopify-gid:Return',
        reason: 'Downstream order return lists use proxy-created synthetic return IDs.',
      },
      {
        path: '$.order.returns.nodes[*].returnShippingFees[*].id',
        matcher: 'shopify-gid:ReturnShippingFee',
        reason: 'Downstream order return lists use proxy-created synthetic return shipping fee IDs.',
      },
    ],
    targets: [
      {
        name: 'return-create-shipping-fee-payload',
        capturePath: '$.returnCreate.response.payload.data.returnCreate',
        proxyPath: '$.data.returnCreate',
      },
      {
        name: 'return-shipping-fee-downstream-read',
        capturePath: '$.downstreamRead.response.payload.data',
        proxyPath: '$.data',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/return-shipping-fee-read-recorded.graphql',
          variables: {
            returnId: {
              fromPrimaryProxyPath: '$.data.returnCreate.return.id',
            },
            orderId: {
              fromCapturePath: '$.returnCreate.variables.returnInput.orderId',
            },
          },
        },
      },
    ],
  },
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      specPath,
      orderId,
      returnId,
      cleanupCancelUserErrors: readArray(readRecord(returnPayload(cleanupCancel, 'orderCancel'))?.['userErrors']),
      cleanupDeleteUserErrors: readArray(readRecord(returnPayload(cleanupDelete, 'orderDelete'))?.['userErrors']),
    },
    null,
    2,
  ),
);
