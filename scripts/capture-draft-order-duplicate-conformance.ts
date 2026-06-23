/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  futureNoonIso,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

const cap = await createConformanceCapture();
const fixturePath = cap.fixturePath('orders', 'draft-order-duplicate-parity.json');

const draftOrderHydrateQuery = await cap.readRequestRaw('orders', 'draft-order-hydrate.graphql');
const duplicateDocument = await cap.readRequest('orders', 'draftOrderDuplicate-parity-plan.graphql');
const downstreamReadDocument = await cap.readRequest('orders', 'draftOrderCreate-downstream-read.graphql');

const draftOrderCreateMutation = `#graphql
  mutation DraftOrderDuplicateCaptureCreate($input: DraftOrderInput!) {
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

const draftOrderDeleteMutation = `#graphql
  mutation DraftOrderDuplicateCaptureDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

// Disposable source draft mirroring the original duplicate-scenario precondition.
const createInput = {
  purchasingEntity: { customerId: 'gid://shopify/Customer/9522206933298' },
  email: `hermes-draft-order-duplicate-${cap.stamp}@example.com`,
  note: 'draft order duplicate setup',
  taxExempt: true,
  reserveInventoryUntil: futureNoonIso(cap.now, 90),
  tags: ['parity-capture', 'draft-order-family', 'duplicate'],
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
const sourceDraftId = requireString(readRecord(createdDraft['draftOrder'])?.['id'], 'created source draft id');

// Capture the cold hydrate of the source draft the proxy forwards on duplicate.
const hydratePayload = await cap.run(draftOrderHydrateQuery, { id: sourceDraftId }, 'draftOrderHydrate');

const duplicatePayload = await cap.run(duplicateDocument, { id: sourceDraftId }, 'draftOrderDuplicate');
const duplicateRoot = cap.mutationRoot(duplicatePayload, 'draftOrderDuplicate', 'draftOrderDuplicate');
const duplicateDraftId = requireString(readRecord(duplicateRoot['draftOrder'])?.['id'], 'duplicated draft order id');

const downstreamPayload = await cap.run(downstreamReadDocument, { id: duplicateDraftId }, 'draftOrderDownstreamRead');

// Cleanup both disposable drafts (source + duplicate). Errors reported, not fatal.
const cleanupSource = await cap.runGraphqlRequest(draftOrderDeleteMutation, { input: { id: sourceDraftId } });
const cleanupDuplicate = await cap.runGraphqlRequest(draftOrderDeleteMutation, {
  input: { id: duplicateDraftId },
});

await cap.writeJson(fixturePath, {
  scenarioId: 'draft-order-duplicate-live-parity',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  variables: { id: sourceDraftId },
  mutation: { response: duplicatePayload },
  downstreamRead: { response: downstreamPayload },
  upstreamCalls: [
    {
      operationName: 'OrdersDraftOrderHydrate',
      variables: { id: sourceDraftId },
      query: draftOrderHydrateQuery,
      response: {
        status: 200,
        body: hydratePayload,
      },
    },
  ],
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      sourceDraftId,
      duplicateDraftId,
      cleanupSourceStatus: cleanupSource.status,
      cleanupDuplicateStatus: cleanupDuplicate.status,
    } satisfies JsonRecord,
    null,
    2,
  ),
);
