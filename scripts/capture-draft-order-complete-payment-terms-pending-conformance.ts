/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  futureNoonIso,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

const cap = await createConformanceCapture();
const fixturePath = cap.fixturePath('orders', 'draft-order-complete-payment-terms-pending.json');

// Read verbatim from the shared request so the cassette matches the Rust hydrate query.
const draftOrderHydrateQuery = await cap.readRequestRaw('orders', 'draft-order-hydrate.graphql');
const completeDocument = await cap.readRequest('orders', 'draftOrderComplete-payment-terms-pending.graphql');
const orderReadDocument = await cap.readRequest(
  'orders',
  'draftOrderComplete-payment-terms-pending-order-read.graphql',
);

const draftOrderCreateMutation = `#graphql
  mutation DraftOrderCompletePaymentTermsCreateDraft($input: DraftOrderInput!) {
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

const paymentTermsCreateMutation = `#graphql
  mutation DraftOrderCompletePaymentTermsCreateTerms(
    $referenceId: ID!
    $attrs: PaymentTermsCreateInput!
  ) {
    paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
      paymentTerms {
        id
        paymentTermsName
        paymentTermsType
        dueInDays
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const paymentTermsDeleteMutation = `#graphql
  mutation DraftOrderCompletePaymentTermsCleanupTerms($input: PaymentTermsDeleteInput!) {
    paymentTermsDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const draftOrderDeleteMutation = `#graphql
  mutation DraftOrderCompletePaymentTermsCleanupDraft($input: DraftOrderDeleteInput!) {
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
  mutation DraftOrderCompletePaymentTermsCleanupOrder(
    $orderId: ID!
    $reason: OrderCancelReason!
    $notifyCustomer: Boolean!
    $restock: Boolean!
  ) {
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

function assertPendingOrder(value: unknown, label: string): void {
  const order = readRecord(value);
  if (!order) throw new Error(`${label} missing order payload`);
  if (order['displayFinancialStatus'] !== 'PENDING') {
    throw new Error(`${label} expected PENDING financial status: ${JSON.stringify(order, null, 2)}`);
  }
  if (
    !Array.isArray(order['paymentGatewayNames']) ||
    JSON.stringify(order['paymentGatewayNames']) !== JSON.stringify(['manual'])
  ) {
    throw new Error(`${label} expected manual payment gateway: ${JSON.stringify(order, null, 2)}`);
  }
  if (
    !Array.isArray(order['transactions']) ||
    order['transactions'].length !== 1 ||
    readRecord(order['transactions'][0])?.['kind'] !== 'SALE' ||
    readRecord(order['transactions'][0])?.['status'] !== 'PENDING' ||
    readRecord(order['transactions'][0])?.['gateway'] !== 'manual'
  ) {
    throw new Error(`${label} expected one manual SALE/PENDING transaction: ${JSON.stringify(order, null, 2)}`);
  }
}

const createInput = {
  email: `draft-complete-payment-terms-${cap.stamp}@example.com`,
  note: 'payment terms draft order complete parity probe',
  tags: ['draft-order-complete', 'payment-terms', 'parity-probe'],
  taxExempt: true,
  lineItems: [
    {
      title: 'Payment terms invoice service',
      quantity: 1,
      originalUnitPrice: '42.00',
      requiresShipping: false,
      taxable: false,
      sku: `payment-terms-complete-${cap.stamp}`,
    },
  ],
};

const paymentTermsCreateVariables = {
  referenceId: '',
  attrs: {
    paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
    paymentSchedules: [{ issuedAt: futureNoonIso(cap.now, 30) }],
  },
};

let draftOrderId: string | null = null;
let openDraftOrderId: string | null = null;
let paymentTermsId: string | null = null;
let completedOrderId: string | null = null;
const cleanup: JsonRecord = {};

try {
  const createVariables = { input: createInput };
  const createPayload = await cap.run(draftOrderCreateMutation, createVariables, 'draftOrderCreate');
  const createRoot = cap.mutationRoot(createPayload, 'draftOrderCreate', 'draftOrderCreate');
  draftOrderId = requireString(readRecord(createRoot['draftOrder'])?.['id'], 'created draft order id');
  openDraftOrderId = draftOrderId;

  paymentTermsCreateVariables.referenceId = draftOrderId;
  const paymentTermsCreatePayload = await cap.run(
    paymentTermsCreateMutation,
    paymentTermsCreateVariables,
    'paymentTermsCreate',
  );
  const paymentTermsCreateRoot = cap.mutationRoot(
    paymentTermsCreatePayload,
    'paymentTermsCreate',
    'paymentTermsCreate',
  );
  paymentTermsId = requireString(readRecord(paymentTermsCreateRoot['paymentTerms'])?.['id'], 'payment terms id');

  const hydratePayload = await cap.run(draftOrderHydrateQuery, { id: draftOrderId }, 'draftOrderHydrate');

  const completeVariables = { id: draftOrderId };
  const completePayload = await cap.run(completeDocument, completeVariables, 'draftOrderComplete');
  const completeRoot = cap.mutationRoot(completePayload, 'draftOrderComplete', 'draftOrderComplete');
  const completedDraft = readRecord(completeRoot['draftOrder']);
  if (completedDraft?.['status'] !== 'COMPLETED') {
    throw new Error(`draftOrderComplete expected COMPLETED draft: ${JSON.stringify(completedDraft, null, 2)}`);
  }
  const completedOrder = readRecord(completedDraft?.['order']);
  completedOrderId = requireString(completedOrder?.['id'], 'completed order id');
  assertPendingOrder(completedOrder, 'draftOrderComplete');
  openDraftOrderId = null;
  paymentTermsId = null;

  const orderReadVariables = { id: completedOrderId };
  const orderReadPayload = await cap.run(orderReadDocument, orderReadVariables, 'order read after completion');
  assertPendingOrder(readRecord(readRecord(orderReadPayload['data'])?.['order']), 'order(id:) read after completion');

  const cleanupVariables = {
    orderId: completedOrderId,
    reason: 'OTHER',
    notifyCustomer: false,
    restock: false,
  };
  cleanup['orderCancel'] = (await cap.runGraphqlRequest(orderCancelMutation, cleanupVariables)).payload;
  completedOrderId = null;

  await cap.writeJson(fixturePath, {
    scenarioId: 'draftOrderComplete-payment-terms-pending',
    apiVersion: cap.apiVersion,
    storeDomain: cap.storeDomain,
    recordedAt: new Date().toISOString(),
    source: 'live-shopify-admin-graphql',
    notes:
      'Live draft-order completion capture. A disposable custom-line draft has NET payment terms attached through paymentTermsCreate, then draftOrderComplete is called without paymentPending. Shopify leaves the completed order pending with one manual SALE/PENDING transaction and no successful payment capture.',
    setup: {
      draftOrderCreate: {
        query: draftOrderCreateMutation,
        variables: createVariables,
        response: createPayload,
      },
      paymentTermsCreate: {
        query: paymentTermsCreateMutation,
        variables: paymentTermsCreateVariables,
        response: paymentTermsCreatePayload,
      },
    },
    variables: completeVariables,
    mutation: { response: completePayload },
    orderRead: {
      query: orderReadDocument,
      variables: orderReadVariables,
      response: orderReadPayload,
    },
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
    cleanup,
  });

  console.log(JSON.stringify({ fixturePath, draftOrderId, completedOrderId: orderReadVariables.id }, null, 2));
} finally {
  if (completedOrderId) {
    cleanup['orderCancel'] = (
      await cap.runGraphqlRequest(orderCancelMutation, {
        orderId: completedOrderId,
        reason: 'OTHER',
        notifyCustomer: false,
        restock: false,
      })
    ).payload;
  }
  if (paymentTermsId) {
    cleanup['paymentTermsDelete'] = (
      await cap.runGraphqlRequest(paymentTermsDeleteMutation, { input: { paymentTermsId } })
    ).payload;
  }
  if (openDraftOrderId) {
    cleanup['draftOrderDelete'] = (
      await cap.runGraphqlRequest(draftOrderDeleteMutation, { input: { id: openDraftOrderId } })
    ).payload;
  }
}
