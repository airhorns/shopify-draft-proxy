/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  futureNoonIso,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

const cap = await createConformanceCapture();
const fixturePath = cap.fixturePath('orders', 'draft-order-delete-parity.json');

// The exact draft-order hydrate query the proxy forwards on a cold draftOrderDelete,
// read verbatim from the shared .graphql so the recorded cassette byte-matches.
const draftOrderHydrateQuery = await cap.readRequestRaw('orders', 'draft-order-hydrate.graphql');
const deleteDocument = await cap.readRequest('orders', 'draftOrderDelete-parity-plan.graphql');
const downstreamReadDocument = await cap.readRequest('orders', 'draftOrderCreate-downstream-read.graphql');

const draftOrderCreateMutation = `#graphql
  mutation DraftOrderDeleteCaptureCreate($input: DraftOrderInput!) {
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

// Disposable draft mirroring the original delete-scenario precondition (real
// customer + untracked variant on harry-test-heelo) so the proxy's cold hydrate
// reflects a real merchant draft rather than a seed.
const createInput = {
  purchasingEntity: { customerId: 'gid://shopify/Customer/9522206933298' },
  email: `hermes-draft-order-delete-${cap.stamp}@example.com`,
  note: 'draft order delete setup',
  taxExempt: true,
  reserveInventoryUntil: futureNoonIso(cap.now, 90),
  tags: ['parity-capture', 'draft-order-family', 'delete'],
  customAttributes: [
    { key: 'source', value: 'phone-order' },
    { key: 'purchase-order', value: 'PO-117' },
  ],
  appliedDiscount: {
    title: 'Loyalty credit',
    description: 'merchant order-level discount',
    value: 5,
    amount: 5,
    valueType: 'FIXED_AMOUNT',
  },
  billingAddress: {
    firstName: 'Hermes',
    lastName: 'Buyer',
    address1: '123 Queen St W',
    city: 'Toronto',
    provinceCode: 'ON',
    countryCode: 'CA',
    zip: 'M5H 2M9',
    phone: '+14165550101',
  },
  shippingAddress: {
    firstName: 'Hermes',
    lastName: 'Buyer',
    address1: '500 King St W',
    city: 'Toronto',
    provinceCode: 'ON',
    countryCode: 'CA',
    zip: 'M5V 1L9',
    phone: '+14165550102',
  },
  shippingLine: { title: 'Merchant Courier', priceWithCurrency: { amount: '7.25', currencyCode: 'CAD' } },
  lineItems: [
    {
      title: 'Custom installation service',
      quantity: 2,
      originalUnitPrice: '20.00',
      requiresShipping: false,
      taxable: false,
      sku: 'CUSTOM-INSTALL',
      appliedDiscount: {
        title: 'Service discount',
        description: '10 percent off service',
        value: 10,
        amount: 4,
        valueType: 'PERCENTAGE',
      },
      customAttributes: [{ key: 'appointment', value: 'morning' }],
    },
    { variantId: 'gid://shopify/ProductVariant/49875425296690', quantity: 1 },
  ],
};

const createPayload = await cap.run(draftOrderCreateMutation, { input: createInput }, 'draftOrderCreate');
const createdDraft = cap.mutationRoot(createPayload, 'draftOrderCreate', 'draftOrderCreate');
const draftOrderId = requireString(readRecord(createdDraft['draftOrder'])?.['id'], 'created draft order id');

// Capture the cold hydrate before the delete removes the draft.
const hydratePayload = await cap.run(draftOrderHydrateQuery, { id: draftOrderId }, 'draftOrderHydrate');

// The delete mutation under test doubles as cleanup of the disposable draft.
const deletePayload = await cap.run(deleteDocument, { input: { id: draftOrderId } }, 'draftOrderDelete');
cap.mutationRoot(deletePayload, 'draftOrderDelete', 'draftOrderDelete');

// Downstream read of the now-deleted draft (Shopify returns null).
const downstreamPayload = await cap.run(downstreamReadDocument, { id: draftOrderId }, 'draftOrderDownstreamRead');

await cap.writeJson(fixturePath, {
  scenarioId: 'draft-order-delete-live-parity',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  variables: { input: { id: draftOrderId } },
  mutation: { response: deletePayload },
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

console.log(JSON.stringify({ fixturePath, draftOrderId } satisfies JsonRecord, null, 2));
