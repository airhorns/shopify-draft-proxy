/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, any>;

type CaptureStep = {
  query: string;
  variables: Record<string, unknown>;
  status: number;
  response: JsonRecord;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'fulfillment-state-preconditions.json');

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
): Promise<CaptureStep> {
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

function assertNoUserErrors(step: CaptureStep, pathParts: string[], label: string): void {
  let cursor: any = step.response;
  for (const part of pathParts) {
    cursor = cursor?.[part];
  }

  const userErrors = cursor?.userErrors;
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

const orderCreateMutation = `#graphql
  mutation FulfillmentStatePreconditionsOrderCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
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
  mutation FulfillmentStatePreconditionsFulfillmentCreate($fulfillment: FulfillmentInput!, $message: String) {
    fulfillmentCreate(fulfillment: $fulfillment, message: $message) {
      fulfillment {
        id
        status
        displayStatus
        trackingInfo {
          number
          url
          company
        }
      }
      userErrors { field message }
    }
  }
`;

const fulfillmentCancelMutation = `#graphql
  mutation FulfillmentStatePreconditionsCancel($id: ID!) {
    fulfillmentCancel(id: $id) {
      fulfillment {
        id
        status
        displayStatus
        trackingInfo {
          number
          url
          company
        }
      }
      userErrors { field message }
    }
  }
`;

const fulfillmentTrackingInfoUpdateMutation = `#graphql
  mutation FulfillmentStatePreconditionsTracking(
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
        id
        status
        displayStatus
        trackingInfo {
          number
          url
          company
        }
      }
      userErrors { field message }
    }
  }
`;

const fulfillmentEventCreateMutation = `#graphql
  mutation FulfillmentStatePreconditionsEventCreate($fulfillmentEvent: FulfillmentEventInput!) {
    fulfillmentEventCreate(fulfillmentEvent: $fulfillmentEvent) {
      fulfillmentEvent {
        id
        status
        message
        happenedAt
        createdAt
      }
      userErrors { field message }
    }
  }
`;

const cleanupOrderCancelMutation = `#graphql
  mutation FulfillmentStatePreconditionsCleanup(
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
const deliveredAt = isoWithoutMilliseconds(stamp + 60_000);
const cancelledEventAt = isoWithoutMilliseconds(stamp + 120_000);

async function createFulfillmentFlow(label: string): Promise<{
  orderId: string;
  orderCreate: CaptureStep;
  fulfillmentCreate: CaptureStep;
  fulfillmentId: string;
}> {
  const orderCreate = await graphqlStep(`${label}OrderCreate`, orderCreateMutation, {
    order: {
      email: `fulfillment-state-${label}-${stamp}@example.com`,
      note: `fulfillment state preconditions capture ${label} ${stamp}`,
      tags: ['parity-probe', 'fulfillment-state-preconditions'],
      test: true,
      lineItems: [
        {
          title: `Fulfillment state ${label} line ${stamp}`,
          quantity: 1,
          priceSet: {
            shopMoney: {
              amount: '10.00',
              currencyCode: 'CAD',
            },
          },
          requiresShipping: true,
          taxable: false,
          sku: `FSP-${label}-${stamp}`,
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
  assertNoUserErrors(orderCreate, ['data', 'orderCreate'], `${label} orderCreate`);

  const order = requirePath(orderCreate.response['data']?.orderCreate?.order, `${label}.orderCreate.order`);
  const orderId = requirePath(order.id, `${label}.order.id`);
  const fulfillmentOrderId = requirePath(
    order.fulfillmentOrders?.nodes?.[0]?.id,
    `${label}.order.fulfillmentOrders.nodes[0].id`,
  );
  const trackingNumber = `FSP-${label.toUpperCase()}-${stamp}`;
  const fulfillmentCreate = await graphqlStep(`${label}FulfillmentCreate`, fulfillmentCreateMutation, {
    fulfillment: {
      notifyCustomer: false,
      trackingInfo: {
        number: trackingNumber,
        url: `https://example.com/track/${trackingNumber}`,
        company: 'Hermes',
      },
      lineItemsByFulfillmentOrder: [{ fulfillmentOrderId }],
    },
    message: `fulfillment state preconditions ${label}`,
  });
  assertNoUserErrors(fulfillmentCreate, ['data', 'fulfillmentCreate'], `${label} fulfillmentCreate`);

  const fulfillment = requirePath(
    fulfillmentCreate.response['data']?.fulfillmentCreate?.fulfillment,
    `${label}.fulfillmentCreate.fulfillment`,
  );
  const fulfillmentId = requirePath(fulfillment.id, `${label}.fulfillment.id`);

  return {
    orderId,
    orderCreate,
    fulfillmentCreate,
    fulfillmentId,
  };
}

async function cleanupOrder(orderId: string, label: string): Promise<CaptureStep> {
  try {
    return await graphqlStep(`${label}Cleanup`, cleanupOrderCancelMutation, {
      orderId,
      notifyCustomer: false,
      reason: 'OTHER',
      refund: true,
      restock: true,
    });
  } catch (error) {
    return {
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
}

const cancelledSetup = await createFulfillmentFlow('cancelled');

const initialCancel = await graphqlStep('initialCancel', fulfillmentCancelMutation, {
  id: cancelledSetup.fulfillmentId,
});
assertNoUserErrors(initialCancel, ['data', 'fulfillmentCancel'], 'initial fulfillmentCancel');

const cancelAlreadyCancelled = await graphqlStep('cancelAlreadyCancelled', fulfillmentCancelMutation, {
  id: cancelledSetup.fulfillmentId,
});
assertNoUserErrors(cancelAlreadyCancelled, ['data', 'fulfillmentCancel'], 'cancelAlreadyCancelled');

const trackingAlreadyCancelledNumber = `FSP-CANCELLED-UPDATED-${stamp}`;
const trackingAlreadyCancelled = await graphqlStep('trackingAlreadyCancelled', fulfillmentTrackingInfoUpdateMutation, {
  fulfillmentId: cancelledSetup.fulfillmentId,
  notifyCustomer: false,
  trackingInfoInput: {
    number: trackingAlreadyCancelledNumber,
    url: `https://example.com/track/${trackingAlreadyCancelledNumber}`,
    company: 'Hermes',
  },
});
assertNoUserErrors(trackingAlreadyCancelled, ['data', 'fulfillmentTrackingInfoUpdate'], 'trackingAlreadyCancelled');

const eventOnCancelled = await graphqlStep(
  'eventOnCancelled',
  fulfillmentEventCreateMutation,
  {
    fulfillmentEvent: {
      fulfillmentId: cancelledSetup.fulfillmentId,
      status: 'DELIVERED',
      message: 'cancelled fulfillment accepted delivered event',
      happenedAt: cancelledEventAt,
    },
  },
  { allowTopLevelErrors: true },
);
assertNoUserErrors(eventOnCancelled, ['data', 'fulfillmentEventCreate'], 'eventOnCancelled');

const deliveredSetup = await createFulfillmentFlow('delivered');

const deliveredEventCreate = await graphqlStep('deliveredEventCreate', fulfillmentEventCreateMutation, {
  fulfillmentEvent: {
    fulfillmentId: deliveredSetup.fulfillmentId,
    status: 'DELIVERED',
    message: 'delivered before cancel',
    happenedAt: deliveredAt,
  },
});
assertNoUserErrors(deliveredEventCreate, ['data', 'fulfillmentEventCreate'], 'deliveredEventCreate');

const cancelDelivered = await graphqlStep('cancelDelivered', fulfillmentCancelMutation, {
  id: deliveredSetup.fulfillmentId,
});
assertNoUserErrors(cancelDelivered, ['data', 'fulfillmentCancel'], 'cancelDelivered');

const cleanup = {
  cancelled: await cleanupOrder(cancelledSetup.orderId, 'cancelled'),
  delivered: await cleanupOrder(deliveredSetup.orderId, 'delivered'),
};

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  notes: [
    'Public Admin GraphQL accepts fulfillmentCancel on an already-cancelled fulfillment as idempotent success with an empty userErrors array.',
    'Public Admin GraphQL accepts fulfillmentTrackingInfoUpdate on a cancelled fulfillment and returns the fulfillment payload with an empty userErrors array.',
    'Public Admin GraphQL accepts fulfillmentEventCreate on a cancelled fulfillment and returns a fulfillmentEvent payload with an empty userErrors array.',
    'Public Admin GraphQL accepts fulfillmentCancel after a DELIVERED fulfillment event; there is no delivered-state payload userError guard.',
  ],
  setup: {
    orderCreate: cancelledSetup.orderCreate,
    fulfillmentCreate: cancelledSetup.fulfillmentCreate,
  },
  initialCancel,
  cancelAlreadyCancelled,
  trackingAlreadyCancelled,
  eventOnCancelled,
  deliveredSetup: {
    orderCreate: deliveredSetup.orderCreate,
    fulfillmentCreate: deliveredSetup.fulfillmentCreate,
  },
  deliveredEventCreate,
  cancelDelivered,
  cleanup,
  upstreamCalls: [],
};

await writeJson(fixturePath, fixture);
console.log(`wrote ${fixturePath}`);
