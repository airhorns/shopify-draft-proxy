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

const scenarioId = 'orderEdit-shipping-line-validation';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

if (apiVersion !== '2026-04') {
  throw new Error(`${scenarioId} requires SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixturePath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders', `${scenarioId}.json`);
const requestDir = path.join('config', 'parity-requests', 'orders');

const orderFields = `#graphql
  fragment OrderEditShippingLineValidationOrderFields on Order {
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
    paymentGatewayNames
    note
    tags
    customAttributes {
      key
      value
    }
    customer {
      id
      email
      displayName
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
    shippingLines(first: 10) {
      nodes {
        id
        title
        code
        source
        originalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
          presentmentMoney {
            amount
            currencyCode
          }
        }
        discountedPriceSet {
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
  }
`;

const orderCreateMutation = `#graphql
  ${orderFields}
  mutation OrderEditShippingLineValidationCreate(
    $order: OrderCreateOrderInput!
    $options: OrderCreateOptionsInput
  ) {
    orderCreate(order: $order, options: $options) {
      order {
        ...OrderEditShippingLineValidationOrderFields
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation OrderEditShippingLineValidationCancel(
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

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

function stripGraphqlTag(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord = {}): Promise<GraphqlCapture> {
  return {
    query: stripGraphqlTag(query),
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

function orderFromCreate(captureResult: GraphqlCapture): JsonRecord {
  const order = readRecord(mutationPayload(captureResult, 'orderCreate')['order']);
  if (!order) {
    throw new Error(`Expected orderCreate.order: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
  return order;
}

function calculatedOrderId(captureResult: GraphqlCapture): string {
  const calculatedOrder = readRecord(mutationPayload(captureResult, 'orderEditBegin')['calculatedOrder']);
  return requireString(calculatedOrder?.['id'], 'calculated order id');
}

function firstCalculatedLineItemId(captureResult: GraphqlCapture): string {
  const calculatedOrder = readRecord(mutationPayload(captureResult, 'orderEditBegin')['calculatedOrder']);
  const lineItems = readRecord(calculatedOrder?.['lineItems']);
  const nodes = readArray(lineItems?.['nodes']);
  const firstNode = readRecord(nodes[0]);
  return requireString(firstNode?.['id'], 'calculated line item id');
}

function assertNoTopLevelErrors(label: string, captureResult: GraphqlCapture): void {
  if (captureResult.response.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
}

function assertEmptyUserErrors(label: string, captureResult: GraphqlCapture, rootName: string): void {
  const errors = readArray(mutationPayload(captureResult, rootName)['userErrors']);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertHasTopLevelErrors(label: string, captureResult: GraphqlCapture): void {
  if (readArray(captureResult.response.payload.errors).length === 0) {
    throw new Error(`${label} expected top-level errors: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
}

function assertHasUserErrors(label: string, captureResult: GraphqlCapture, rootName: string): void {
  assertNoTopLevelErrors(label, captureResult);
  const errors = readArray(mutationPayload(captureResult, rootName)['userErrors']);
  if (errors.length === 0) {
    throw new Error(`${label} expected userErrors: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
}

function orderCreateVariables(stamp: string): JsonRecord {
  return {
    order: {
      email: `order-edit-shipping-line-validation-${stamp}@example.com`,
      note: `orderEdit shipping line validation capture ${stamp}`,
      tags: ['order-edit-shipping-line-validation', stamp],
      test: true,
      currency: 'CAD',
      shippingAddress: {
        firstName: 'Conformance',
        lastName: 'ShippingLine',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          title: `Order edit source item ${stamp}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: true,
          taxable: true,
          sku: `order-edit-shipping-line-${stamp}`,
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

// The exact order hydrate query the proxy forwards on a cold order-edit begin, byte-identical to
// ORDER_EDIT_HYDRATE_QUERY (`include_str!` of order-edit-hydrate.graphql) so the recorded cassette
// replays verbatim. The de-seeded begin resolves the precondition order this way instead of a seed.
const orderEditHydrateQuery = await readRequest('order-edit-hydrate.graphql');
const beginDocument = await readRequest('orderEdit-shipping-line-validation-begin.graphql');
const addMissingPriceDocument = await readRequest('orderEdit-shipping-line-validation-add-missing-price.graphql');
const addDocument = await readRequest('orderEdit-shipping-line-validation-add.graphql');
const updateDocument = await readRequest('orderEdit-shipping-line-validation-update.graphql');
const removeDocument = await readRequest('orderEdit-shipping-line-validation-remove.graphql');
const discountMissingCurrencyDocument = await readRequest(
  'orderEdit-shipping-line-validation-discount-missing-currency.graphql',
);

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

let createdOrderId: string | null = null;
let cleanup: GraphqlCapture | null = null;

try {
  const orderCreate = await capture(orderCreateMutation, orderCreateVariables(stamp));
  assertNoTopLevelErrors('orderCreate setup', orderCreate);
  assertEmptyUserErrors('orderCreate setup', orderCreate, 'orderCreate');
  const createdOrder = orderFromCreate(orderCreate);
  createdOrderId = requireString(createdOrder['id'], 'created order id');

  // Resolve the precondition the de-seeded way: forward the exact cold ORDER_EDIT_HYDRATE_QUERY the
  // proxy emits on begin and record its response verbatim as the single upstreamCall. No seed/setup.
  const orderEditHydrate = await capture(orderEditHydrateQuery, { id: createdOrderId });
  assertNoTopLevelErrors('order-edit hydrate', orderEditHydrate);
  if (!readRecord(responseData(orderEditHydrate)['order'])) {
    throw new Error(`Expected order-edit hydrate read: ${JSON.stringify(orderEditHydrate.response.payload, null, 2)}`);
  }

  const begin = await capture(beginDocument, { id: createdOrderId });
  assertNoTopLevelErrors('orderEditBegin', begin);
  assertEmptyUserErrors('orderEditBegin', begin, 'orderEditBegin');
  const calculatedOrderIdValue = calculatedOrderId(begin);
  const lineItemId = firstCalculatedLineItemId(begin);
  const unknownShippingLineId = 'gid://shopify/CalculatedShippingLine/999999999999';

  const addMissingPrice = await capture(addMissingPriceDocument, { id: calculatedOrderIdValue });
  assertHasTopLevelErrors('add shipping missing price', addMissingPrice);

  const addCurrencyMismatch = await capture(addDocument, {
    id: calculatedOrderIdValue,
    shippingLine: {
      title: 'Currency mismatch shipping',
      price: {
        amount: '5.00',
        currencyCode: 'USD',
      },
    },
  });
  assertHasUserErrors('add shipping currency mismatch', addCurrencyMismatch, 'orderEditAddShippingLine');

  const updateUnknown = await capture(updateDocument, {
    id: calculatedOrderIdValue,
    shippingLineId: unknownShippingLineId,
    shippingLine: {
      title: 'Unknown update',
      price: {
        amount: '6.00',
        currencyCode: 'CAD',
      },
    },
  });
  assertHasUserErrors('update unknown shipping line', updateUnknown, 'orderEditUpdateShippingLine');

  const removeUnknown = await capture(removeDocument, {
    id: calculatedOrderIdValue,
    shippingLineId: unknownShippingLineId,
  });
  assertHasUserErrors('remove unknown shipping line', removeUnknown, 'orderEditRemoveShippingLine');

  const discountMissingCurrency = await capture(discountMissingCurrencyDocument, {
    id: calculatedOrderIdValue,
    lineItemId,
  });
  assertHasTopLevelErrors('discount missing currency', discountMissingCurrency);

  cleanup = await capture(orderCancelMutation, {
    orderId: createdOrderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });

  await writeJson(fixturePath, {
    capturedAt: new Date().toISOString(),
    scenarioId,
    apiVersion,
    storeDomain,
    source: 'live-shopify-admin-graphql',
    notes:
      'Live orderEdit shipping-line validation capture against one disposable CAD order-edit session. The precondition order is resolved via a real cold OrdersOrderEditHydrate forward (single upstreamCall) rather than a seed/setup block. Invalid branches do not stage shipping lines or discounts; the source order is cancelled in cleanup.',
    begin,
    cases: {
      addMissingPrice,
      addCurrencyMismatch,
      updateUnknown,
      removeUnknown,
      discountMissingCurrency,
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'OrdersOrderEditHydrate',
        variables: { id: createdOrderId },
        query: orderEditHydrateQuery,
        response: {
          status: orderEditHydrate.response.status,
          body: orderEditHydrate.response.payload,
        },
      },
    ],
  });

  console.log(JSON.stringify({ fixturePath, orderId: createdOrderId }, null, 2));
} finally {
  if (createdOrderId && cleanup === null) {
    try {
      await capture(orderCancelMutation, {
        orderId: createdOrderId,
        reason: 'OTHER',
        notifyCustomer: false,
        restock: true,
      });
    } catch (error) {
      console.error(`Cleanup failed for ${createdOrderId}: ${(error as Error).message}`);
    }
  }
}
