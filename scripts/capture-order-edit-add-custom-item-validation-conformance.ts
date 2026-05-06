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

const scenarioId = 'orderEditAddCustomItem-validation';
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
  fragment OrderEditAddCustomItemValidationOrderFields on Order {
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
    totalOutstandingSet {
      shopMoney {
        amount
        currencyCode
      }
      presentmentMoney {
        amount
        currencyCode
      }
    }
    totalReceivedSet {
      shopMoney {
        amount
        currencyCode
      }
      presentmentMoney {
        amount
        currencyCode
      }
    }
    totalRefundedSet {
      shopMoney {
        amount
        currencyCode
      }
      presentmentMoney {
        amount
        currencyCode
      }
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
    transactions {
      id
      kind
      status
      gateway
      amountSet {
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
    refunds {
      id
      note
      totalRefundedSet {
        shopMoney {
          amount
          currencyCode
        }
        presentmentMoney {
          amount
          currencyCode
        }
      }
      refundLineItems(first: 10) {
        nodes {
          id
          quantity
          restockType
          lineItem {
            id
            title
          }
          subtotalSet {
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
      transactions(first: 10) {
        nodes {
          id
          kind
          status
          gateway
          amountSet {
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
    }
    fulfillments {
      id
      status
      displayStatus
      createdAt
      updatedAt
      trackingInfo {
        number
        url
        company
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
  mutation OrderEditAddCustomItemValidationCreate(
    $order: OrderCreateOrderInput!
    $options: OrderCreateOptionsInput
  ) {
    orderCreate(order: $order, options: $options) {
      order {
        ...OrderEditAddCustomItemValidationOrderFields
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
  query OrderEditAddCustomItemValidationHydrate($id: ID!) {
    order(id: $id) {
      ...OrderEditAddCustomItemValidationOrderFields
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation OrderEditAddCustomItemValidationCancel(
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

function assertTopLevelErrorCode(label: string, captureResult: GraphqlCapture, code: string): void {
  const errors = readArray(captureResult.response.payload.errors);
  const first = readRecord(errors[0]);
  const extensions = readRecord(first?.['extensions']);
  if (extensions?.['code'] !== code) {
    throw new Error(`${label} expected top-level ${code}: ${JSON.stringify(captureResult.response.payload, null, 2)}`);
  }
}

function assertUserError(label: string, captureResult: GraphqlCapture, field: string[], message: string): void {
  assertNoTopLevelErrors(label, captureResult);
  const errors = readArray(mutationPayload(captureResult, 'orderEditAddCustomItem')['userErrors']);
  const first = readRecord(errors[0]);
  const actualField = readArray(first?.['field']);
  if (JSON.stringify(actualField) !== JSON.stringify(field) || first?.['message'] !== message) {
    throw new Error(
      `${label} expected userError ${JSON.stringify({ field, message })}: ${JSON.stringify(
        captureResult.response.payload,
        null,
        2,
      )}`,
    );
  }
}

function orderCreateVariables(stamp: string): JsonRecord {
  return {
    order: {
      email: `order-edit-add-custom-item-validation-${stamp}@example.com`,
      note: `orderEditAddCustomItem validation capture ${stamp}`,
      tags: ['order-edit-add-custom-item-validation', stamp],
      test: true,
      currency: 'CAD',
      shippingAddress: {
        firstName: 'Conformance',
        lastName: 'CustomItem',
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
          sku: `order-edit-custom-item-${stamp}`,
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

function caseVariables(
  calculatedOrderIdValue: string,
  title: string,
  quantity: number,
  amount: string,
  currencyCode: string,
): JsonRecord {
  return {
    id: calculatedOrderIdValue,
    title,
    quantity,
    price: {
      amount,
      currencyCode,
    },
  };
}

const beginDocument = await readRequest('orderEditAddCustomItem-validation-begin.graphql');
const missingTitleDocument = await readRequest('orderEditAddCustomItem-validation-missing-title.graphql');
const caseDocument = await readRequest('orderEditAddCustomItem-validation-case.graphql');
const inlineMissingCurrencyDocument = await readRequest(
  'orderEditAddCustomItem-validation-inline-missing-currency.graphql',
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

  const orderReadBeforeEdit = await capture(orderReadQuery, { id: createdOrderId });
  assertNoTopLevelErrors('pre-edit order read', orderReadBeforeEdit);
  const seedOrder = readRecord(responseData(orderReadBeforeEdit)['order']);
  if (!seedOrder) {
    throw new Error(`Expected pre-edit order read: ${JSON.stringify(orderReadBeforeEdit.response.payload, null, 2)}`);
  }

  const begin = await capture(beginDocument, { id: createdOrderId });
  assertNoTopLevelErrors('orderEditBegin', begin);
  assertEmptyUserErrors('orderEditBegin', begin, 'orderEditBegin');
  const calculatedOrderIdValue = calculatedOrderId(begin);
  const oversizedTitle = 'x'.repeat(256);

  const missingTitle = await capture(missingTitleDocument, {
    id: calculatedOrderIdValue,
    quantity: 1,
    price: {
      amount: '1.00',
      currencyCode: 'CAD',
    },
  });
  assertTopLevelErrorCode('missing title', missingTitle, 'missingRequiredArguments');

  const blankTitle = await capture(caseDocument, caseVariables(calculatedOrderIdValue, '', 1, '1.00', 'CAD'));
  assertUserError('blank title', blankTitle, ['title'], "can't be blank");

  const oversizedTitleResult = await capture(
    caseDocument,
    caseVariables(calculatedOrderIdValue, oversizedTitle, 1, '1.00', 'CAD'),
  );
  assertUserError('oversized title', oversizedTitleResult, ['title'], 'is too long (maximum is 255 characters)');

  const zeroQuantity = await capture(
    caseDocument,
    caseVariables(calculatedOrderIdValue, 'Quantity zero', 0, '1.00', 'CAD'),
  );
  assertUserError('zero quantity', zeroQuantity, ['quantity'], 'must be greater than 0');

  const negativeQuantity = await capture(
    caseDocument,
    caseVariables(calculatedOrderIdValue, 'Quantity negative', -1, '1.00', 'CAD'),
  );
  assertUserError('negative quantity', negativeQuantity, ['quantity'], 'must be greater than 0');

  const negativePrice = await capture(
    caseDocument,
    caseVariables(calculatedOrderIdValue, 'Negative price', 1, '-5.00', 'CAD'),
  );
  assertUserError('negative price', negativePrice, ['price', 'amount'], 'must be greater than or equal to 0');

  const inlineMissingCurrency = await capture(inlineMissingCurrencyDocument, { id: calculatedOrderIdValue });
  assertTopLevelErrorCode('inline missing currency', inlineMissingCurrency, 'missingRequiredInputObjectAttribute');

  const currencyMismatch = await capture(
    caseDocument,
    caseVariables(calculatedOrderIdValue, 'Currency mismatch', 1, '1.00', 'USD'),
  );
  assertUserError('currency mismatch', currencyMismatch, ['price', 'amount'], 'Currency must be CAD.');

  const happyPath = await capture(
    caseDocument,
    caseVariables(calculatedOrderIdValue, 'Happy custom item', 2, '3.00', 'CAD'),
  );
  assertNoTopLevelErrors('happy path', happyPath);
  assertEmptyUserErrors('happy path', happyPath, 'orderEditAddCustomItem');

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
      'Live orderEditAddCustomItem validation capture against one disposable CAD order-edit session. Invalid branches do not stage a line item; the happy path is captured last and the source order is cancelled in cleanup.',
    setup: {
      orderCreate,
      orderReadBeforeEdit,
    },
    begin,
    cases: {
      missingTitle,
      blankTitle,
      oversizedTitle: {
        ...oversizedTitleResult,
        variables: {
          ...oversizedTitleResult.variables,
          title: oversizedTitle,
        },
      },
      zeroQuantity,
      negativeQuantity,
      negativePrice,
      inlineMissingCurrency,
      currencyMismatch,
      happyPath,
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'OrdersOrderHydrate',
        variables: { id: createdOrderId },
        query: 'hand-synthesized from live setup order read for orderEditAddCustomItem validation replay',
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
