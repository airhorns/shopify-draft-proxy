/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
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
const fixturePath = path.join(fixtureDir, 'return-customer-note-recorded.json');
const requestDir = path.join('config', 'parity-requests', 'orders');
const specPath = path.join('config', 'parity-specs', 'orders', 'return-request-customer-note-recorded.json');

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

const orderFields = `#graphql
  fragment ReturnCustomerNoteOrderFields on Order {
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

const schemaQuery = `#graphql
  query ReturnCustomerNoteSchemaEvidence {
    returnRequestLineItemInput: __type(name: "ReturnRequestLineItemInput") {
      inputFields(includeDeprecated: true) {
        name
        isDeprecated
        deprecationReason
      }
    }
    returnLineItem: __type(name: "ReturnLineItem") {
      fields(includeDeprecated: true) {
        name
        isDeprecated
        deprecationReason
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  ${orderFields}
  mutation ReturnCustomerNoteOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...ReturnCustomerNoteOrderFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation ReturnCustomerNoteFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
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
  query ReturnCustomerNoteOrderRead($id: ID!) {
    order(id: $id) {
      ...ReturnCustomerNoteOrderFields
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation ReturnCustomerNoteOrderCancel(
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
  mutation ReturnCustomerNoteOrderDelete($orderId: ID!) {
    orderDelete(orderId: $orderId) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const returnRequestMutation = await readRequest('return-request-customer-note-recorded.graphql');
const downstreamReadQuery = await readRequest('return-customer-note-read-recorded.graphql');
const returnOrderHydrateQuery = await readRequest('return-order-hydrate.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const customerNote = 'Screen arrived cracked';
const orderVariables = {
  order: {
    email: `return-customer-note-${stamp}@example.com`,
    note: `return customer note capture ${stamp}`,
    tags: ['return-customer-note', stamp],
    test: true,
    currency: 'USD',
    lineItems: [
      {
        title: `Return customer note item ${stamp}`,
        quantity: 2,
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

const schema = await capture(schemaQuery);
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
      number: `RETURN-CUSTOMER-NOTE-FULFILL-${stamp}`,
      url: `https://example.com/track/RETURN-CUSTOMER-NOTE-FULFILL-${stamp}`,
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
  message: `return customer note fulfillment ${stamp}`,
});
requireEmptyUserErrors(fulfillmentCreate, 'fulfillmentCreate');

const orderReadAfterFulfillment = await capture(orderReadQuery, { id: orderId });
const orderAfterFulfillment = readRecord(readRecord(orderReadAfterFulfillment.response.payload)['data'])?.['order'];
const fulfillmentLineItem = firstFulfillmentLineItem(readRecord(orderAfterFulfillment) ?? {});
const fulfillmentLineItemId = requireString(fulfillmentLineItem['id'], 'fulfilled fulfillment line item id');

const returnOrderHydrate = await runGraphqlRequest(returnOrderHydrateQuery, { id: orderId });
if (returnOrderHydrate.payload['errors']) {
  throw new Error(`return-order hydrate returned errors: ${JSON.stringify(returnOrderHydrate.payload)}`);
}

const returnRequest = await capture(returnRequestMutation, {
  input: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
        returnReason: 'DEFECTIVE',
        customerNote,
      },
    ],
  },
});
requireEmptyUserErrors(returnRequest, 'returnRequest');

const requestedReturn = readRecord(returnPayload(returnRequest, 'returnRequest')['return']) ?? {};
const returnId = requireString(requestedReturn['id'], 'requested return id');
const returnLineItem = readNodes(requestedReturn['returnLineItems'])[0] ?? {};
if (returnLineItem['customerNote'] !== customerNote) {
  throw new Error(`returnRequest did not echo customerNote: ${JSON.stringify(requestedReturn)}`);
}

const downstreamRead = await capture(downstreamReadQuery, {
  returnId,
});
const downstreamReturn = readRecord(readRecord(downstreamRead.response.payload as JsonRecord)['data'])?.['return'];
const downstreamReturnLineItem = readNodes(readRecord(downstreamReturn)?.['returnLineItems'])[0] ?? {};
if (downstreamReturnLineItem['customerNote'] !== customerNote) {
  throw new Error(`return(id:) did not echo customerNote: ${JSON.stringify(downstreamRead.response.payload)}`);
}

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
    'Live public Admin GraphQL capture for returnRequest with ReturnRequestLineItemInput.customerNote. The mutation payload and follow-up return(id:) read both echo the per-line customerNote.',
  schema,
  setup: {
    orderCreate,
    fulfillmentCreate,
    orderReadAfterFulfillment,
  },
  returnRequest,
  downstreamRead,
  cleanup: {
    orderCancel: cleanupCancel,
    orderDelete: cleanupDelete,
  },
  upstreamCalls: [
    {
      operationName: 'OrdersReturnOrderHydrate',
      variables: { id: orderId },
      query: returnOrderHydrateQuery,
      response: {
        status: returnOrderHydrate.status,
        body: returnOrderHydrate.payload,
      },
    },
  ],
});

await writeJson(specPath, {
  scenarioId: 'return-request-customer-note-recorded',
  operationNames: ['return', 'returnRequest'],
  scenarioStatus: 'captured',
  assertionKinds: ['lifecycle-transition-parity', 'downstream-read-parity', 'schema-introspection'],
  liveCaptureFiles: [fixturePath],
  proxyRequest: {
    documentPath: 'config/parity-requests/orders/return-request-customer-note-recorded.graphql',
    variablesCapturePath: '$.returnRequest.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  notes:
    'Live public Admin GraphQL evidence that ReturnRequestLineItemInput.customerNote is accepted and echoed on ReturnLineItem.customerNote in the mutation payload and a follow-up return(id:) read.',
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
    ],
    targets: [
      {
        name: 'return-request-customer-note-payload',
        capturePath: '$.returnRequest.response.payload.data.returnRequest',
        proxyPath: '$.data.returnRequest',
      },
      {
        name: 'return-customer-note-downstream-read',
        capturePath: '$.downstreamRead.response.payload.data',
        proxyPath: '$.data',
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/return-customer-note-read-recorded.graphql',
          variables: {
            returnId: {
              fromPrimaryProxyPath: '$.data.returnRequest.return.id',
            },
          },
        },
      },
    ],
  },
});

formatGeneratedJson([fixturePath, specPath]);

console.log(
  JSON.stringify(
    {
      fixturePath,
      specPath,
      orderId,
      returnId,
      customerNote,
      cleanupCancelUserErrors: readArray(readRecord(returnPayload(cleanupCancel, 'orderCancel'))?.['userErrors']),
      cleanupDeleteUserErrors: readArray(readRecord(returnPayload(cleanupDelete, 'orderDelete'))?.['userErrors']),
    },
    null,
    2,
  ),
);
