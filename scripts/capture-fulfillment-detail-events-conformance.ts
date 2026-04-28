/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, any>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const fixturePath = path.join(fixtureDir, 'fulfillment-detail-events-lifecycle.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphqlRequest: (query: string, variables?: Record<string, unknown>) => Promise<{ status: number; payload: any }>;
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

function isoWithoutMilliseconds(value: number): string {
  return new Date(value).toISOString().replace(/\.\d{3}Z$/u, 'Z');
}

async function graphqlStep(
  name: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<{ query: string; variables: Record<string, unknown>; status: number; response: JsonRecord }> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${name} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }

  return {
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

const fulfillmentDetailSelection = `#graphql
  fragment FulfillmentDetailEventsFields on Fulfillment {
    id
    status
    displayStatus
    createdAt
    updatedAt
    deliveredAt
    estimatedDeliveryAt
    inTransitAt
    trackingInfo(first: 1) {
      number
      url
      company
    }
    events(first: 5) {
      nodes {
        id
        status
        message
        happenedAt
        createdAt
        estimatedDeliveryAt
        city
        province
        country
        zip
        address1
        latitude
        longitude
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    service {
      id
      handle
      serviceName
      trackingSupport
      type
      location {
        id
        name
      }
    }
    location {
      id
      name
    }
    originAddress {
      address1
      address2
      city
      countryCode
      provinceCode
      zip
    }
    fulfillmentLineItems(first: 5) {
      nodes {
        id
        quantity
        lineItem {
          id
          title
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const orderCreateMutation = `#graphql
  mutation FulfillmentDetailEventsOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
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
        fulfillmentOrders(first: 5) {
          nodes {
            id
            status
            requestStatus
            assignedLocation { name }
            lineItems(first: 5) {
              nodes {
                id
                totalQuantity
                remainingQuantity
                lineItem { id title }
              }
            }
          }
        }
      }
      userErrors { field message }
    }
  }
`;

const fulfillmentCreateMutation = `#graphql
  ${fulfillmentDetailSelection}
  mutation FulfillmentDetailEventsFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        ...FulfillmentDetailEventsFields
      }
      userErrors { field message }
    }
  }
`;

const fulfillmentEventCreateMutation = `#graphql
  mutation FulfillmentDetailEventsEventCreate($fulfillmentEvent: FulfillmentEventInput!) {
    fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
      fulfillmentEvent {
        id
        status
        message
        happenedAt
        createdAt
        estimatedDeliveryAt
        city
        province
        country
        zip
        address1
        latitude
        longitude
      }
      userErrors { field message }
    }
  }
`;

const detailReadQuery = `#graphql
  ${fulfillmentDetailSelection}
  query FulfillmentDetailEventsRead($orderId: ID!, $fulfillmentId: ID!) {
    fulfillment(id: $fulfillmentId) {
      ...FulfillmentDetailEventsFields
    }
    order(id: $orderId) {
      id
      displayFulfillmentStatus
      fulfillments(first: 5) {
        ...FulfillmentDetailEventsFields
      }
    }
  }
`;

const trackingUpdateMutation = `#graphql
  ${fulfillmentDetailSelection}
  mutation FulfillmentDetailEventsTrackingUpdate(
    $fulfillmentId: ID!
    $trackingInfoInput: FulfillmentTrackingInput!
    $notifyCustomer: Boolean
  ) {
    fulfillmentTrackingInfoUpdate(
      fulfillmentId: $fulfillmentId
      trackingInfoInput: $trackingInfoInput
      notifyCustomer: $notifyCustomer
    ) {
      fulfillment {
        ...FulfillmentDetailEventsFields
      }
      userErrors { field message }
    }
  }
`;

const fulfillmentCancelMutation = `#graphql
  ${fulfillmentDetailSelection}
  mutation FulfillmentDetailEventsCancel($id: ID!) {
    fulfillmentCancel(id: $id) {
      fulfillment {
        ...FulfillmentDetailEventsFields
      }
      userErrors { field message }
    }
  }
`;

const cleanupOrderCancelMutation = `#graphql
  mutation FulfillmentDetailEventsCleanup(
    $orderId: ID!
    $notifyCustomer: Boolean
    $reason: OrderCancelReason!
    $refund: Boolean!
    $restock: Boolean!
  ) {
    orderCancel(
      orderId: $orderId
      notifyCustomer: $notifyCustomer
      reason: $reason
      refund: $refund
      restock: $restock
    ) {
      job { id done }
      orderCancelUserErrors { field message code }
      userErrors { field message }
    }
  }
`;

const stamp = Date.now();
const happenedAt = isoWithoutMilliseconds(stamp + 60_000);
const estimatedDeliveryAt = isoWithoutMilliseconds(stamp + 2 * 24 * 60 * 60 * 1000);

const orderCreate = await graphqlStep('orderCreate', orderCreateMutation, {
  order: {
    email: `hermes-fulfillment-event-${stamp}@example.com`,
    note: `HAR-235 fulfillment detail/event capture ${stamp}`,
    tags: ['parity-probe', 'har-235', 'fulfillment-event'],
    test: true,
    lineItems: [
      {
        title: `HAR-235 fulfillment event item ${stamp}`,
        quantity: 1,
        priceSet: {
          shopMoney: {
            amount: '10.00',
            currencyCode: 'CAD',
          },
        },
        requiresShipping: true,
        taxable: false,
        sku: `HAR-235-${stamp}`,
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
            amount: '10.00',
            currencyCode: 'CAD',
          },
        },
      },
    ],
  },
  options: null,
});

const order = requirePath(orderCreate.response['data']?.orderCreate?.order, 'orderCreate.order');
const orderId = requirePath(order.id, 'order.id');
const fulfillmentOrderId = requirePath(order.fulfillmentOrders?.nodes?.[0]?.id, 'order.fulfillmentOrders.nodes[0].id');
const trackingNumber = `HAR235-CREATE-${stamp}`;
const updatedTrackingNumber = `HAR235-UPDATED-${stamp}`;

const fulfillmentCreate = await graphqlStep('fulfillmentCreate', fulfillmentCreateMutation, {
  fulfillment: {
    notifyCustomer: false,
    trackingInfo: {
      number: trackingNumber,
      url: `https://example.com/track/${trackingNumber}`,
      company: 'Hermes',
    },
    lineItemsByFulfillmentOrder: [{ fulfillmentOrderId }],
  },
  message: 'HAR-235 fulfillment detail/event capture',
});

const fulfillment = requirePath(
  fulfillmentCreate.response['data']?.fulfillmentCreate?.fulfillment,
  'fulfillmentCreate.fulfillment',
);
const fulfillmentId = requirePath(fulfillment.id, 'fulfillment.id');

const fulfillmentEventCreate = await graphqlStep('fulfillmentEventCreate', fulfillmentEventCreateMutation, {
  fulfillmentEvent: {
    fulfillmentId,
    status: 'IN_TRANSIT',
    message: 'HAR-235 package scanned in transit',
    happenedAt,
    estimatedDeliveryAt,
    city: 'Toronto',
    province: 'Ontario',
    country: 'Canada',
    zip: 'M5H 2M9',
    address1: '123 Queen St W',
    latitude: 43.6532,
    longitude: -79.3832,
  },
});

const detailRead = await graphqlStep('detailRead', detailReadQuery, { orderId, fulfillmentId });

const fulfillmentTrackingInfoUpdate = await graphqlStep('fulfillmentTrackingInfoUpdate', trackingUpdateMutation, {
  fulfillmentId,
  notifyCustomer: false,
  trackingInfoInput: {
    number: updatedTrackingNumber,
    url: `https://example.com/track/${updatedTrackingNumber}`,
    company: 'Hermes Updated',
  },
});

const fulfillmentCancel = await graphqlStep('fulfillmentCancel', fulfillmentCancelMutation, { id: fulfillmentId });

let cleanup: { query: string; variables: Record<string, unknown>; status: number; response: JsonRecord };
try {
  cleanup = await graphqlStep('cleanupOrderCancel', cleanupOrderCancelMutation, {
    orderId,
    notifyCustomer: false,
    reason: 'OTHER',
    refund: true,
    restock: true,
  });
} catch (error) {
  cleanup = {
    query: cleanupOrderCancelMutation,
    variables: {
      orderId,
      notifyCustomer: false,
      reason: 'OTHER',
      refund: true,
      restock: true,
    },
    status: 0,
    response: {
      cleanupError: error instanceof Error ? error.message : String(error),
    },
  };
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  setup: {
    orderCreate,
    fulfillmentCreate,
  },
  fulfillmentEventCreate,
  detailRead,
  fulfillmentTrackingInfoUpdate,
  fulfillmentCancel,
  cleanup,
  response: detailRead.response,
};

await writeJson(fixturePath, fixture);
console.log(`wrote ${fixturePath}`);
