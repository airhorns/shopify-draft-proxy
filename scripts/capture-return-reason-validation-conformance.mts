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
const fixturePath = path.join(fixtureDir, 'return-reason-validation.json');
const requestDir = path.join('config', 'parity-requests', 'orders');
const specPath = path.join('config', 'parity-specs', 'orders', 'return-reason-validation.json');

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

function requireValidationUserErrors(captureResult: GraphqlCapture, rootName: string, label: string): void {
  const payload = captureResult.response.payload as JsonRecord;
  const errors = payload['errors'];
  const root = readRecord(readRecord(payload['data'])?.[rootName]);
  const userErrors = readArray(root?.['userErrors']);
  if (errors || userErrors.length === 0 || root?.['return'] !== null) {
    throw new Error(`Expected ${label} to return only userErrors: ${JSON.stringify(captureResult.response.payload)}`);
  }
}

function firstFulfillmentOrder(order: JsonRecord): JsonRecord {
  return readNodes(order['fulfillmentOrders'])[0] ?? {};
}

function firstFulfillmentLineItem(order: JsonRecord): JsonRecord {
  return readNodes(readRecord(readArray(order['fulfillments'])[0])?.['fulfillmentLineItems'])[0] ?? {};
}

function otherReturnReasonDefinitionId(captureResult: GraphqlCapture): string {
  const data = readRecord(captureResult.response.payload as JsonRecord)?.['data'];
  const definitions = readNodes(readRecord(data)?.['returnReasonDefinitions']);
  const definition =
    definitions.find((node) => node['handle'] === 'other-reason' && node['deleted'] !== true) ??
    definitions.find((node) => node['name'] === 'Other' && node['deleted'] !== true);
  return requireString(definition?.['id'], 'other return reason definition id');
}

const orderFields = `#graphql
  fragment ReturnReasonValidationOrderFields on Order {
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
  mutation ReturnReasonValidationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...ReturnReasonValidationOrderFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  mutation ReturnReasonValidationFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
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
  query ReturnReasonValidationOrderRead($id: ID!) {
    order(id: $id) {
      ...ReturnReasonValidationOrderFields
    }
  }
`;

const schemaQuery = `#graphql
  query ReturnReasonValidationSchema {
    returnReason: __type(name: "ReturnReason") {
      kind
      name
      enumValues {
        name
      }
    }
    returnLineItemInput: __type(name: "ReturnLineItemInput") {
      inputFields(includeDeprecated: true) {
        name
        isDeprecated
        deprecationReason
        defaultValue
        type {
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
`;

const returnReasonDefinitionsQuery = `#graphql
  query ReturnReasonValidationDefinitions {
    returnReasonDefinitions(first: 20) {
      nodes {
        id
        name
        handle
        deleted
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation ReturnReasonValidationOrderCancel(
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
  mutation ReturnReasonValidationOrderDelete($orderId: ID!) {
    orderDelete(orderId: $orderId) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const returnCreateMutation = await readRequest('return-create-reason-validation.graphql');
const returnRequestMutation = await readRequest('return-request-reason-validation.graphql');
const returnOrderHydrateQuery = await readRequest('return-order-hydrate.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const orderVariables = {
  order: {
    email: `return-reason-validation-${stamp}@example.com`,
    note: `return reason validation capture ${stamp}`,
    tags: ['return-reason-validation', stamp],
    test: true,
    currency: 'USD',
    lineItems: [
      {
        variantId: 'gid://shopify/ProductVariant/48540157378793',
        title: `Return reason validation item ${stamp}`,
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
const otherReasonDefinitionId = otherReturnReasonDefinitionId(returnReasonDefinitions);
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
      number: `RETURN-REASON-FULFILL-${stamp}`,
      url: `https://example.com/track/RETURN-REASON-FULFILL-${stamp}`,
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
  message: `return reason validation fulfillment ${stamp}`,
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

const missingReasonCreate = await capture(returnCreateMutation, {
  returnInput: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
      },
    ],
  },
});
requireValidationUserErrors(missingReasonCreate, 'returnCreate', 'missing returnCreate reason');

const missingReasonRequest = await capture(returnRequestMutation, {
  input: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
      },
    ],
  },
});
requireValidationUserErrors(missingReasonRequest, 'returnRequest', 'missing returnRequest reason');

const otherBlankNoteCreate = await capture(returnCreateMutation, {
  returnInput: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
        returnReason: 'OTHER',
      },
    ],
  },
});
requireValidationUserErrors(otherBlankNoteCreate, 'returnCreate', 'returnCreate OTHER without note');

const otherBlankNoteRequest = await capture(returnRequestMutation, {
  input: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
        returnReason: 'OTHER',
      },
    ],
  },
});
requireEmptyUserErrors(otherBlankNoteRequest, 'returnRequest');

const otherDefinitionNoNoteRequest = await capture(returnRequestMutation, {
  input: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
        returnReasonDefinitionId: otherReasonDefinitionId,
      },
    ],
  },
});
requireEmptyUserErrors(otherDefinitionNoNoteRequest, 'returnRequest');

const invalidReasonCreate = await capture(returnCreateMutation, {
  returnInput: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
        returnReason: 'NOT_A_REASON',
      },
    ],
  },
});

const invalidReasonRequest = await capture(returnRequestMutation, {
  input: {
    orderId,
    returnLineItems: [
      {
        fulfillmentLineItemId,
        quantity: 1,
        returnReason: 'NOT_A_REASON',
      },
    ],
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
    'Live public Admin GraphQL capture for returnCreate/returnRequest return line item returnReason validation against a fulfilled order. Missing legacy reason without reasonDefinitionId returns NOT_FOUND/BLANK by root, explicit legacy OTHER without a note returns BLANK for returnCreate, returnRequest accepts explicit OTHER without a note, the public enum boundary for invalid returnReason is recorded for this Admin API version, and the public other-reason returnReasonDefinitionId is accepted without a note on this shop/API version.',
  schema,
  setup: {
    returnReasonDefinitions,
    orderCreate,
    fulfillmentCreate,
    orderReadAfterFulfillment,
  },
  missingReasonCreate,
  missingReasonRequest,
  otherBlankNoteCreate,
  otherBlankNoteRequest,
  otherDefinitionNoNoteRequest,
  invalidReasonCreate,
  invalidReasonRequest,
  cleanup: {
    orderCancel: cleanupCancel,
    orderDelete: cleanupDelete,
  },
  expected: {
    emptyObject: {},
    emptyArray: [],
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
  scenarioId: 'return-reason-validation',
  operationNames: ['returnCreate', 'returnRequest'],
  scenarioStatus: 'captured',
  assertionKinds: ['user-errors-parity', 'validation-side-effects', 'schema-introspection'],
  liveCaptureFiles: [fixturePath],
  comparisonMode: 'captured-vs-proxy-request',
  notes:
    'Live public Admin GraphQL evidence for returnCreate/returnRequest return line item returnReason validation. Invalid validation targets run against isolated proxy state and assert the payload plus no staged returns, no returnsByOrder index rows, no order hydration, and no mutation-log entries. returnRequest explicit OTHER and public other-reason returnReasonDefinitionId are captured as valid no-note inputs because Shopify accepts them on this shop/API version.',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'return-create-missing-reason-user-error',
        capturePath: '$.missingReasonCreate.response.payload.data.returnCreate',
        proxyPath: '$.data.returnCreate',
        isolatedProxy: true,
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/return-create-reason-validation.graphql',
          variablesCapturePath: '$.missingReasonCreate.variables',
          apiVersion,
        },
      },
      {
        name: 'return-create-missing-reason-no-staged-returns',
        capturePath: '$.expected.emptyObject',
        proxyStatePath: '$.stagedState.returns',
        preserveProxyState: true,
      },
      {
        name: 'return-create-missing-reason-no-return-order-index',
        capturePath: '$.expected.emptyObject',
        proxyStatePath: '$.stagedState.returnsByOrder',
        preserveProxyState: true,
      },
      {
        name: 'return-create-missing-reason-no-order-hydration',
        capturePath: '$.expected.emptyObject',
        proxyStatePath: '$.stagedState.orders',
        preserveProxyState: true,
      },
      {
        name: 'return-create-missing-reason-no-mutation-log',
        capturePath: '$.expected.emptyArray',
        proxyLogPath: '$.entries',
        preserveProxyState: true,
      },
      {
        name: 'return-request-missing-reason-user-error',
        capturePath: '$.missingReasonRequest.response.payload.data.returnRequest',
        proxyPath: '$.data.returnRequest',
        isolatedProxy: true,
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/return-request-reason-validation.graphql',
          variablesCapturePath: '$.missingReasonRequest.variables',
          apiVersion,
        },
      },
      {
        name: 'return-request-missing-reason-no-staged-returns',
        capturePath: '$.expected.emptyObject',
        proxyStatePath: '$.stagedState.returns',
        preserveProxyState: true,
      },
      {
        name: 'return-create-other-note-user-error',
        capturePath: '$.otherBlankNoteCreate.response.payload.data.returnCreate',
        proxyPath: '$.data.returnCreate',
        isolatedProxy: true,
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/return-create-reason-validation.graphql',
          variablesCapturePath: '$.otherBlankNoteCreate.variables',
          apiVersion,
        },
      },
      {
        name: 'return-create-other-note-no-staged-returns',
        capturePath: '$.expected.emptyObject',
        proxyStatePath: '$.stagedState.returns',
        preserveProxyState: true,
      },
      {
        name: 'return-request-other-no-note-stages',
        capturePath: '$.otherBlankNoteRequest.response.payload.data.returnRequest',
        proxyPath: '$.data.returnRequest',
        isolatedProxy: true,
        expectedDifferences: [
          {
            path: '$.return.id',
            matcher: 'shopify-gid:Return',
            reason:
              'Shopify and the proxy allocate independent return IDs for the accepted returnRequest OTHER request.',
          },
        ],
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/return-request-reason-validation.graphql',
          variablesCapturePath: '$.otherBlankNoteRequest.variables',
          apiVersion,
        },
      },
      {
        name: 'return-request-other-definition-no-note-stages',
        capturePath: '$.otherDefinitionNoNoteRequest.response.payload.data.returnRequest',
        proxyPath: '$.data.returnRequest',
        isolatedProxy: true,
        expectedDifferences: [
          {
            path: '$.return.id',
            matcher: 'shopify-gid:Return',
            reason: 'Shopify and the proxy allocate independent return IDs for the accepted reason-definition request.',
          },
        ],
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/return-request-reason-validation.graphql',
          variablesCapturePath: '$.otherDefinitionNoNoteRequest.variables',
          apiVersion,
        },
      },
      {
        name: 'return-create-invalid-reason-response',
        capturePath: '$.invalidReasonCreate.response.payload',
        proxyPath: '$',
        isolatedProxy: true,
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/return-create-reason-validation.graphql',
          variablesCapturePath: '$.invalidReasonCreate.variables',
          apiVersion,
        },
      },
      {
        name: 'return-create-invalid-reason-no-staged-returns',
        capturePath: '$.expected.emptyObject',
        proxyStatePath: '$.stagedState.returns',
        preserveProxyState: true,
      },
      {
        name: 'return-request-invalid-reason-response',
        capturePath: '$.invalidReasonRequest.response.payload',
        proxyPath: '$',
        isolatedProxy: true,
        proxyRequest: {
          documentPath: 'config/parity-requests/orders/return-request-reason-validation.graphql',
          variablesCapturePath: '$.invalidReasonRequest.variables',
          apiVersion,
        },
      },
      {
        name: 'return-request-invalid-reason-no-staged-returns',
        capturePath: '$.expected.emptyObject',
        proxyStatePath: '$.stagedState.returns',
        preserveProxyState: true,
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
      otherReasonDefinitionId,
      cleanupCancelUserErrors: readArray(readRecord(returnPayload(cleanupCancel, 'orderCancel'))?.['userErrors']),
      cleanupDeleteUserErrors: readArray(readRecord(returnPayload(cleanupDelete, 'orderDelete'))?.['userErrors']),
      otherDefinitionNoNotePayload: otherDefinitionNoNoteRequest.response.payload,
      invalidReasonCreatePayload: invalidReasonCreate.response.payload,
      invalidReasonRequestPayload: invalidReasonRequest.response.payload,
    },
    null,
    2,
  ),
);
