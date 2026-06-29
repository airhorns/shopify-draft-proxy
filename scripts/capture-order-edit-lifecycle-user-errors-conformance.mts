/* oxlint-disable no-console -- CLI script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type CaptureCase = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

const scenarioId = 'orderEdit-lifecycle-userErrors';
const requestDir = path.join('config', 'parity-requests', 'orders');
const paritySpecPath = path.join('config', 'parity-specs', 'orders', 'orderEdit-lifecycle-userErrors.json');
const unknownCalculatedOrderId = 'gid://shopify/CalculatedOrder/999999999999999';
const unknownCalculatedLineItemId = 'gid://shopify/CalculatedLineItem/999999999999999';
const unknownVariantId = 'gid://shopify/ProductVariant/999999999999999';
const missingOrderId = 'gid://shopify/Order/0';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
if (apiVersion !== '2026-04') {
  throw new Error(`${scenarioId} capture requires SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`);
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

function cleanDocument(document: string): string {
  return document.replace(/^#graphql\n/u, '').trim();
}

async function capture(query: string, variables: JsonRecord = {}): Promise<CaptureCase> {
  return {
    query: cleanDocument(query),
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

function responseData(captureResult: CaptureCase): JsonRecord {
  const data = readRecord(captureResult.response.payload.data);
  if (!data) {
    throw new Error(`Expected GraphQL data for capture: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
  return data;
}

function mutationPayload(captureResult: CaptureCase, rootName: string): JsonRecord {
  const root = readRecord(responseData(captureResult)[rootName]);
  if (!root) {
    throw new Error(`Expected ${rootName} payload: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
  return root;
}

function assertNoTopLevelErrors(label: string, captureResult: CaptureCase): void {
  if (captureResult.response.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
}

function assertEmptyUserErrors(label: string, captureResult: CaptureCase, rootName: string): void {
  assertNoTopLevelErrors(label, captureResult);
  const errors = readArray(mutationPayload(captureResult, rootName)['userErrors']);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertUserErrorField(label: string, captureResult: CaptureCase, rootName: string, field: string[]): void {
  assertNoTopLevelErrors(label, captureResult);
  const errors = readArray(mutationPayload(captureResult, rootName)['userErrors']);
  const first = readRecord(errors[0]);
  const actualField = readArray(first?.['field']);
  if (errors.length !== 1 || JSON.stringify(actualField) !== JSON.stringify(field)) {
    throw new Error(
      `${label} expected one ${JSON.stringify(field)} userError: ${JSON.stringify(
        captureResult.response.payload,
        null,
        2,
      )}`,
    );
  }
}

function orderPayload(captureResult: CaptureCase): JsonRecord {
  const order = readRecord(mutationPayload(captureResult, 'orderCreate')['order']);
  if (!order) {
    throw new Error(`Expected orderCreate.order payload: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
  return order;
}

function calculatedOrderId(captureResult: CaptureCase): string {
  const calculatedOrder = readRecord(mutationPayload(captureResult, 'orderEditBegin')['calculatedOrder']);
  return requireString(calculatedOrder?.['id'], 'calculated order id');
}

function orderHydrateCassette(orderId: string, hydrate: CaptureCase): JsonRecord {
  return {
    operationName: 'OrdersOrderEditHydrate',
    variables: { id: orderId },
    query: orderEditHydrateQuery,
    response: {
      status: hydrate.response.status,
      body: hydrate.response.payload,
    },
  };
}

function variantHydrateCassette(variantId: string, hydrate: CaptureCase): JsonRecord {
  return {
    operationName: 'OrdersDraftOrderVariantHydrate',
    variables: { id: variantId },
    query: variantHydrateQuery,
    response: {
      status: hydrate.response.status,
      body: hydrate.response.payload,
    },
  };
}

function orderCreateVariables(stamp: string, label: string, currency: string): JsonRecord {
  return {
    order: {
      email: `order-edit-lifecycle-${label}-${stamp}@example.com`,
      note: `orderEdit lifecycle userErrors ${label} ${stamp}`,
      tags: ['order-edit-lifecycle-user-errors', label, stamp],
      test: true,
      currency,
      lineItems: [
        {
          title: `Order edit lifecycle ${label} line ${stamp}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: currency,
            },
          },
          requiresShipping: false,
          taxable: false,
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

function orderIsNotEditable(order: JsonRecord | null): boolean {
  if (!order) return false;
  return (
    order['merchantEditable'] === false ||
    typeof order['cancelledAt'] === 'string' ||
    typeof order['cancelReason'] === 'string' ||
    order['displayFinancialStatus'] === 'REFUNDED' ||
    order['displayFinancialStatus'] === 'VOIDED'
  );
}

async function waitForNotEditableHydrate(orderId: string): Promise<CaptureCase> {
  let lastHydrate: CaptureCase | null = null;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    if (attempt > 0) await sleep(1000);
    lastHydrate = await capture(orderEditHydrateQuery, { id: orderId });
    assertNoTopLevelErrors('not-editable order hydrate', lastHydrate);
    const order = readRecord(responseData(lastHydrate)['order']);
    if (orderIsNotEditable(order)) return lastHydrate;
  }
  throw new Error(
    `Expected hydrated order to be non-editable: ${JSON.stringify(lastHydrate?.response.payload, null, 2)}`,
  );
}

function cleanupVariables(orderId: string): JsonRecord {
  return {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  };
}

const beginDocument = await readRequest('orderEdit-lifecycle-userErrors-begin.graphql');
const addVariantDocument = await readRequest('orderEdit-lifecycle-userErrors-addVariant.graphql');
const setQuantityDocument = await readRequest('orderEdit-lifecycle-userErrors-setQuantity.graphql');
const commitDocument = await readRequest('orderEdit-lifecycle-userErrors-commit.graphql');
const addLineItemDiscountDocument = await readRequest(
  'orderEdit-shipping-line-validation-add-line-item-discount.graphql',
);
const orderEditHydrateQuery = await readRequest('order-edit-hydrate.graphql');
const variantHydrateQuery =
  'query OrdersDraftOrderVariantHydrate($id: ID!) {\n  productVariant(id: $id) { id title sku taxable price inventoryItem { requiresShipping } product { title } }\n}\n';

const shopCurrencyQuery = `#graphql
  query OrderEditLifecycleUserErrorsShopCurrency {
    shop {
      currencyCode
    }
  }
`;
const orderCreateMutation = `#graphql
  mutation OrderEditLifecycleUserErrorsCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
      }
      userErrors {
        field
        message
      }
    }
  }
`;
const orderCancelMutation = `#graphql
  mutation OrderEditLifecycleUserErrorsCancel(
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
const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'orders',
  'order-edit-lifecycle-user-errors.json',
);
const createdOrderIds = new Set<string>();
const cleanupCaptures: JsonRecord = {};

try {
  const shopCurrency = requireString(
    readRecord(responseData(await capture(shopCurrencyQuery))['shop'])?.['currencyCode'],
    'shop currencyCode',
  );

  const missingOrderHydrate = await capture(orderEditHydrateQuery, { id: missingOrderId });
  assertNoTopLevelErrors('missing order hydrate', missingOrderHydrate);
  const beginNotFound = await capture(beginDocument, { id: missingOrderId });
  assertUserErrorField('begin not found', beginNotFound, 'orderEditBegin', ['id']);

  const addVariantMissingCalculatedOrder = await capture(addVariantDocument, {
    id: unknownCalculatedOrderId,
    variantId: unknownVariantId,
    quantity: 1,
  });
  assertUserErrorField('addVariant missing calculated order', addVariantMissingCalculatedOrder, 'orderEditAddVariant', [
    'id',
  ]);

  const setQuantityMissingCalculatedOrder = await capture(setQuantityDocument, {
    id: unknownCalculatedOrderId,
    lineItemId: unknownCalculatedLineItemId,
    quantity: 1,
  });
  assertUserErrorField(
    'setQuantity missing calculated order',
    setQuantityMissingCalculatedOrder,
    'orderEditSetQuantity',
    ['id'],
  );

  const commitMissingCalculatedOrder = await capture(commitDocument, { id: unknownCalculatedOrderId });
  assertUserErrorField('commit missing calculated order', commitMissingCalculatedOrder, 'orderEditCommit', ['id']);

  const editableCreate = await capture(orderCreateMutation, orderCreateVariables(stamp, 'editable', shopCurrency));
  assertEmptyUserErrors('editable orderCreate', editableCreate, 'orderCreate');
  const editableOrderId = requireString(orderPayload(editableCreate)['id'], 'editable order id');
  createdOrderIds.add(editableOrderId);

  const editableHydrate = await capture(orderEditHydrateQuery, { id: editableOrderId });
  assertNoTopLevelErrors('editable order hydrate', editableHydrate);
  if (!readRecord(responseData(editableHydrate)['order'])) {
    throw new Error(`Expected editable order hydrate: ${JSON.stringify(editableHydrate.response.payload, null, 2)}`);
  }

  const editableBegin = await capture(beginDocument, { id: editableOrderId });
  assertEmptyUserErrors('editable orderEditBegin', editableBegin, 'orderEditBegin');
  const editableCalculatedOrderId = calculatedOrderId(editableBegin);

  const setQuantityUnknownLine = await capture(setQuantityDocument, {
    id: editableCalculatedOrderId,
    lineItemId: unknownCalculatedLineItemId,
    quantity: 1,
  });
  assertUserErrorField('setQuantity unknown line item', setQuantityUnknownLine, 'orderEditSetQuantity', ['lineItemId']);

  const addLineItemDiscountUnknownLine = await capture(addLineItemDiscountDocument, {
    id: editableCalculatedOrderId,
    lineItemId: unknownCalculatedLineItemId,
    discount: {
      description: 'Unknown line discount',
      fixedValue: {
        amount: '1.00',
        currencyCode: shopCurrency,
      },
    },
  });
  assertUserErrorField(
    'addLineItemDiscount unknown line item',
    addLineItemDiscountUnknownLine,
    'orderEditAddLineItemDiscount',
    ['id'],
  );

  const unknownVariantHydrate = await capture(variantHydrateQuery, { id: unknownVariantId });
  assertNoTopLevelErrors('unknown variant hydrate', unknownVariantHydrate);

  const addVariantUnknownVariant = await capture(addVariantDocument, {
    id: editableCalculatedOrderId,
    variantId: unknownVariantId,
    quantity: 1,
  });
  assertUserErrorField('addVariant unknown variant', addVariantUnknownVariant, 'orderEditAddVariant', ['variantId']);

  const notEditableCreate = await capture(
    orderCreateMutation,
    orderCreateVariables(stamp, 'not-editable', shopCurrency),
  );
  assertEmptyUserErrors('not-editable orderCreate', notEditableCreate, 'orderCreate');
  const notEditableOrderId = requireString(orderPayload(notEditableCreate)['id'], 'not-editable order id');
  createdOrderIds.add(notEditableOrderId);

  const notEditableCancel = await capture(orderCancelMutation, cleanupVariables(notEditableOrderId));
  assertEmptyUserErrors('not-editable orderCancel', notEditableCancel, 'orderCancel');
  const notEditableHydrate = await waitForNotEditableHydrate(notEditableOrderId);
  createdOrderIds.delete(notEditableOrderId);

  const beginNotEditable = await capture(beginDocument, { id: notEditableOrderId });
  assertUserErrorField('begin not editable', beginNotEditable, 'orderEditBegin', []);

  const editableCleanup = await capture(orderCancelMutation, cleanupVariables(editableOrderId));
  cleanupCaptures['editableOrderCancel'] = editableCleanup;
  createdOrderIds.delete(editableOrderId);

  const upstreamCalls = [
    orderHydrateCassette(missingOrderId, missingOrderHydrate),
    orderHydrateCassette(editableOrderId, editableHydrate),
    variantHydrateCassette(unknownVariantId, unknownVariantHydrate),
    orderHydrateCassette(notEditableOrderId, notEditableHydrate),
  ];

  await writeJson(fixturePath, {
    scenarioId,
    apiVersion,
    storeDomain,
    source: 'live-shopify-admin-graphql',
    recordedAt: new Date().toISOString(),
    cases: [
      {
        name: 'begin-not-found',
        ...beginNotFound,
      },
      {
        name: 'add-variant-missing-calculated-order',
        ...addVariantMissingCalculatedOrder,
      },
      {
        name: 'set-quantity-missing-calculated-order',
        ...setQuantityMissingCalculatedOrder,
      },
      {
        name: 'commit-missing-calculated-order',
        ...commitMissingCalculatedOrder,
      },
    ],
    sessionCases: {
      editableCreate,
      editableHydrate,
      editableBegin,
      setQuantityUnknownLine,
      addLineItemDiscountUnknownLine,
      unknownVariantHydrate,
      addVariantUnknownVariant,
      notEditableCreate,
      notEditableCancel,
      notEditableHydrate,
      beginNotEditable,
    },
    cleanup: cleanupCaptures,
    upstreamCalls,
  });

  await writeJson(paritySpecPath, {
    scenarioId,
    operationNames: [
      'orderEditBegin',
      'orderEditAddVariant',
      'orderEditSetQuantity',
      'orderEditAddLineItemDiscount',
      'orderEditCommit',
    ],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: 'config/parity-requests/orders/orderEdit-lifecycle-userErrors-begin.graphql',
      apiVersion,
      variablesCapturePath: '$.sessionCases.editableBegin.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Captured Shopify 2026-04 order-edit userError payloads for missing roots plus rendered i18n message branches on an open order-edit session: not-editable begin, unknown line item for setQuantity/addLineItemDiscount, and unknown variant for addVariant.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'begin-editable-session-user-errors',
          capturePath: '$.sessionCases.editableBegin.response.payload.data.orderEditBegin.userErrors',
          proxyPath: '$.data.orderEditBegin.userErrors',
        },
        {
          name: 'set-quantity-unknown-line-item-user-error',
          capturePath: '$.sessionCases.setQuantityUnknownLine.response.payload.data.orderEditSetQuantity.userErrors',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-lifecycle-userErrors-setQuantity.graphql',
            apiVersion,
            variables: {
              id: { fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.id' },
              lineItemId: unknownCalculatedLineItemId,
              quantity: 1,
            },
          },
          proxyPath: '$.data.orderEditSetQuantity.userErrors',
        },
        {
          name: 'add-line-item-discount-unknown-line-item-user-error',
          capturePath:
            '$.sessionCases.addLineItemDiscountUnknownLine.response.payload.data.orderEditAddLineItemDiscount.userErrors',
          proxyRequest: {
            documentPath:
              'config/parity-requests/orders/orderEdit-shipping-line-validation-add-line-item-discount.graphql',
            apiVersion,
            variables: {
              id: { fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.id' },
              lineItemId: unknownCalculatedLineItemId,
              discount: {
                description: 'Unknown line discount',
                fixedValue: {
                  amount: '1.00',
                  currencyCode: shopCurrency,
                },
              },
            },
          },
          proxyPath: '$.data.orderEditAddLineItemDiscount.userErrors',
        },
        {
          name: 'add-variant-unknown-variant-user-error',
          capturePath: '$.sessionCases.addVariantUnknownVariant.response.payload.data.orderEditAddVariant.userErrors',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-lifecycle-userErrors-addVariant.graphql',
            apiVersion,
            variables: {
              id: { fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.id' },
              variantId: unknownVariantId,
              quantity: 1,
            },
          },
          proxyPath: '$.data.orderEditAddVariant.userErrors',
        },
        {
          name: 'begin-not-found-root',
          capturePath: '$.cases[0].response.payload.data.orderEditBegin',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-lifecycle-userErrors-begin.graphql',
            apiVersion,
            variablesCapturePath: '$.cases[0].variables',
          },
          isolatedProxy: true,
          proxyPath: '$.data.orderEditBegin',
        },
        {
          name: 'add-variant-missing-calculated-order-root',
          capturePath: '$.cases[1].response.payload.data.orderEditAddVariant',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-lifecycle-userErrors-addVariant.graphql',
            apiVersion,
            variablesCapturePath: '$.cases[1].variables',
          },
          isolatedProxy: true,
          proxyPath: '$.data.orderEditAddVariant',
        },
        {
          name: 'set-quantity-missing-calculated-order-root',
          capturePath: '$.cases[2].response.payload.data.orderEditSetQuantity',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-lifecycle-userErrors-setQuantity.graphql',
            apiVersion,
            variablesCapturePath: '$.cases[2].variables',
          },
          isolatedProxy: true,
          proxyPath: '$.data.orderEditSetQuantity',
        },
        {
          name: 'commit-missing-calculated-order-root',
          capturePath: '$.cases[3].response.payload.data.orderEditCommit',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-lifecycle-userErrors-commit.graphql',
            apiVersion,
            variablesCapturePath: '$.cases[3].variables',
          },
          isolatedProxy: true,
          proxyPath: '$.data.orderEditCommit',
        },
        {
          name: 'begin-not-editable-user-error',
          capturePath: '$.sessionCases.beginNotEditable.response.payload.data.orderEditBegin.userErrors',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-lifecycle-userErrors-begin.graphql',
            apiVersion,
            variablesCapturePath: '$.sessionCases.beginNotEditable.variables',
          },
          isolatedProxy: true,
          proxyPath: '$.data.orderEditBegin.userErrors',
        },
      ],
    },
  });

  console.log(
    JSON.stringify(
      {
        fixturePath,
        paritySpecPath,
        apiVersion,
        storeDomain,
        capturedMessages: {
          beginNotEditable: mutationPayload(beginNotEditable, 'orderEditBegin')['userErrors'],
          setQuantityUnknownLine: mutationPayload(setQuantityUnknownLine, 'orderEditSetQuantity')['userErrors'],
          addLineItemDiscountUnknownLine: mutationPayload(
            addLineItemDiscountUnknownLine,
            'orderEditAddLineItemDiscount',
          )['userErrors'],
          addVariantUnknownVariant: mutationPayload(addVariantUnknownVariant, 'orderEditAddVariant')['userErrors'],
        },
      },
      null,
      2,
    ),
  );
} finally {
  for (const orderId of createdOrderIds) {
    try {
      await capture(orderCancelMutation, cleanupVariables(orderId));
    } catch (error) {
      console.error(`Cleanup failed for ${orderId}: ${(error as Error).message}`);
    }
  }
}
