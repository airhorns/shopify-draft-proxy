/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

const cap = await createConformanceCapture();
const fixturePath = cap.fixturePath('orders', 'draft-order-complete-invalid-payment-gateway.json');

const invalidGatewayDocument = await cap.readRequest('orders', 'draftOrderComplete-invalid-payment-gateway.graphql');
const codeSelectionDocument = await cap.readRequest('orders', 'draftOrderComplete-user-error-code-selection.graphql');

const draftOrderCreateMutation = `#graphql
  mutation DraftOrderCompleteInvalidGatewayCreate($input: DraftOrderInput!) {
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
  mutation DraftOrderCompleteInvalidGatewayCleanup($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertInvalidGatewayPayload(payload: JsonRecord, draftOrderId: string): void {
  const root = readRecord(readRecord(payload['data'])?.['draftOrderComplete']);
  if (!root) {
    throw new Error(`draftOrderComplete missing payload: ${JSON.stringify(payload, null, 2)}`);
  }
  const draftOrder = readRecord(root['draftOrder']);
  if (
    !draftOrder ||
    draftOrder['id'] !== draftOrderId ||
    draftOrder['status'] !== 'OPEN' ||
    draftOrder['order'] !== null
  ) {
    throw new Error(`invalid gateway should return the still-open draftOrder: ${JSON.stringify(root, null, 2)}`);
  }
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length !== 1) {
    throw new Error(`invalid gateway should return one userError: ${JSON.stringify(root, null, 2)}`);
  }
  const error = readRecord(userErrors[0]);
  if (!error || error['field'] !== null || error['message'] !== 'Invalid payment gateway' || 'code' in error) {
    throw new Error(`invalid gateway userError shape mismatch: ${JSON.stringify(error, null, 2)}`);
  }
}

function assertCodeSelectionRejection(payload: JsonRecord): void {
  const errors = readArray(payload['errors']);
  if (errors.length !== 1) {
    throw new Error(`code selection should return one top-level error: ${JSON.stringify(payload, null, 2)}`);
  }
  const error = readRecord(errors[0]);
  const extensions = readRecord(error?.['extensions']);
  if (
    error?.['message'] !== "Field 'code' doesn't exist on type 'UserError'" ||
    extensions?.['code'] !== 'undefinedField' ||
    extensions?.['typeName'] !== 'UserError' ||
    extensions?.['fieldName'] !== 'code'
  ) {
    throw new Error(`code selection error mismatch: ${JSON.stringify(error, null, 2)}`);
  }
}

const createInput = {
  email: `draft-complete-invalid-gateway-${cap.stamp}@example.com`,
  note: 'invalid payment gateway draft order complete parity probe',
  tags: ['draft-order-complete', 'invalid-payment-gateway', 'parity-probe'],
  taxExempt: true,
  lineItems: [
    {
      title: 'Invalid gateway service',
      quantity: 1,
      originalUnitPrice: '10.00',
      requiresShipping: false,
      taxable: false,
      sku: `invalid-gateway-${cap.stamp}`,
    },
  ],
};

const paymentGatewayId = 'gid://shopify/PaymentGateway/999999999999';
let draftOrderId: string | null = null;
let cleanupPayload: JsonRecord | null = null;

try {
  const setupVariables = { input: createInput };
  const createPayload = await cap.run(draftOrderCreateMutation, setupVariables, 'draftOrderCreate');
  const createRoot = cap.mutationRoot(createPayload, 'draftOrderCreate', 'draftOrderCreate');
  draftOrderId = requireString(readRecord(createRoot['draftOrder'])?.['id'], 'created draft order id');

  const variables = {
    id: draftOrderId,
    paymentGatewayId,
    paymentPending: false,
  };
  const invalidGatewayPayload = await cap.run(invalidGatewayDocument, variables, 'draftOrderComplete invalid gateway');
  assertInvalidGatewayPayload(invalidGatewayPayload, draftOrderId);

  const codeSelectionResult = await cap.runGraphqlRequest(codeSelectionDocument, {
    id: draftOrderId,
    paymentGatewayId,
  });
  assertCodeSelectionRejection(codeSelectionResult.payload ?? {});

  cleanupPayload = (
    await cap.runGraphqlRequest(draftOrderDeleteMutation, {
      input: { id: draftOrderId },
    })
  ).payload;
  draftOrderId = null;

  await cap.writeJson(fixturePath, {
    scenarioId: 'draft-order-complete-invalid-payment-gateway',
    apiVersion: cap.apiVersion,
    storeDomain: cap.storeDomain,
    recordedAt: new Date().toISOString(),
    source: 'live-shopify-admin-graphql',
    notes:
      'Live draft-order completion capture for a paymentGatewayId that does not resolve to an enabled manual payment gateway. Shopify returns a base UserError with field null and message Invalid payment gateway. A separate operation selecting userErrors.code is rejected by the public schema because draftOrderComplete.userErrors is plain UserError.',
    setup: {
      draftOrderCreate: {
        query: draftOrderCreateMutation,
        variables: setupVariables,
        response: createPayload,
      },
    },
    variables,
    mutation: {
      query: invalidGatewayDocument,
      response: invalidGatewayPayload,
    },
    codeSelection: {
      query: codeSelectionDocument,
      variables: { id: draftOrderId ?? variables.id, paymentGatewayId },
      response: codeSelectionResult.payload,
    },
    cleanup: {
      draftOrderDelete: {
        query: draftOrderDeleteMutation,
        response: cleanupPayload,
      },
    },
    upstreamCalls: [],
  });

  console.log(
    JSON.stringify({ fixturePath, draftOrderId: variables.id, paymentGatewayId } satisfies JsonRecord, null, 2),
  );
} finally {
  if (draftOrderId) {
    await cap.runGraphqlRequest(draftOrderDeleteMutation, { input: { id: draftOrderId } });
  }
}
