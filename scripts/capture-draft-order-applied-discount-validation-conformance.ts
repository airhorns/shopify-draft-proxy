import { createConformanceCapture, readArray, readRecord, type JsonRecord } from './conformance-capture-lib.js';
import { captureDraftProxyShopPricingHydrate } from './support/shopify/runtime-hydration-capture.js';

type CaptureEntry = {
  variables: JsonRecord;
  response: JsonRecord;
};

const cap = await createConformanceCapture();
const shopPricingHydrate = await captureDraftProxyShopPricingHydrate((query, variables) =>
  cap.runGraphqlRequest(query, variables),
);

function discount(value: number, valueType?: 'PERCENTAGE' | 'FIXED_AMOUNT'): JsonRecord {
  const appliedDiscount: JsonRecord = {
    title: `Applied discount validation ${cap.stamp}`,
    description: 'Draft order applied discount validation capture',
    value,
    amount: Math.max(0, Math.min(10, value)),
  };
  if (valueType) appliedDiscount['valueType'] = valueType;
  return appliedDiscount;
}

function invalidValueTypeDiscount(): JsonRecord {
  return {
    ...discount(10),
    valueType: 'BOGUS',
  };
}

function lineItem(appliedDiscount?: JsonRecord): JsonRecord {
  return {
    title: `Applied discount item ${cap.stamp}`,
    quantity: 1,
    originalUnitPrice: '10.00',
    requiresShipping: false,
    ...(appliedDiscount ? { appliedDiscount } : {}),
  };
}

function inputWithOrderDiscount(appliedDiscount: JsonRecord, label: string): JsonRecord {
  return {
    email: `draft-discount-${label}-${cap.stamp}@example.com`,
    lineItems: [lineItem()],
    appliedDiscount,
  };
}

function inputWithLineDiscount(appliedDiscount: JsonRecord, label: string): JsonRecord {
  return {
    email: `draft-discount-${label}-${cap.stamp}@example.com`,
    lineItems: [lineItem(appliedDiscount)],
  };
}

function validationVariables(): JsonRecord {
  return {
    orderPercentageAboveMax: inputWithOrderDiscount(discount(150, 'PERCENTAGE'), 'order-above-max'),
    orderPercentageNegative: inputWithOrderDiscount(discount(-5, 'PERCENTAGE'), 'order-negative'),
    orderValueTooPrecise: inputWithOrderDiscount(discount(12.345, 'PERCENTAGE'), 'order-precision'),
    linePercentageAboveMax: inputWithLineDiscount(discount(150, 'PERCENTAGE'), 'line-above-max'),
    linePercentageNegative: inputWithLineDiscount(discount(-5, 'PERCENTAGE'), 'line-negative'),
    lineValueTooPrecise: inputWithLineDiscount(discount(12.345, 'PERCENTAGE'), 'line-precision'),
    validOrderPercentage: inputWithOrderDiscount(discount(12.34, 'PERCENTAGE'), 'order-valid'),
    validLinePercentage: inputWithLineDiscount(discount(12.34, 'PERCENTAGE'), 'line-valid'),
  };
}

async function capture(document: string, variables: JsonRecord): Promise<CaptureEntry> {
  const result = await cap.runGraphqlRequest<JsonRecord>(document, variables);
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`Capture request failed: ${JSON.stringify(result, null, 2)}`);
  }
  return { variables, response: result.payload as JsonRecord };
}

async function captureGraphqlErrors(document: string, variables: JsonRecord): Promise<CaptureEntry> {
  const result = await cap.runGraphqlRequest<JsonRecord>(document, variables);
  if (result.status < 200 || result.status >= 300 || !result.payload.errors) {
    throw new Error(`Expected GraphQL errors: ${JSON.stringify(result, null, 2)}`);
  }
  return { variables, response: result.payload as JsonRecord };
}

function draftOrderIdFromRoot(response: JsonRecord, rootName: string): string | null {
  const root = readRecord(readRecord(response['data'])?.[rootName]);
  const draftOrder = readRecord(root?.['draftOrder']);
  const id = draftOrder?.['id'];
  return typeof id === 'string' && id.length > 0 ? id : null;
}

function collectCreatedDraftOrderIds(response: JsonRecord, rootNames: string[]): string[] {
  return rootNames
    .map((rootName) => draftOrderIdFromRoot(response, rootName))
    .filter((id): id is string => id !== null);
}

function assertNoUserErrors(response: JsonRecord, rootNames: string[], payloadName: string): void {
  for (const rootName of rootNames) {
    const root = readRecord(readRecord(response['data'])?.[rootName]);
    const userErrors = readArray(root?.['userErrors']);
    if (userErrors.length !== 0) {
      throw new Error(`Expected ${payloadName}.${rootName} to pass: ${JSON.stringify(root, null, 2)}`);
    }
  }
}

const setupDocument = await cap.readRequest('orders', 'draftOrder-applied-discount-validation-setup.graphql');
const createDocument = await cap.readRequest('orders', 'draftOrder-applied-discount-validation-create.graphql');
const updateDocument = await cap.readRequest('orders', 'draftOrder-applied-discount-validation-update.graphql');
const calculateDocument = await cap.readRequest('orders', 'draftOrder-applied-discount-validation-calculate.graphql');
const missingValueTypeDocument = await cap.readRequest(
  'orders',
  'draftOrder-applied-discount-value-type-required.graphql',
);

const deleteDocument = `#graphql
  mutation DraftOrderAppliedDiscountValidationCleanup($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const invalidRootNames = [
  'orderPercentageAboveMax',
  'orderPercentageNegative',
  'orderValueTooPrecise',
  'linePercentageAboveMax',
  'linePercentageNegative',
  'lineValueTooPrecise',
];
const validRootNames = ['validOrderPercentage', 'validLinePercentage'];
const createdDraftOrderIds: string[] = [];

let setupCreate: CaptureEntry | null = null;
let createValidation: CaptureEntry | null = null;
let updateValidation: CaptureEntry | null = null;
let calculateValidation: CaptureEntry | null = null;
let missingValueTypeValidation: CaptureEntry | null = null;

try {
  setupCreate = await capture(setupDocument, {
    input: {
      email: `draft-discount-setup-${cap.stamp}@example.com`,
      lineItems: [lineItem()],
    },
  });
  const setupDraftOrderId = draftOrderIdFromRoot(setupCreate.response, 'draftOrderCreate');
  if (!setupDraftOrderId) {
    throw new Error(`Setup draftOrderCreate did not return an id: ${JSON.stringify(setupCreate, null, 2)}`);
  }
  createdDraftOrderIds.push(setupDraftOrderId);

  createValidation = await capture(createDocument, validationVariables());
  createdDraftOrderIds.push(
    ...collectCreatedDraftOrderIds(createValidation.response, [...invalidRootNames, ...validRootNames]),
  );
  assertNoUserErrors(createValidation.response, validRootNames, 'createValidation');

  updateValidation = await capture(updateDocument, {
    id: setupDraftOrderId,
    ...validationVariables(),
  });
  assertNoUserErrors(updateValidation.response, validRootNames, 'updateValidation');

  calculateValidation = await capture(calculateDocument, validationVariables());
  assertNoUserErrors(calculateValidation.response, validRootNames, 'calculateValidation');

  missingValueTypeValidation = await captureGraphqlErrors(missingValueTypeDocument, {
    id: setupDraftOrderId,
    createOrderMissingValueType: inputWithOrderDiscount(discount(10), 'create-order-missing-type'),
    createLineMissingValueType: inputWithLineDiscount(discount(10), 'create-line-missing-type'),
    updateOrderMissingValueType: inputWithOrderDiscount(discount(10), 'update-order-missing-type'),
    updateLineMissingValueType: inputWithLineDiscount(discount(10), 'update-line-missing-type'),
    calculateOrderMissingValueType: inputWithOrderDiscount(discount(10), 'calculate-order-missing-type'),
    calculateLineMissingValueType: inputWithLineDiscount(discount(10), 'calculate-line-missing-type'),
    createOrderInvalidValueType: inputWithOrderDiscount(invalidValueTypeDiscount(), 'create-order-invalid-type'),
    createLineInvalidValueType: inputWithLineDiscount(invalidValueTypeDiscount(), 'create-line-invalid-type'),
    updateOrderInvalidValueType: inputWithOrderDiscount(invalidValueTypeDiscount(), 'update-order-invalid-type'),
    updateLineInvalidValueType: inputWithLineDiscount(invalidValueTypeDiscount(), 'update-line-invalid-type'),
    calculateOrderInvalidValueType: inputWithOrderDiscount(invalidValueTypeDiscount(), 'calculate-order-invalid-type'),
    calculateLineInvalidValueType: inputWithLineDiscount(invalidValueTypeDiscount(), 'calculate-line-invalid-type'),
  });

  const fixturePath = cap.fixturePath('orders', 'draftOrder-applied-discount-validation.json');
  await cap.writeJson(fixturePath, {
    metadata: {
      storeDomain: cap.storeDomain,
      apiVersion: cap.apiVersion,
      capturedAt: new Date().toISOString(),
      description:
        'DraftOrderAppliedDiscountInput value/valueType validation for draftOrderCreate, draftOrderUpdate, and draftOrderCalculate.',
    },
    invalidRootNames,
    validRootNames,
    setupCreate,
    createValidation,
    updateValidation,
    calculateValidation,
    missingValueTypeValidation,
    upstreamCalls: [shopPricingHydrate],
  });

  process.stdout.write(`${JSON.stringify({ fixturePath }, null, 2)}\n`);
} finally {
  const uniqueDraftOrderIds = [...new Set(createdDraftOrderIds)];
  await Promise.allSettled(
    uniqueDraftOrderIds.map((id) =>
      cap.runGraphqlRequest(deleteDocument, {
        input: { id },
      }),
    ),
  );
}
