/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, any>;

interface GraphqlResult {
  status: number;
  payload: JsonRecord;
}

interface CaptureContext {
  order: JsonRecord;
  orderId: string;
  lineItemId: string;
  saleTransactionId: string | null;
  locationId: string | null;
}

interface CaptureScenarioOptions {
  scenario: string;
  lineItemQuantity: number;
  buildRefundInput: (context: CaptureContext) => Record<string, unknown>;
  fixturePath: string;
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);

const partialFixturePath = path.join(fixtureDir, 'refund-create-partial-shipping-restock-parity.json');
const fullFixturePath = path.join(fixtureDir, 'refund-create-full-parity.json');
const overRefundFixturePath = path.join(fixtureDir, 'refund-create-over-refund-user-errors.json');
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphqlRequest: (query: string, variables?: Record<string, unknown>) => Promise<GraphqlResult>;
};

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function requirePath<T>(value: T | null | undefined, label: string): T {
  if (value === null || value === undefined || value === '') {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

const locationsQuery = `#graphql
  query RefundCaptureLocations {
    locations(first: 5) {
      nodes {
        id
        isActive
        fulfillsOnlineOrders
      }
    }
  }
`;

function makeOrderVariables(stamp: number, scenario: string, lineItemQuantity = 2): Record<string, unknown> {
  const unitAmount = '10.00';
  const shippingAmount = '5.00';
  const totalAmount = `${(Number(unitAmount) * lineItemQuantity + Number(shippingAmount)).toFixed(2)}`;

  return {
    order: {
      email: `hermes-refund-${scenario}-${stamp}@example.com`,
      note: `refundCreate ${scenario} parity seed order`,
      tags: ['parity-probe', 'refund-create', scenario],
      test: true,
      customAttributes: [
        {
          key: 'source',
          value: 'hermes-refund-parity',
        },
        {
          key: 'scenario',
          value: scenario,
        },
      ],
      billingAddress: {
        firstName: 'Hermes',
        lastName: 'Refund',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
        phone: '+14165550101',
      },
      shippingAddress: {
        firstName: 'Hermes',
        lastName: 'Refund',
        address1: '123 Queen St W',
        city: 'Toronto',
        provinceCode: 'ON',
        countryCode: 'CA',
        zip: 'M5H 2M9',
        phone: '+14165550101',
      },
      shippingLines: [
        {
          title: 'Standard',
          code: 'STANDARD',
          source: 'hermes-refund-parity',
          priceSet: {
            shopMoney: {
              amount: shippingAmount,
              currencyCode: 'CAD',
            },
          },
        },
      ],
      lineItems: [
        {
          title: `Hermes refundable ${scenario} item`,
          quantity: lineItemQuantity,
          priceSet: {
            shopMoney: {
              amount: unitAmount,
              currencyCode: 'CAD',
            },
          },
          requiresShipping: true,
          taxable: false,
          sku: `hermes-refund-${scenario}-${stamp}`,
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
              amount: totalAmount,
              currencyCode: 'CAD',
            },
          },
        },
      ],
    },
    options: null,
  };
}

const orderCreateMutation = `#graphql
  mutation RefundCaptureOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        createdAt
        updatedAt
        displayFinancialStatus
        displayFulfillmentStatus
        note
        tags
        customAttributes { key value }
        subtotalPriceSet { shopMoney { amount currencyCode } }
        currentTotalPriceSet { shopMoney { amount currencyCode } }
        totalPriceSet { shopMoney { amount currencyCode } }
        totalRefundedSet { shopMoney { amount currencyCode } }
        shippingLines(first: 5) {
          nodes {
            title
            code
            originalPriceSet { shopMoney { amount currencyCode } }
          }
        }
        lineItems(first: 5) {
          nodes {
            id
            title
            quantity
            sku
            variantTitle
            originalUnitPriceSet { shopMoney { amount currencyCode } }
          }
        }
        transactions {
          id
          kind
          status
          gateway
          amountSet { shopMoney { amount currencyCode } }
        }
        refunds {
          id
          note
        }
        returns(first: 5) {
          nodes { id status }
          pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
      }
      userErrors { field message }
    }
  }
`;

const refundCreateMutation = `#graphql
  mutation RefundCreateParity($input: RefundInput!) {
    refundCreate(input: $input) {
      refund {
        id
        note
        createdAt
        updatedAt
        totalRefundedSet { shopMoney { amount currencyCode } }
        refundLineItems(first: 5) {
          nodes {
            id
            quantity
            restockType
            restocked
            lineItem {
              id
              title
            }
            subtotalSet { shopMoney { amount currencyCode } }
          }
        }
        transactions(first: 5) {
          nodes {
            id
            kind
            status
            gateway
            amountSet { shopMoney { amount currencyCode } }
          }
        }
      }
      order {
        id
        displayFinancialStatus
        totalRefundedSet { shopMoney { amount currencyCode } }
      }
      userErrors { field message }
    }
  }
`;

const orderReadAfterRefund = `#graphql
  query OrderRefundReadParity($id: ID!) {
    order(id: $id) {
      id
      name
      displayFinancialStatus
      displayFulfillmentStatus
      refunds {
        id
        note
        totalRefundedSet { shopMoney { amount currencyCode } }
      }
      returns(first: 5) {
        nodes { id status }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      transactions {
        id
        kind
        status
        gateway
        amountSet { shopMoney { amount currencyCode } }
      }
      totalRefundedSet { shopMoney { amount currencyCode } }
    }
  }
`;

async function captureScenario({
  scenario,
  lineItemQuantity,
  buildRefundInput,
  fixturePath,
}: CaptureScenarioOptions): Promise<Record<string, unknown>> {
  const stamp = Date.now();
  const orderVariables = makeOrderVariables(stamp, scenario, lineItemQuantity);
  const orderCreate = await runGraphql(orderCreateMutation, orderVariables);
  const order = orderCreate.payload?.data?.orderCreate?.order;
  const orderId = requirePath(order?.id, `${scenario}.order.id`);
  const lineItemId = requirePath(order?.lineItems?.nodes?.[0]?.id, `${scenario}.lineItem.id`);
  const saleTransactionId = order?.transactions?.[0]?.id ?? null;
  const locations = await runGraphql(locationsQuery, {});
  const locationId =
    locations.payload?.data?.locations?.nodes?.find((location) => location?.isActive === true)?.id ??
    locations.payload?.data?.locations?.nodes?.[0]?.id ??
    null;
  const refundVariables = {
    input: buildRefundInput({
      order,
      orderId,
      lineItemId,
      saleTransactionId,
      locationId,
    }),
  };
  const refund = await runGraphql(refundCreateMutation, refundVariables);
  const downstreamRead = await runGraphql(orderReadAfterRefund, { id: orderId });

  await writeJson(fixturePath, {
    variables: refundVariables,
    setup: {
      orderCreate: {
        variables: orderVariables,
        response: orderCreate.payload,
      },
      locations: {
        response: locations.payload,
      },
    },
    mutation: {
      response: refund.payload,
    },
    downstreamRead: {
      variables: { id: orderId },
      response: downstreamRead.payload,
    },
  });

  return {
    scenario,
    fixturePath,
    orderId,
    refundId: refund.payload?.data?.refundCreate?.refund?.id ?? null,
    userErrors: refund.payload?.data?.refundCreate?.userErrors ?? null,
  };
}

const partial = await captureScenario({
  scenario: 'partial-shipping-restock',
  lineItemQuantity: 2,
  fixturePath: partialFixturePath,
  buildRefundInput: ({ orderId, lineItemId, locationId, saleTransactionId }) => ({
    orderId,
    note: 'partial line item and shipping refund',
    notify: false,
    refundLineItems: [
      {
        lineItemId,
        quantity: 1,
        restockType: 'RETURN',
        locationId,
      },
    ],
    shipping: {
      amount: '5.00',
    },
    transactions: [
      {
        amount: '15.00',
        gateway: 'manual',
        kind: 'REFUND',
        orderId,
        parentId: saleTransactionId,
      },
    ],
  }),
});

const full = await captureScenario({
  scenario: 'full',
  lineItemQuantity: 1,
  fixturePath: fullFixturePath,
  buildRefundInput: ({ orderId, lineItemId, saleTransactionId }) => ({
    orderId,
    note: 'full line item and shipping refund',
    notify: false,
    refundLineItems: [
      {
        lineItemId,
        quantity: 1,
        restockType: 'NO_RESTOCK',
      },
    ],
    shipping: {
      fullRefund: true,
    },
    transactions: [
      {
        amount: '15.00',
        gateway: 'manual',
        kind: 'REFUND',
        orderId,
        parentId: saleTransactionId,
      },
    ],
  }),
});

const overRefund = await captureScenario({
  scenario: 'over-refund',
  lineItemQuantity: 1,
  fixturePath: overRefundFixturePath,
  buildRefundInput: ({ orderId, lineItemId, saleTransactionId }) => ({
    orderId,
    note: 'invalid over refund',
    notify: false,
    refundLineItems: [
      {
        lineItemId,
        quantity: 1,
        restockType: 'NO_RESTOCK',
      },
    ],
    shipping: {
      fullRefund: true,
    },
    transactions: [
      {
        amount: '25.00',
        gateway: 'manual',
        kind: 'REFUND',
        orderId,
        parentId: saleTransactionId,
      },
    ],
  }),
});

console.log(
  JSON.stringify(
    {
      ok: true,
      partial,
      full,
      overRefund,
    },
    null,
    2,
  ),
);
