/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';
import { captureDraftProxyShopPricingHydrate } from './support/shopify/runtime-hydration-capture.js';

const cap = await createConformanceCapture();
const shopPricingHydrate = await captureDraftProxyShopPricingHydrate((query, variables) =>
  cap.runGraphqlRequest(query, variables),
);
const fixturePath = cap.fixturePath('orders', 'draft-order-invoice-send-created-no-recipient.json');

const createDocument = await cap.readRequest('orders', 'draftOrderInvoiceSend-created-no-recipient-create.graphql');
const sendDocument = await cap.readRequest('orders', 'draftOrderInvoiceSend-created-no-recipient-send.graphql');

const deleteMutation = `#graphql
  mutation DraftOrderInvoiceSendCreatedNoRecipientDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

function mutationRootAllowingUserErrors(payload: JsonRecord, rootName: string, label: string): JsonRecord {
  const root = readRecord(readRecord(payload['data'])?.[rootName]);
  if (!root) {
    throw new Error(`${label} missing ${rootName}: ${JSON.stringify(payload, null, 2)}`);
  }
  return root;
}

function draftOrderFromRoot(root: JsonRecord, label: string): JsonRecord {
  const draftOrder = readRecord(root['draftOrder']);
  if (!draftOrder) {
    throw new Error(`${label} missing draftOrder: ${JSON.stringify(root, null, 2)}`);
  }
  return draftOrder;
}

function assertLineAndTotals(draftOrder: JsonRecord, label: string): void {
  const lineItems = readArray(readRecord(draftOrder['lineItems'])?.['nodes']);
  const firstLine = readRecord(lineItems[0]);
  if (!firstLine) {
    throw new Error(`${label} missing first line item: ${JSON.stringify(draftOrder, null, 2)}`);
  }
  if (firstLine['title'] !== 'Invoice error parity item' || firstLine['quantity'] !== 2) {
    throw new Error(`${label} unexpected line item: ${JSON.stringify(firstLine, null, 2)}`);
  }
  const total = readRecord(readRecord(draftOrder['totalPriceSet'])?.['shopMoney']);
  if (total?.['amount'] !== '6.5') {
    throw new Error(`${label} unexpected totalPriceSet: ${JSON.stringify(draftOrder['totalPriceSet'], null, 2)}`);
  }
}

const createVariables: JsonRecord = {
  input: {
    note: `Draft order invoice created no-recipient ${cap.stamp}`,
    tags: ['draft-proxy-capture', 'invoice-created-no-recipient', cap.stamp],
    lineItems: [
      {
        title: 'Invoice error parity item',
        quantity: 2,
        originalUnitPriceWithCurrency: {
          amount: '3.25',
          currencyCode: 'CAD',
        },
        requiresShipping: false,
        taxable: false,
      },
    ],
  },
};

let draftOrderId: string | null = null;
let createResponse: JsonRecord | null = null;
let sendResponse: JsonRecord | null = null;
let cleanup: JsonRecord | null = null;

try {
  createResponse = await cap.run(createDocument, createVariables, 'create no-recipient invoice draft');
  const createRoot = cap.mutationRoot(createResponse, 'draftOrderCreate', 'create no-recipient invoice draft');
  const createdDraft = draftOrderFromRoot(createRoot, 'created draft');
  draftOrderId = requireString(createdDraft['id'], 'created draft id');
  assertLineAndTotals(createdDraft, 'created draft');

  const sendVariables: JsonRecord = { id: draftOrderId };
  sendResponse = await cap.run(sendDocument, sendVariables, 'send no-recipient invoice');
  const sendRoot = mutationRootAllowingUserErrors(sendResponse, 'draftOrderInvoiceSend', 'send no-recipient invoice');
  const sentDraft = draftOrderFromRoot(sendRoot, 'send no-recipient invoice');
  assertLineAndTotals(sentDraft, 'send no-recipient invoice');

  const userErrors = readArray(sendRoot['userErrors']);
  const firstUserError = readRecord(userErrors[0]);
  if (firstUserError?.['message'] !== "To can't be blank") {
    throw new Error(`send no-recipient invoice unexpected userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }

  const cleanupResult = await cap.runGraphqlRequest(deleteMutation, { input: { id: draftOrderId } });
  cleanup = { status: cleanupResult.status, response: cleanupResult.payload };

  await cap.writeJson(fixturePath, {
    scenarioId: 'draft-order-invoice-send-created-no-recipient',
    apiVersion: cap.apiVersion,
    storeDomain: cap.storeDomain,
    recordedAt: new Date().toISOString(),
    source: 'live-shopify-admin-graphql',
    safetyPolicy:
      'Creates a disposable draft order without a recipient, sends draftOrderInvoiceSend without an email argument so Shopify returns validation userErrors instead of sending customer-visible email, then deletes the draft order during cleanup.',
    create: {
      variables: createVariables,
      response: createResponse,
    },
    send: {
      variables: sendVariables,
      response: sendResponse,
    },
    cleanup: {
      draftOrderDelete: cleanup,
    },
    upstreamCalls: [shopPricingHydrate],
  });

  console.log(
    JSON.stringify(
      {
        fixturePath,
        draftOrderId,
        createStatus: createdDraft['status'],
        sendStatus: sentDraft['status'],
        userErrors: userErrors.length,
        cleanupStatus: cleanupResult.status,
      } satisfies JsonRecord,
      null,
      2,
    ),
  );
} finally {
  if (draftOrderId && cleanup === null) {
    const cleanupResult = await cap.runGraphqlRequest(deleteMutation, { input: { id: draftOrderId } });
    console.error(
      JSON.stringify(
        {
          cleanupAfterError: {
            draftOrderId,
            status: cleanupResult.status,
            response: cleanupResult.payload,
          },
        } satisfies JsonRecord,
        null,
        2,
      ),
    );
  }
}
