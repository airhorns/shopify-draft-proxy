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

const scenarioId = 'orderEdit-quantity-validation';
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

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'orderEdit-quantity-validation.json');
const paritySpecPath = path.join('config', 'parity-specs', 'orders', 'orderEdit-quantity-validation.json');
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

function assertNoTopLevelErrors(label: string, captureResult: GraphqlCapture): void {
  if (captureResult.response.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
}

function assertEmptyUserErrors(label: string, captureResult: GraphqlCapture, rootName: string): void {
  assertNoTopLevelErrors(label, captureResult);
  const errors = readArray(mutationPayload(captureResult, rootName)['userErrors']);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertQuantityUserError(label: string, captureResult: GraphqlCapture, rootName: string): void {
  assertNoTopLevelErrors(label, captureResult);
  const root = mutationPayload(captureResult, rootName);
  const errors = readArray(root['userErrors']);
  const fields = errors.map(readRecord).map((error) => readArray(error?.['field']));
  if (errors.length === 0 || fields.some((field) => JSON.stringify(field) !== JSON.stringify(['quantity']))) {
    throw new Error(`${label} expected quantity userErrors: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
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
  throw new Error('No product variant available for order edit quantity validation capture');
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

const beginDocument = await readRequest('orderEdit-quantity-validation-begin.graphql');
const setQuantityDocument = await readRequest('orderEdit-quantity-validation-setQuantity.graphql');
const addVariantDocument = await readRequest('orderEdit-quantity-validation-addVariant.graphql');

const orderFields = `#graphql
  fragment OrderEditQuantityValidationOrderFields on Order {
    id
    name
    email
    note
    tags
    createdAt
    updatedAt
    cancelledAt
    displayFinancialStatus
    displayFulfillmentStatus
    presentmentCurrencyCode
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
        variant {
          id
          title
          sku
        }
      }
    }
  }
`;

const seedQuery = `#graphql
  query OrderEditQuantityValidationSeed($first: Int!) {
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
  mutation OrderEditQuantityValidationCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ...OrderEditQuantityValidationOrderFields
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
  query OrderEditQuantityValidationHydrate($id: ID!) {
    order(id: $id) {
      ...OrderEditQuantityValidationOrderFields
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation OrderEditQuantityValidationCancel(
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

function orderCreateVariables(stamp: string): JsonRecord {
  return {
    order: {
      email: `order-edit-quantity-validation-${stamp}@example.com`,
      note: `orderEdit quantity validation capture ${stamp}`,
      tags: ['order-edit-quantity-validation', stamp],
      test: true,
      currency: 'CAD',
      shippingAddress: {
        firstName: 'Conformance',
        lastName: 'OrderEditQuantity',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
      },
      lineItems: [
        {
          title: `Order edit quantity source item ${stamp}`,
          quantity: 3,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: true,
          taxable: true,
          sku: `order-edit-quantity-${stamp}`,
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

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);

let createdOrderId: string | null = null;
let cleanup: GraphqlCapture | null = null;

try {
  const seed = await capture(seedQuery, { first: 20 });
  const locationId = firstActiveLocationId(seed);
  const variant = firstProductVariant(seed);
  const variantId = requireString(variant['id'], 'variant id');

  const orderCreate = await capture(orderCreateMutation, orderCreateVariables(stamp));
  assertEmptyUserErrors('orderCreate setup', orderCreate, 'orderCreate');
  const createdOrder = orderFromPayload(orderCreate, 'orderCreate');
  createdOrderId = requireString(createdOrder['id'], 'created order id');

  const orderReadBeforeEdit = await capture(orderReadQuery, { id: createdOrderId });
  assertNoTopLevelErrors('pre-edit order read', orderReadBeforeEdit);
  const seedOrder = readRecord(responseData(orderReadBeforeEdit)['order']);
  if (!seedOrder) {
    throw new Error(`Expected pre-edit order read: ${JSON.stringify(orderReadBeforeEdit.response.payload, null, 2)}`);
  }

  const begin = await capture(beginDocument, { id: createdOrderId });
  assertEmptyUserErrors('orderEditBegin', begin, 'orderEditBegin');
  const calculatedOrderIdForCapture = calculatedOrderId(begin);
  const calculatedLineItemIdForCapture = firstCalculatedLineItemId(begin);

  const setQuantityNegative = await capture(setQuantityDocument, {
    id: calculatedOrderIdForCapture,
    lineItemId: calculatedLineItemIdForCapture,
    quantity: -1,
    restock: false,
  });
  assertQuantityUserError('negative setQuantity', setQuantityNegative, 'orderEditSetQuantity');

  const addVariantZero = await capture(addVariantDocument, {
    id: calculatedOrderIdForCapture,
    variantId,
    quantity: 0,
    allowDuplicates: true,
  });
  assertQuantityUserError('zero addVariant', addVariantZero, 'orderEditAddVariant');

  const addVariantNegative = await capture(addVariantDocument, {
    id: calculatedOrderIdForCapture,
    variantId,
    quantity: -3,
    allowDuplicates: true,
  });
  assertQuantityUserError('negative addVariant', addVariantNegative, 'orderEditAddVariant');

  const addVariantHappyPath = await capture(addVariantDocument, {
    id: calculatedOrderIdForCapture,
    variantId,
    quantity: 2,
    allowDuplicates: true,
  });
  assertEmptyUserErrors('happy addVariant after rejected operations', addVariantHappyPath, 'orderEditAddVariant');

  cleanup = await capture(orderCancelMutation, {
    orderId: createdOrderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: true,
  });

  const upstreamCalls = [
    {
      operationName: 'OrdersOrderHydrate',
      variables: { id: createdOrderId },
      query: 'hand-synthesized from live setup order read for orderEdit quantity validation replay',
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
      query: 'hand-synthesized from live product variant seed for orderEditAddVariant quantity validation replay',
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
    scenarioId,
    apiVersion,
    storeDomain,
    source: 'live-shopify-admin-graphql',
    notes:
      'Live order-edit quantity validation capture against one disposable order-edit session. Rejected quantity branches must not mutate the open calculated order; the final valid addVariant payload proves read-after-reject state.',
    setupReferences: {
      selectedLocationId: locationId,
      selectedVariant: variant,
    },
    setup: {
      seed,
      orderCreate,
      orderReadBeforeEdit,
    },
    begin,
    cases: {
      setQuantityNegative,
      addVariantZero,
      addVariantNegative,
      addVariantHappyPath,
    },
    cleanup,
    upstreamCalls,
  });

  await writeJson(paritySpecPath, {
    scenarioId,
    operationNames: ['orderEditBegin', 'orderEditSetQuantity', 'orderEditAddVariant'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'read-after-write-parity', 'known-projection-gap'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: 'config/parity-requests/orders/orderEdit-quantity-validation-begin.graphql',
      variablesCapturePath: '$.begin.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      "Captured Shopify Admin API 2026-04 orderEdit quantity validation. Negative setQuantity and zero/negative addVariant return quantity userErrors and leave the calculated order unchanged, proven by the original line item quantity remaining 3 in a final valid addVariant payload. The valid addVariant target excludes the proxy's extra calculatedOrder.lineItems added-variant projection, which is tracked separately because this scenario is scoped to rejected quantity validation.",
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'begin-session-baseline',
          capturePath: '$.begin.response.payload.data.orderEditBegin',
          proxyPath: '$.data.orderEditBegin',
          excludedPaths: ['$.calculatedOrder.id', '$.calculatedOrder.lineItems.nodes[*].id', '$.orderEditSession.id'],
        },
        {
          name: 'negative-set-quantity-user-error',
          capturePath: '$.cases.setQuantityNegative.response.payload.data.orderEditSetQuantity',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-quantity-validation-setQuantity.graphql',
            variables: {
              id: {
                fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.id',
              },
              lineItemId: {
                fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.lineItems.nodes[0].id',
              },
              quantity: -1,
              restock: false,
            },
          },
          proxyPath: '$.data.orderEditSetQuantity',
        },
        {
          name: 'zero-add-variant-user-error',
          capturePath: '$.cases.addVariantZero.response.payload.data.orderEditAddVariant',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-quantity-validation-addVariant.graphql',
            variables: {
              id: {
                fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.id',
              },
              variantId,
              quantity: 0,
              allowDuplicates: true,
            },
          },
          proxyPath: '$.data.orderEditAddVariant',
        },
        {
          name: 'negative-add-variant-user-error',
          capturePath: '$.cases.addVariantNegative.response.payload.data.orderEditAddVariant',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-quantity-validation-addVariant.graphql',
            variables: {
              id: {
                fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.id',
              },
              variantId,
              quantity: -3,
              allowDuplicates: true,
            },
          },
          proxyPath: '$.data.orderEditAddVariant',
        },
        {
          name: 'valid-add-variant-after-rejected-quantities',
          capturePath: '$.cases.addVariantHappyPath.response.payload.data.orderEditAddVariant',
          proxyRequest: {
            documentPath: 'config/parity-requests/orders/orderEdit-quantity-validation-addVariant.graphql',
            variables: {
              id: {
                fromPrimaryProxyPath: '$.data.orderEditBegin.calculatedOrder.id',
              },
              variantId,
              quantity: 2,
              allowDuplicates: true,
            },
          },
          proxyPath: '$.data.orderEditAddVariant',
          excludedPaths: [
            '$.calculatedOrder.id',
            '$.calculatedOrder.lineItems.nodes[*].id',
            '$.calculatedOrder.lineItems.nodes[1]',
            '$.calculatedOrder.addedLineItems.nodes[*].id',
            '$.calculatedLineItem.id',
            '$.orderEditSession.id',
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
        orderId: createdOrderId,
        variantId,
        locationId,
        capturedQuantityErrors: {
          setQuantityNegative: mutationPayload(setQuantityNegative, 'orderEditSetQuantity')['userErrors'],
          addVariantZero: mutationPayload(addVariantZero, 'orderEditAddVariant')['userErrors'],
          addVariantNegative: mutationPayload(addVariantNegative, 'orderEditAddVariant')['userErrors'],
        },
        cleanupUserErrors: readArray(mutationPayload(cleanup, 'orderCancel')['userErrors']),
      },
      null,
      2,
    ),
  );
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
