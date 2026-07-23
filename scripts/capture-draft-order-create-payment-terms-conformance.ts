/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  futureNoonIso,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

const cap = await createConformanceCapture();
const fixturePath = cap.fixturePath('orders', 'draft-order-create-payment-terms.json');
const createDocument = await cap.readRequest('orders', 'draftOrderCreate-payment-terms.graphql');
const readDocument = await cap.readRequest('orders', 'draftOrderCreate-payment-terms-read.graphql');

const deleteDocument = `#graphql
  mutation DraftOrderCreatePaymentTermsCleanup($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const variables: JsonRecord = {
  input: {
    lineItems: [
      {
        title: 'Draft order payment terms capture',
        quantity: 1,
        originalUnitPrice: '1.00',
      },
    ],
    paymentTerms: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
      paymentSchedules: [{ issuedAt: futureNoonIso(cap.now, 30) }],
    },
  },
};

let draftOrderId: string | null = null;
const cleanup: JsonRecord = {};

try {
  const mutationPayload = await cap.run(createDocument, variables, 'draftOrderCreate with payment terms');
  const mutationRoot = cap.mutationRoot(mutationPayload, 'draftOrderCreate', 'draftOrderCreate with payment terms');
  draftOrderId = requireString(readRecord(mutationRoot['draftOrder'])?.['id'], 'created draft order id');

  const readVariables = { id: draftOrderId };
  const readPayload = await cap.run(readDocument, readVariables, 'draftOrder read after payment terms create');

  const cleanupPayload = await cap.run(deleteDocument, { input: { id: draftOrderId } }, 'draftOrderDelete cleanup');
  cap.mutationRoot(cleanupPayload, 'draftOrderDelete', 'draftOrderDelete cleanup');
  cleanup['draftOrderDelete'] = cleanupPayload;
  draftOrderId = null;

  await cap.writeJson(fixturePath, {
    scenarioId: 'draftOrderCreate-payment-terms',
    capturedAt: new Date().toISOString(),
    storeDomain: cap.storeDomain,
    apiVersion: cap.apiVersion,
    source: 'live-shopify-admin-graphql',
    notes:
      'Live merchant-facing draftOrderCreate with NET payment terms. Template 4 creates a distinct PaymentTerms record with Net 30 / NET / dueInDays 30, and the immediate draftOrder(id:) read preserves the same payment-terms metadata.',
    variables,
    mutation: { response: mutationPayload },
    read: {
      query: readDocument,
      variables: readVariables,
      response: readPayload,
    },
    cleanup,
  });

  console.log(JSON.stringify({ fixturePath, draftOrderId: readVariables.id }, null, 2));
} finally {
  if (draftOrderId) {
    cleanup['draftOrderDelete'] = (
      await cap.runGraphqlRequest(deleteDocument, { input: { id: draftOrderId } })
    ).payload;
  }
}
