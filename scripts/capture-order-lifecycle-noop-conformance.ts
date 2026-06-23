/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import { createConformanceCapture, readRecord, requireString, type JsonRecord } from './conformance-capture-lib.js';

// De-seeded re-record of the orderClose/orderOpen redundant no-op safety scenarios.
//
// Both no-ops previously relied on /__meta/seed to pre-stage the precondition order.
// They now resolve the precondition the real way: the proxy forwards a cold
// `OrderManagementDownstreamRead` order hydrate on a miss (ORDER_LIFECYCLE_HYDRATE_QUERY),
// so each fixture records that full-query hydrate cassette — byte-matching the proxy's
// emitted query — instead of a seed pre-stage. The disposable orders are test orders
// created with BYPASS inventory and cancelled afterward, so the live store is left clean.
const cap = await createConformanceCapture();

// The exact order hydrate query the proxy forwards on a cold lifecycle mutation,
// read verbatim from the shared .graphql so the recorded cassette byte-matches the
// proxy's ORDER_LIFECYCLE_HYDRATE_QUERY constant (cassette match trims trailing ws).
const downstreamReadQuery = await cap.readRequestRaw('orders', 'order-management-downstream-read.graphql');
// The proxy request documents under test — run live to capture the ground-truth no-op responses.
const closeDocument = await cap.readRequest('orders', 'orderClose-noop-on-already-closed.graphql');
const openDocument = await cap.readRequest('orders', 'orderOpen-noop-on-already-open.graphql');

const orderCreateMutation = `#graphql
  mutation OrderLifecycleNoopCaptureCreate($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
    orderCreate(order: $order, options: $options) {
      order {
        id
        closed
        closedAt
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation OrderLifecycleNoopCaptureCancel($orderId: ID!, $reason: OrderCancelReason!, $notifyCustomer: Boolean!, $restock: Boolean!) {
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

// A disposable, fully-paid test order so the redundant close is a clean no-op.
function disposableOrderInput(label: string): JsonRecord {
  return {
    email: `har-588-${label}-${cap.stamp}@example.com`,
    note: `HAR-588 order lifecycle no-op ${label}`,
    tags: ['har-588', 'order-lifecycle-noop', label],
    test: true,
    currency: 'USD',
    lineItems: [
      {
        title: `HAR-588 ${label} custom item`,
        quantity: 1,
        priceSet: { shopMoney: { amount: '1.00', currencyCode: 'USD' } },
        requiresShipping: false,
        taxable: false,
        sku: `har-588-${label}-${cap.stamp}`,
      },
    ],
    transactions: [
      {
        kind: 'SALE',
        status: 'SUCCESS',
        gateway: 'manual',
        test: true,
        amountSet: { shopMoney: { amount: '1.00', currencyCode: 'USD' } },
      },
    ],
  };
}

async function createOrder(label: string): Promise<string> {
  const payload = await cap.run(
    orderCreateMutation,
    {
      order: disposableOrderInput(label),
      options: { inventoryBehaviour: 'BYPASS', sendReceipt: false, sendFulfillmentReceipt: false },
    },
    `create ${label}`,
  );
  const root = cap.mutationRoot(payload, 'orderCreate', `create ${label}`);
  return requireString(readRecord(root['order'])?.['id'], `created ${label} order id`);
}

// Forward the proxy's full order hydrate query live and shape the recorded cassette.
async function hydrateCassette(id: string): Promise<JsonRecord> {
  const body = await cap.run(downstreamReadQuery, { id }, `hydrate ${id}`);
  return {
    operationName: 'OrderManagementDownstreamRead',
    variables: { id },
    query: downstreamReadQuery,
    response: { status: 200, body },
  } satisfies JsonRecord;
}

async function cancelOrder(id: string): Promise<number> {
  const cancel = await cap.runGraphqlRequest(orderCancelMutation, {
    orderId: id,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  });
  return cancel.status;
}

// ── orderClose no-op on an already-closed order ──────────────────────────────
const closedId = await createOrder('already-closed');
// First close transitions the order to closed so the captured close is a true no-op.
const firstClosePayload = await cap.run(closeDocument, { input: { id: closedId } }, 'first close');
cap.mutationRoot(firstClosePayload, 'orderClose', 'first close');
// Hydrate AFTER the first close so the recorded cassette carries closed:true + closedAt.
const closedHydrate = await hydrateCassette(closedId);
// The redundant close — silent success, no change to closedAt/updatedAt.
const closedNoopResponse = await cap.run(closeDocument, { input: { id: closedId } }, 'redundant close');

await cap.writeJson(cap.fixturePath('orders', 'orderClose-noop-on-already-closed.json'), {
  scenarioId: 'orderClose-noop-on-already-closed',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  variables: { input: { id: closedId } },
  mutation: { response: closedNoopResponse },
  upstreamCalls: [closedHydrate],
});

// Cleanup: re-open then cancel the disposable order.
await cap.runGraphqlRequest(openDocument, { input: { id: closedId } });
const closedCancelStatus = await cancelOrder(closedId);

// ── orderOpen no-op on an already-open (never-closed) order ──────────────────
const openId = await createOrder('already-open');
const openHydrate = await hydrateCassette(openId);
// The redundant open — silent success on an order that was never closed.
const openNoopResponse = await cap.run(openDocument, { input: { id: openId } }, 'redundant open');

await cap.writeJson(cap.fixturePath('orders', 'orderOpen-noop-on-already-open.json'), {
  scenarioId: 'orderOpen-noop-on-already-open',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  variables: { input: { id: openId } },
  mutation: { response: openNoopResponse },
  upstreamCalls: [openHydrate],
});

const openCancelStatus = await cancelOrder(openId);

console.log(
  JSON.stringify(
    {
      closedFixture: cap.fixturePath('orders', 'orderClose-noop-on-already-closed.json'),
      openFixture: cap.fixturePath('orders', 'orderOpen-noop-on-already-open.json'),
      closedId,
      openId,
      closedCancelStatus,
      openCancelStatus,
      closedNoopClosed: readRecord(readRecord(readRecord(closedNoopResponse['data'])?.['orderClose'])?.['order'])?.[
        'closed'
      ],
      openNoopClosed: readRecord(readRecord(readRecord(openNoopResponse['data'])?.['orderOpen'])?.['order'])?.[
        'closed'
      ],
    } satisfies JsonRecord,
    null,
    2,
  ),
);
