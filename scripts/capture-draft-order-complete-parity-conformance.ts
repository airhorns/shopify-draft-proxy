/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import { createConformanceCapture, readRecord, requireString, type JsonRecord } from './conformance-capture-lib.js';

const cap = await createConformanceCapture();
const fixturePath = cap.fixturePath('orders', 'draft-order-complete-parity.json');

// The exact draft-order hydrate query the proxy forwards on a cold draftOrderComplete,
// read verbatim from the shared .graphql so the recorded cassette byte-matches the Rust
// `DRAFT_ORDER_HYDRATE_QUERY` (`include_str!` of the same file).
const draftOrderHydrateQuery = await cap.readRequestRaw('orders', 'draft-order-hydrate.graphql');
const completeDocument = await cap.readRequest('orders', 'draftOrderComplete-parity-plan.graphql');
const downstreamReadDocument = await cap.readRequest('orders', 'draftOrderComplete-downstream-read.graphql');

const draftOrderCreateMutation = `#graphql
  mutation DraftOrderCompleteCaptureCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
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
  mutation DraftOrderCompleteCaptureCleanup($orderId: ID!, $reason: OrderCancelReason!, $notifyCustomer: Boolean!, $restock: Boolean!) {
    orderCancel(orderId: $orderId, reason: $reason, notifyCustomer: $notifyCustomer, restock: $restock) {
      job { id }
      userErrors { field message }
    }
  }
`;

// Disposable, fully-ready draft using only non-taxable custom line items so the proxy's
// local completion math (which echoes the hydrated draft's totals) matches Shopify exactly
// without any inventory or tax coupling. The completed order inherits the draft's note/tags.
const createInput = {
  email: `hermes-draft-order-complete-${cap.stamp}@example.com`,
  note: 'merchant realistic draft order complete parity probe',
  tags: ['draft-order-complete', 'merchant-realistic', 'parity-probe'],
  taxExempt: true,
  customAttributes: [{ key: 'source', value: 'phone-order' }],
  shippingAddress: {
    firstName: 'Hermes',
    lastName: 'Buyer',
    address1: '123 Queen St W',
    city: 'Toronto',
    provinceCode: 'ON',
    countryCode: 'CA',
    zip: 'M5H 2M9',
  },
  lineItems: [
    {
      title: 'Custom installation service',
      quantity: 2,
      originalUnitPrice: '20.00',
      requiresShipping: false,
      taxable: false,
      sku: `hermes-custom-service-${cap.stamp}`,
    },
    {
      title: 'Premium support plan',
      quantity: 1,
      originalUnitPrice: '15.00',
      requiresShipping: false,
      taxable: false,
      sku: `hermes-support-${cap.stamp}`,
    },
  ],
};

const createPayload = await cap.run(draftOrderCreateMutation, { input: createInput }, 'draftOrderCreate');
const createdDraft = cap.mutationRoot(createPayload, 'draftOrderCreate', 'draftOrderCreate');
const draftOrderId = requireString(readRecord(createdDraft['draftOrder'])?.['id'], 'created draft order id');

// Capture the cold hydrate (draft still OPEN) before completion consumes it.
const hydratePayload = await cap.run(draftOrderHydrateQuery, { id: draftOrderId }, 'draftOrderHydrate');

const completeVariables = {
  id: draftOrderId,
  paymentGatewayId: null,
  sourceName: 'hermes-cron-orders',
  paymentPending: false,
};
const completePayload = await cap.run(completeDocument, completeVariables, 'draftOrderComplete');
const completedDraft = cap.mutationRoot(completePayload, 'draftOrderComplete', 'draftOrderComplete');
const completedOrderId = requireString(
  readRecord(readRecord(completedDraft['draftOrder'])?.['order'])?.['id'],
  'completed order id',
);

// Downstream read of the now-completed draft (status COMPLETED, linked order).
const downstreamPayload = await cap.run(downstreamReadDocument, { id: draftOrderId }, 'draftOrderDownstreamRead');

// Cleanup: cancel the resulting order. restock:false — custom line items track no inventory.
await cap.runGraphqlRequest(orderCancelMutation, {
  orderId: completedOrderId,
  reason: 'OTHER',
  notifyCustomer: false,
  restock: false,
});

await cap.writeJson(fixturePath, {
  scenarioId: 'draft-order-complete-live-parity',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  notes:
    'Live draft-order completion capture (re-homed from very-big-test-store to harry-test-heelo). A disposable, fully-ready draft with two non-taxable custom line items is completed; the precondition draft is resolved via a real cold OrdersDraftOrderHydrate forward (the single upstreamCall) rather than a setup-block seed. The completed order inherits the draft note/tags; sourceName is the acting app id, payment settles through the manual gateway.',
  variables: completeVariables,
  mutation: { response: completePayload },
  downstreamRead: { response: downstreamPayload },
  upstreamCalls: [
    {
      operationName: 'OrdersDraftOrderHydrate',
      variables: { id: draftOrderId },
      query: draftOrderHydrateQuery,
      response: {
        status: 200,
        body: hydratePayload,
      },
    },
  ],
});

console.log(JSON.stringify({ fixturePath, draftOrderId, completedOrderId } satisfies JsonRecord, null, 2));
