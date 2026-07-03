/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import { createConformanceCapture, readRecord, requireString, type JsonRecord } from './conformance-capture-lib.js';

const cap = await createConformanceCapture();
const fixturePath = cap.fixturePath('orders', 'draft-order-invoice-send-status-transition.json');

const createDocument = await cap.readRequest('orders', 'draftOrderInvoiceSend-status-transition-create.graphql');
const sendDocument = await cap.readRequest('orders', 'draftOrderInvoiceSend-status-transition-send.graphql');
const readDocument = await cap.readRequest('orders', 'draftOrderInvoiceSend-status-transition-read.graphql');

const deleteMutation = `#graphql
  mutation DraftOrderInvoiceSendStatusTransitionDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

function draftOrderFromMutation(payload: JsonRecord, rootName: string, label: string): JsonRecord {
  const root = cap.mutationRoot(payload, rootName, label);
  const draftOrder = readRecord(root['draftOrder']);
  if (!draftOrder) {
    throw new Error(`${label} missing draftOrder: ${JSON.stringify(payload, null, 2)}`);
  }
  return draftOrder;
}

function draftOrderFromRead(payload: JsonRecord, label: string): JsonRecord {
  const draftOrder = readRecord(readRecord(payload['data'])?.['draftOrder']);
  if (!draftOrder) {
    throw new Error(`${label} missing draftOrder: ${JSON.stringify(payload, null, 2)}`);
  }
  return draftOrder;
}

function assertStatus(draftOrder: JsonRecord, expected: string, label: string): void {
  if (draftOrder['status'] !== expected) {
    throw new Error(`${label} expected status ${expected}: ${JSON.stringify(draftOrder, null, 2)}`);
  }
}

function assertInvoiceSentAt(draftOrder: JsonRecord, label: string): string {
  return requireString(draftOrder['invoiceSentAt'], `${label} invoiceSentAt`);
}

const email = `draft-order-invoice-transition-${cap.stamp}@example.com`;
const createVariables: JsonRecord = {
  input: {
    email,
    tags: ['draft-proxy-capture', 'invoice-status-transition', cap.stamp],
    lineItems: [
      {
        title: `Invoice status transition item ${cap.stamp}`,
        quantity: 1,
        originalUnitPrice: '1.00',
        requiresShipping: false,
        taxable: false,
      },
    ],
  },
};

let draftOrderId: string | null = null;
let createResponse: JsonRecord | null = null;
let sendResponse: JsonRecord | null = null;
let readAfterSendResponse: JsonRecord | null = null;
let cleanup: JsonRecord | null = null;

try {
  createResponse = await cap.run(createDocument, createVariables, 'create invoice-send status draft');
  const createdDraft = draftOrderFromMutation(createResponse, 'draftOrderCreate', 'create invoice-send status draft');
  draftOrderId = requireString(createdDraft['id'], 'created draft id');
  assertStatus(createdDraft, 'OPEN', 'created draft');

  const sendVariables: JsonRecord = { id: draftOrderId };
  sendResponse = await cap.run(sendDocument, sendVariables, 'send invoice for status transition');
  const sentDraft = draftOrderFromMutation(sendResponse, 'draftOrderInvoiceSend', 'send invoice for status transition');
  assertStatus(sentDraft, 'INVOICE_SENT', 'invoice-send payload draft');
  const sentAt = assertInvoiceSentAt(sentDraft, 'invoice-send payload draft');

  const readVariables: JsonRecord = { id: draftOrderId };
  readAfterSendResponse = await cap.run(readDocument, readVariables, 'read draft after invoice send');
  const readDraft = draftOrderFromRead(readAfterSendResponse, 'read draft after invoice send');
  assertStatus(readDraft, 'INVOICE_SENT', 'read-back draft');
  const readSentAt = assertInvoiceSentAt(readDraft, 'read-back draft');
  if (readSentAt !== sentAt) {
    throw new Error(`read-back invoiceSentAt did not match send payload: ${sentAt} vs ${readSentAt}`);
  }

  const cleanupResult = await cap.runGraphqlRequest(deleteMutation, { input: { id: draftOrderId } });
  cleanup = { status: cleanupResult.status, response: cleanupResult.payload };

  await cap.writeJson(fixturePath, {
    scenarioId: 'draft-order-invoice-send-status-transition',
    apiVersion: cap.apiVersion,
    storeDomain: cap.storeDomain,
    recordedAt: new Date().toISOString(),
    source: 'live-shopify-admin-graphql',
    safetyPolicy:
      'Creates a disposable draft order with a reserved example.com recipient, sends the invoice once to capture Shopify state transition behavior, reads the draft back, then deletes the draft order during cleanup.',
    create: {
      variables: createVariables,
      response: createResponse,
    },
    send: {
      variables: sendVariables,
      response: sendResponse,
    },
    readAfterSend: {
      variables: readVariables,
      response: readAfterSendResponse,
    },
    cleanup: {
      draftOrderDelete: cleanup,
    },
    upstreamCalls: [],
  });

  console.log(
    JSON.stringify(
      {
        fixturePath,
        draftOrderId,
        status: {
          send: sentDraft['status'],
          readAfterSend: readDraft['status'],
        },
        invoiceSentAtPresent: sentAt.length > 0 && readSentAt.length > 0,
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
