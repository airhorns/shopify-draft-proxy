/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

// De-seeded re-record of the draftOrderInvoiceSend no-email safety scenario.
//
// Every branch the proxy resolves now arrives via a cold draft-order hydrate
// (DRAFT_ORDER_HYDRATE_QUERY forwarded on a miss), not a /__meta/seed pre-stage,
// so each case records the full-query hydrate cassette the proxy actually emits.
// SAFETY: no branch sends a customer-visible invoice email — validation uses an
// unknown/deleted id, and the live drafts are created without any recipient, so
// every live draftOrderInvoiceSend returns a userError instead of dispatching mail.
const cap = await createConformanceCapture();
const fixturePath = cap.fixturePath('orders', 'draft-order-invoice-send-safety.json');

// The exact draft-order hydrate query the proxy forwards on a cold invoiceSend,
// read verbatim from the shared .graphql so the recorded cassette byte-matches
// the proxy's include_str! constant.
const draftOrderHydrateQuery = await cap.readRequestRaw('orders', 'draft-order-hydrate.graphql');
// The proxy request document under test — run live to capture the ground-truth
// userErrors for each safety branch.
const invoiceSendDocument = await cap.readRequest('orders', 'draftOrderInvoiceSend-parity-plan.graphql');

const draftOrderCreateMutation = `#graphql
  mutation DraftOrderInvoiceSafetyCaptureCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        status
        email
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderCompleteMutation = `#graphql
  mutation DraftOrderInvoiceSafetyCaptureComplete($id: ID!) {
    draftOrderComplete(id: $id, paymentPending: true) {
      draftOrder {
        id
        status
        order {
          id
          name
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderDeleteMutation = `#graphql
  mutation DraftOrderInvoiceSafetyCaptureDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const orderCancelMutation = `#graphql
  mutation DraftOrderInvoiceSafetyCaptureCancel(
    $orderId: ID!
    $reason: OrderCancelReason!
    $refund: Boolean!
    $restock: Boolean!
    $notifyCustomer: Boolean
  ) {
    orderCancel(
      orderId: $orderId
      reason: $reason
      refund: $refund
      restock: $restock
      notifyCustomer: $notifyCustomer
    ) {
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

// A disposable draft mirroring the original safety precondition: a real draft on
// harry-test-heelo with NO recipient email, so an invoice send can never reach a
// customer. `label` only differentiates the throwaway notes/tags.
function safetyDraftInput(label: string): JsonRecord {
  return {
    note: `HAR-275 draft order invoice safety ${label}`,
    tags: ['parity-capture', 'draft-order-family', 'invoice-safety', label],
    lineItems: [
      {
        title: `HAR-275 invoice safety item ${label}`,
        quantity: 1,
        originalUnitPrice: '1.00',
        requiresShipping: false,
        taxable: false,
      },
    ],
  };
}

async function createSafetyDraft(label: string): Promise<string> {
  const payload = await cap.run(draftOrderCreateMutation, { input: safetyDraftInput(label) }, `create ${label}`);
  const root = cap.mutationRoot(payload, 'draftOrderCreate', `create ${label}`);
  return requireString(readRecord(root['draftOrder'])?.['id'], `created ${label} draft id`);
}

// Forward the proxy's full hydrate query live and shape the recorded cassette.
async function hydrateCassette(id: string): Promise<JsonRecord> {
  const hydratePayload = await cap.run(draftOrderHydrateQuery, { id }, `hydrate ${id}`);
  return {
    operationName: 'OrdersDraftOrderHydrate',
    variables: { id },
    query: draftOrderHydrateQuery,
    response: {
      status: 200,
      body: hydratePayload,
    },
  } satisfies JsonRecord;
}

// Run the live invoiceSend under test and return its raw payload (these branches
// intentionally return userErrors, so we keep the full payload verbatim).
async function liveInvoiceSend(variables: JsonRecord, label: string): Promise<JsonRecord> {
  const result = await cap.runGraphqlRequest<JsonRecord>(invoiceSendDocument, variables);
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`invoiceSend ${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return result.payload as JsonRecord;
}

const upstreamCalls: JsonRecord[] = [];

// ── Case 1: unknown draft order id (validation, never existed) ───────────────
const unknownVariables: JsonRecord = {
  id: 'gid://shopify/DraftOrder/999999999999999',
  email: {
    to: 'draft-order-invoice-safety@example.com',
    subject: 'HAR-275 unknown draft order safety probe',
    customMessage: 'Unknown-id validation should not send an invoice email.',
  },
};
upstreamCalls.push(await hydrateCassette(requireString(unknownVariables['id'], 'unknown id')));
const unknownResponse = await liveInvoiceSend(unknownVariables, 'unknown');

// ── Case 2: open draft with no recipient ─────────────────────────────────────
const openDraftId = await createSafetyDraft('open-no-recipient');
upstreamCalls.push(await hydrateCassette(openDraftId));
const openVariables: JsonRecord = { id: openDraftId };
const openResponse = await liveInvoiceSend(openVariables, 'open-no-recipient');

// ── Case 3: deleted draft order (existed, then removed) ──────────────────────
const deletedDraftId = await createSafetyDraft('deleted');
const deleteCleanup = await cap.run(
  draftOrderDeleteMutation,
  { input: { id: deletedDraftId } },
  'delete deleted-case draft',
);
cap.mutationRoot(deleteCleanup, 'draftOrderDelete', 'delete deleted-case draft');
// Hydrate AFTER deletion so the recorded cassette reflects draftOrder: null.
upstreamCalls.push(await hydrateCassette(deletedDraftId));
const deletedVariables: JsonRecord = { id: deletedDraftId };
const deletedResponse = await liveInvoiceSend(deletedVariables, 'deleted');

// ── Case 4: completed draft with no recipient ────────────────────────────────
const completedDraftId = await createSafetyDraft('completed-no-recipient');
const completePayload = await cap.run(
  draftOrderCompleteMutation,
  { id: completedDraftId },
  'complete completed-case draft',
);
const completeRoot = cap.mutationRoot(completePayload, 'draftOrderComplete', 'complete completed-case draft');
const completedOrderId = readRecord(readRecord(completeRoot['draftOrder'])?.['order'])?.['id'];
upstreamCalls.push(await hydrateCassette(completedDraftId));
const completedVariables: JsonRecord = { id: completedDraftId };
const completedResponse = await liveInvoiceSend(completedVariables, 'completed-no-recipient');

// ── Cleanup ──────────────────────────────────────────────────────────────────
// Open no-recipient draft: delete the disposable draft. Best-effort.
const openCleanup = await cap.runGraphqlRequest(draftOrderDeleteMutation, { input: { id: openDraftId } });
// Completed draft produced a real order; cancel it (no customer notification,
// no refund/restock) so the live store is left clean. Best-effort.
let completedOrderCancelStatus: number | null = null;
if (typeof completedOrderId === 'string') {
  const cancel = await cap.runGraphqlRequest(orderCancelMutation, {
    orderId: completedOrderId,
    reason: 'OTHER',
    refund: false,
    restock: false,
    notifyCustomer: false,
  });
  completedOrderCancelStatus = cancel.status;
}

await cap.writeJson(fixturePath, {
  scenarioId: 'draft-order-invoice-send-safety',
  apiVersion: cap.apiVersion,
  storeDomain: cap.storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  safetyPolicy:
    'All captured invoice-send branches avoid sending customer-visible email: validation uses unknown/deleted draft ids, and every live draft order is created without a recipient, so each live draftOrderInvoiceSend returns a userError instead of dispatching mail. The proxy resolves each branch by forwarding a cold draft-order hydrate (recorded below), never via /__meta/seed.',
  validation: {
    unknownDraftOrder: {
      variables: unknownVariables,
      response: unknownResponse,
    },
  },
  recipient: {
    openNoRecipient: {
      variables: openVariables,
      mutation: { response: openResponse },
    },
  },
  lifecycle: {
    deletedDraftOrder: {
      variables: deletedVariables,
      mutation: { response: deletedResponse },
    },
    completedNoRecipient: {
      variables: completedVariables,
      mutation: { response: completedResponse },
    },
  },
  upstreamCalls,
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      openDraftId,
      deletedDraftId,
      completedDraftId,
      completedOrderId: typeof completedOrderId === 'string' ? completedOrderId : null,
      openCleanupStatus: openCleanup.status,
      completedOrderCancelStatus,
      upstreamCalls: upstreamCalls.length,
      // Surfaced so a human can eyeball that no branch sent mail.
      userErrorSummary: {
        unknown: readArray(readRecord(readRecord(unknownResponse['data'])?.['draftOrderInvoiceSend'])?.['userErrors'])
          .length,
        open: readArray(readRecord(readRecord(openResponse['data'])?.['draftOrderInvoiceSend'])?.['userErrors']).length,
        deleted: readArray(readRecord(readRecord(deletedResponse['data'])?.['draftOrderInvoiceSend'])?.['userErrors'])
          .length,
        completed: readArray(
          readRecord(readRecord(completedResponse['data'])?.['draftOrderInvoiceSend'])?.['userErrors'],
        ).length,
      },
    } satisfies JsonRecord,
    null,
    2,
  ),
);
