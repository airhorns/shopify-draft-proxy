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
const fixturePath = path.join(fixtureDir, 'fulfillment-event-create-validation.json');

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
  options: { allowTopLevelErrors?: boolean } = {},
): Promise<{ query: string; variables: Record<string, unknown>; status: number; response: JsonRecord }> {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300 || (!options.allowTopLevelErrors && result.payload.errors)) {
    throw new Error(`${name} failed: ${JSON.stringify(result.payload, null, 2)}`);
  }

  return {
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

function upstreamCallFromStep(
  operationName: string,
  step: { query: string; variables: Record<string, unknown>; status: number; response: JsonRecord },
): {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: { status: number; body: JsonRecord };
} {
  return {
    operationName,
    variables: step.variables,
    query: step.query,
    response: {
      status: step.status,
      body: step.response,
    },
  };
}

const fulfillmentDetailSelection = `#graphql
  fragment FulfillmentEventCreateValidationFulfillmentFields on Fulfillment {
    id
    status
    displayStatus
    estimatedDeliveryAt
    inTransitAt
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
  }
`;

const fulfillmentHydrateQuery = `#graphql
  query ShippingFulfillmentEventCreateFulfillmentHydrate($id: ID!) {
    fulfillment(id: $id) {
      id
      status
      displayStatus
      createdAt
      updatedAt
      deliveredAt
      estimatedDeliveryAt
      inTransitAt
      trackingInfo(first: 1) { number url company }
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
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      service {
        id
        handle
        serviceName
        trackingSupport
        type
        location { id name }
      }
      location { id name }
      originAddress { address1 address2 city countryCode provinceCode zip }
      fulfillmentLineItems(first: 5) {
        nodes { id quantity lineItem { id title } }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      order { id name displayFulfillmentStatus }
    }
  }
`;

const orderCreateMutation = `#graphql
  mutation FulfillmentEventCreateValidationOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        name
        displayFulfillmentStatus
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
  mutation FulfillmentEventCreateValidationFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        id
        status
        displayStatus
        trackingInfo(first: 1) {
          number
          url
          company
        }
        events(first: 5) {
          nodes { id status message happenedAt createdAt }
        }
      }
      userErrors { field message }
    }
  }
`;

const fulfillmentEventCreateMutation = `#graphql
  mutation FulfillmentEventCreateValidation($fulfillmentEvent: FulfillmentEventInput!) {
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
      userErrors {
        field
        message
      }
    }
  }
`;

const fulfillmentEventCreateWithCodeSelectionMutation = `#graphql
  mutation FulfillmentEventCreateCodeSelection($fulfillmentEvent: FulfillmentEventInput!) {
    fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
      fulfillmentEvent {
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

const fulfillmentEventStatusEnumQuery = `#graphql
  query FulfillmentEventStatusEnumIntrospection {
    __type(name: "FulfillmentEventStatus") {
      kind
      enumValues {
        name
      }
    }
  }
`;

const detailReadQuery = `#graphql
  ${fulfillmentDetailSelection}
  query FulfillmentEventCreateValidationDetailRead($fulfillmentId: ID!) {
    fulfillment(id: $fulfillmentId) {
      ...FulfillmentEventCreateValidationFulfillmentFields
    }
  }
`;

const fulfillmentCancelMutation = `#graphql
  mutation FulfillmentEventCreateValidationCancel($id: ID!) {
    fulfillmentCancel(id: $id) {
      fulfillment {
        id
        status
        displayStatus
      }
      userErrors { field message }
    }
  }
`;

const cleanupOrderCancelMutation = `#graphql
  mutation FulfillmentEventCreateValidationCleanup(
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
const deliveredAt = isoWithoutMilliseconds(stamp + 2 * 60_000);
const estimatedDeliveryAt = isoWithoutMilliseconds(stamp + 2 * 24 * 60 * 60 * 1000);

const orderCreate = await graphqlStep('orderCreate', orderCreateMutation, {
  order: {
    email: `hermes-fulfillment-event-validation-${stamp}@example.com`,
    note: `HAR-582 fulfillmentEventCreate validation capture ${stamp}`,
    tags: ['parity-probe', 'har-582', 'fulfillment-event-create'],
    test: true,
    lineItems: [
      {
        title: `HAR-582 fulfillment event item ${stamp}`,
        quantity: 1,
        priceSet: {
          shopMoney: {
            amount: '10.00',
            currencyCode: 'CAD',
          },
        },
        requiresShipping: true,
        taxable: false,
        sku: `HAR-582-${stamp}`,
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
const trackingNumber = `HAR582-CREATE-${stamp}`;

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
  message: 'HAR-582 fulfillmentEventCreate validation capture',
});

const fulfillment = requirePath(
  fulfillmentCreate.response['data']?.fulfillmentCreate?.fulfillment,
  'fulfillmentCreate.fulfillment',
);
const fulfillmentId = requirePath(fulfillment.id, 'fulfillment.id');
const unknownFulfillmentId = `gid://shopify/Fulfillment/999999999${stamp}`;

const unknownFulfillmentHydrate = await graphqlStep('unknownFulfillmentHydrate', fulfillmentHydrateQuery, {
  id: unknownFulfillmentId,
});

const fulfillmentHydrate = await graphqlStep('fulfillmentHydrate', fulfillmentHydrateQuery, {
  id: fulfillmentId,
});

const fulfillmentEventStatusEnum = await graphqlStep('fulfillmentEventStatusEnum', fulfillmentEventStatusEnumQuery);

const userErrorCodeSelection = await graphqlStep(
  'userErrorCodeSelection',
  fulfillmentEventCreateWithCodeSelectionMutation,
  {
    fulfillmentEvent: {
      fulfillmentId: unknownFulfillmentId,
      status: 'IN_TRANSIT',
    },
  },
  { allowTopLevelErrors: true },
);

const unknownFulfillmentEventCreate = await graphqlStep(
  'unknownFulfillmentEventCreate',
  fulfillmentEventCreateMutation,
  {
    fulfillmentEvent: {
      fulfillmentId: unknownFulfillmentId,
      status: 'IN_TRANSIT',
    },
  },
);

const fulfillmentEventCreate = await graphqlStep('fulfillmentEventCreate', fulfillmentEventCreateMutation, {
  fulfillmentEvent: {
    fulfillmentId,
    status: 'IN_TRANSIT',
    message: 'HAR-582 package scanned in transit',
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

const detailRead = await graphqlStep('detailRead', detailReadQuery, { fulfillmentId });

const invalidStatusVariable = await graphqlStep(
  'invalidStatusVariable',
  fulfillmentEventCreateMutation,
  {
    fulfillmentEvent: {
      fulfillmentId,
      status: 'NOT_A_FULFILLMENT_EVENT_STATUS',
    },
  },
  { allowTopLevelErrors: true },
);

const fulfillmentCancel = await graphqlStep('fulfillmentCancel', fulfillmentCancelMutation, { id: fulfillmentId });

const cancelledFulfillmentEventCreateProbe = await graphqlStep(
  'cancelledFulfillmentEventCreateProbe',
  fulfillmentEventCreateMutation,
  {
    fulfillmentEvent: {
      fulfillmentId,
      status: 'DELIVERED',
      message: 'HAR-582 cancelled fulfillment public API probe',
      happenedAt: deliveredAt,
    },
  },
  { allowTopLevelErrors: true },
);

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
  notes: [
    'Public Admin GraphQL 2026-04 UserError for fulfillmentEventCreate exposes field/message but not code.',
    'Selecting userErrors.code returns a top-level undefinedField error before resolver execution.',
    'Invalid FulfillmentEventStatus variables fail GraphQL coercion with INVALID_VARIABLE before resolver execution.',
    'FulfillmentEventStatus introspection includes DELAYED and CARRIER_PICKED_UP in Admin GraphQL 2026-04.',
    'The cancelledFulfillmentEventCreateProbe records public API behavior after fulfillmentCancel; HAR-582 local runtime keeps the source-backed cancellation guard.',
  ],
  setup: {
    orderCreate,
    fulfillmentCreate,
    unknownFulfillmentHydrate,
    fulfillmentHydrate,
  },
  fulfillmentEventStatusEnum,
  userErrorCodeSelection,
  unknownFulfillmentEventCreate,
  fulfillmentEventCreate,
  detailRead,
  invalidStatusVariable,
  fulfillmentCancel,
  cancelledFulfillmentEventCreateProbe,
  cleanup,
  response: detailRead.response,
  upstreamCalls: [
    upstreamCallFromStep('ShippingFulfillmentEventCreateFulfillmentHydrate', unknownFulfillmentHydrate),
    upstreamCallFromStep('ShippingFulfillmentEventCreateFulfillmentHydrate', fulfillmentHydrate),
  ],
};

await writeJson(fixturePath, fixture);
console.log(`wrote ${fixturePath}`);
