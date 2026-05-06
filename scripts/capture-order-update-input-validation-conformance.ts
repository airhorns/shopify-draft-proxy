/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'orders',
  'orderUpdate-input-validation.json',
);

const orderSelection = `
  id
  name
  updatedAt
  email
  phone
  poNumber
  note
  tags
  customer {
    id
    email
    displayName
  }
  customAttributes {
    key
    value
  }
  shippingAddress {
    firstName
    lastName
    address1
    address2
    company
    city
    province
    provinceCode
    country
    countryCodeV2
    zip
    phone
  }
  gift: metafield(namespace: "custom", key: "gift") {
    id
    namespace
    key
    type
    value
  }
  metafields(first: 10) {
    nodes {
      id
      namespace
      key
      type
      value
    }
  }
`;

const orderCreateMutation = `#graphql
  mutation OrderUpdateInputValidationCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        ${orderSelection}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderReadQuery = `#graphql
  query OrderUpdateInputValidationRead($id: ID!) {
    order(id: $id) {
      ${orderSelection}
    }
  }
`;

const orderUpdateValidationMutation = `#graphql
  mutation OrderUpdateInputValidation(
    $emptyInput: OrderInput!
    $invalidPhone: OrderInput!
    $badShippingAddress: OrderInput!
    $happyNote: OrderInput!
  ) {
    emptyInput: orderUpdate(input: $emptyInput) {
      order {
        id
        note
      }
      userErrors {
        field
        message
      }
    }
    invalidPhone: orderUpdate(input: $invalidPhone) {
      order {
        id
        phone
      }
      userErrors {
        field
        message
      }
    }
    badShippingAddress: orderUpdate(input: $badShippingAddress) {
      order {
        id
        shippingAddress {
          countryCodeV2
          provinceCode
        }
      }
      userErrors {
        field
        message
      }
    }
    happyNote: orderUpdate(input: $happyNote) {
      order {
        id
        name
        note
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation OrderUpdateInputValidationCleanup(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job {
        id
        done
      }
      orderCancelUserErrors {
        field
        message
        code
      }
      userErrors {
        field
        message
      }
    }
  }
`;

function stripGraphqlTag(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function assertNoTopLevelErrors(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function dataRoot(result: ConformanceGraphqlResult, rootName: string): JsonObject {
  const data = result.payload.data;
  const root = typeof data === 'object' && data !== null ? (data as JsonObject)[rootName] : null;
  if (typeof root !== 'object' || root === null) {
    throw new Error(`Missing ${rootName} payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return root as JsonObject;
}

function orderIdFromCreate(result: ConformanceGraphqlResult): string {
  const order = dataRoot(result, 'orderCreate').order;
  if (typeof order !== 'object' || order === null || typeof (order as JsonObject).id !== 'string') {
    throw new Error(`Missing created order id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return (order as JsonObject).id;
}

function assertUserError(label: string, result: ConformanceGraphqlResult, rootName: string): void {
  const userErrors = dataRoot(result, rootName).userErrors;
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`${label} did not return userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertEmptyUserErrors(label: string, result: ConformanceGraphqlResult, rootName: string): void {
  const userErrors = dataRoot(result, rootName).userErrors;
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function orderCreateVariables(stamp: number): JsonObject {
  return {
    order: {
      email: `order-update-input-validation-${stamp}@example.com`,
      note: 'orderUpdate input validation baseline',
      tags: ['order-update-input-validation', String(stamp)],
      test: true,
      currency: 'USD',
      lineItems: [
        {
          title: `orderUpdate input validation item ${stamp}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '1.00',
              currencyCode: 'USD',
            },
          },
          requiresShipping: false,
          taxable: false,
          sku: `order-update-validation-${stamp}`,
        },
      ],
      transactions: [
        {
          kind: 'SALE',
          status: 'SUCCESS',
          gateway: 'manual',
          test: true,
          amountSet: {
            shopMoney: {
              amount: '1.00',
              currencyCode: 'USD',
            },
          },
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

function validationVariables(orderId: string): JsonObject {
  return {
    emptyInput: {
      id: orderId,
    },
    invalidPhone: {
      id: orderId,
      phone: 'not a phone',
    },
    badShippingAddress: {
      id: orderId,
      shippingAddress: {
        address1: '3 Bad Province',
        city: 'Chicago',
        countryCode: 'US',
        provinceCode: 'ON',
      },
    },
    happyNote: {
      id: orderId,
      note: 'orderUpdate input validation happy note',
    },
  };
}

async function cleanupOrder(orderId: string): Promise<unknown> {
  const variables = {
    orderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  };
  const result = await runGraphqlRequest(orderCancelMutation, variables);
  return {
    document: stripGraphqlTag(orderCancelMutation),
    variables,
    response: result.payload,
  };
}

const stamp = Date.now();
const createVariables = orderCreateVariables(stamp);
const createResult = await runGraphqlRequest(orderCreateMutation, createVariables);
assertNoTopLevelErrors('orderCreate setup', createResult);
assertEmptyUserErrors('orderCreate setup', createResult, 'orderCreate');

const createdOrderId = orderIdFromCreate(createResult);
const beforeRead = await runGraphqlRequest(orderReadQuery, { id: createdOrderId });
assertNoTopLevelErrors('pre-update order read', beforeRead);

const variables = validationVariables(createdOrderId);
const mutationResult = await runGraphqlRequest(orderUpdateValidationMutation, variables);
assertNoTopLevelErrors('orderUpdate input validation matrix', mutationResult);
assertUserError('empty input validation', mutationResult, 'emptyInput');
assertUserError('invalid phone validation', mutationResult, 'invalidPhone');
assertUserError('bad shipping address validation', mutationResult, 'badShippingAddress');
assertEmptyUserErrors('happy note update', mutationResult, 'happyNote');

const downstreamRead = await runGraphqlRequest(orderReadQuery, { id: createdOrderId });
assertNoTopLevelErrors('post-update order read', downstreamRead);

const cleanup = await cleanupOrder(createdOrderId);

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  document: orderUpdateValidationMutation,
  variables,
  setup: {
    orderCreate: {
      document: stripGraphqlTag(orderCreateMutation),
      variables: createVariables,
      response: createResult.payload,
    },
    beforeRead: {
      document: stripGraphqlTag(orderReadQuery),
      variables: { id: createdOrderId },
      response: beforeRead.payload,
    },
  },
  mutation: {
    response: mutationResult.payload,
  },
  downstreamRead: {
    variables: { id: createdOrderId },
    response: downstreamRead.payload,
  },
  cleanup,
  upstreamCalls: [
    {
      operationName: 'OrdersOrderHydrate',
      variables: { id: createdOrderId },
      query:
        'hand-synthesized from checked-in setup.beforeRead order response for orderUpdate input validation hydration',
      response: {
        status: 200,
        body: {
          data: {
            order: (beforeRead.payload.data as JsonObject).order,
          },
        },
      },
    },
  ],
});

console.log(`Wrote ${fixturePath}`);
