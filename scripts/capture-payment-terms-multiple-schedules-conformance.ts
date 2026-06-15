/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const scenarioId = 'payment-terms-multiple-schedules';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type JsonRecord = Record<string, unknown>;

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function paymentTermsCreatePayload(result: ConformanceGraphqlResult): JsonRecord | null {
  const data = readRecord(result.payload.data);
  return readRecord(data?.['paymentTermsCreate']);
}

function paymentTermsUpdatePayload(result: ConformanceGraphqlResult): JsonRecord | null {
  const data = readRecord(result.payload.data);
  return readRecord(data?.['paymentTermsUpdate']);
}

function requireCreatedPaymentTermsId(result: ConformanceGraphqlResult, context: string): string {
  const payload = paymentTermsCreatePayload(result);
  const paymentTerms = readRecord(payload?.['paymentTerms']);
  const id = readString(paymentTerms?.['id']);
  if (!id) {
    throw new Error(`${context} did not return paymentTerms.id: ${JSON.stringify(result, null, 2)}`);
  }
  return id;
}

function assertUserError(
  result: ConformanceGraphqlResult,
  rootField: 'paymentTermsCreate' | 'paymentTermsUpdate',
  expectedCode: string,
): void {
  const payload =
    rootField === 'paymentTermsCreate' ? paymentTermsCreatePayload(result) : paymentTermsUpdatePayload(result);
  if (!payload) {
    throw new Error(`${rootField} payload missing: ${JSON.stringify(result, null, 2)}`);
  }
  if (payload['paymentTerms'] !== null) {
    throw new Error(`${rootField} should return paymentTerms: null: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = Array.isArray(payload['userErrors']) ? payload['userErrors'] : [];
  if (userErrors.length !== 1) {
    throw new Error(`${rootField} should return one userError: ${JSON.stringify(payload, null, 2)}`);
  }
  const error = readRecord(userErrors[0]);
  if (
    error?.['field'] !== null ||
    error['message'] !== 'Cannot create payment terms with multiple payment schedules.' ||
    error['code'] !== expectedCode
  ) {
    throw new Error(`${rootField} returned unexpected userError: ${JSON.stringify(error, null, 2)}`);
  }
}

const draftOrderCreateDocument = `#graphql
  mutation PaymentTermsMultipleSchedulesDraftCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        name
        paymentTerms {
          id
        }
        subtotalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
        totalPriceSet {
          shopMoney {
            amount
            currencyCode
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const paymentTermsCreateDocument = `#graphql
  mutation PaymentTermsMultipleSchedulesCreate($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
    paymentTermsCreate(referenceId: $referenceId, paymentTermsAttributes: $attrs) {
      paymentTerms {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const paymentTermsUpdateDocument = `#graphql
  mutation PaymentTermsMultipleSchedulesUpdate($input: PaymentTermsUpdateInput!) {
    paymentTermsUpdate(input: $input) {
      paymentTerms {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const paymentTermsDeleteDocument = `#graphql
  mutation PaymentTermsMultipleSchedulesTermsCleanup($input: PaymentTermsDeleteInput!) {
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

const draftOrderDeleteDocument = `#graphql
  mutation PaymentTermsMultipleSchedulesDraftCleanup($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const issuedScheduleAttrs = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
  paymentSchedules: [{ issuedAt: '2026-05-05T00:00:00Z' }],
};
const multipleScheduleAttrs = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
  paymentSchedules: [{ issuedAt: '2026-05-05T00:00:00Z' }, { issuedAt: '2026-05-06T00:00:00Z' }],
};

await mkdir(outputDir, { recursive: true });

const runId = Date.now();
const draftOrderCreateVariables = {
  input: {
    email: `${scenarioId}-${runId}@example.com`,
    lineItems: [
      {
        title: 'Payment terms multiple schedules conformance',
        quantity: 1,
        originalUnitPrice: '18.50',
      },
    ],
  },
};

let draftOrderId: string | null = null;
let setupPaymentTermsId: string | null = null;
const cleanup: JsonRecord = {};

try {
  const draftOrderCreate = await runGraphqlRequest(draftOrderCreateDocument, draftOrderCreateVariables);
  assertNoTopLevelErrors(draftOrderCreate, 'draftOrderCreate setup');
  const draftOrderCreateData = readRecord(draftOrderCreate.payload.data);
  const draftOrderCreatePayload = readRecord(draftOrderCreateData?.['draftOrderCreate']);
  const draftOrder = readRecord(draftOrderCreatePayload?.['draftOrder']);
  draftOrderId = readString(draftOrder?.['id']);
  if (!draftOrderId) {
    throw new Error(`draftOrderCreate did not return draftOrder.id: ${JSON.stringify(draftOrderCreate, null, 2)}`);
  }

  const createVariables = {
    referenceId: draftOrderId,
    attrs: multipleScheduleAttrs,
  };
  const createMultipleSchedules = await runGraphqlRequest(paymentTermsCreateDocument, createVariables);
  assertNoTopLevelErrors(createMultipleSchedules, 'paymentTermsCreate multiple schedules');
  assertUserError(createMultipleSchedules, 'paymentTermsCreate', 'PAYMENT_TERMS_CREATION_UNSUCCESSFUL');

  const setupCreateVariables = {
    referenceId: draftOrderId,
    attrs: issuedScheduleAttrs,
  };
  const setupPaymentTermsCreate = await runGraphqlRequest(paymentTermsCreateDocument, setupCreateVariables);
  assertNoTopLevelErrors(setupPaymentTermsCreate, 'paymentTermsCreate setup');
  setupPaymentTermsId = requireCreatedPaymentTermsId(setupPaymentTermsCreate, 'paymentTermsCreate setup');
  const capturedSetupPaymentTermsId = setupPaymentTermsId;

  const updateVariables = {
    input: {
      paymentTermsId: setupPaymentTermsId,
      paymentTermsAttributes: multipleScheduleAttrs,
    },
  };
  const updateMultipleSchedules = await runGraphqlRequest(paymentTermsUpdateDocument, updateVariables);
  assertNoTopLevelErrors(updateMultipleSchedules, 'paymentTermsUpdate multiple schedules');
  assertUserError(updateMultipleSchedules, 'paymentTermsUpdate', 'PAYMENT_TERMS_UPDATE_UNSUCCESSFUL');

  const paymentTermsDelete = await runGraphqlRequest(paymentTermsDeleteDocument, {
    input: { paymentTermsId: setupPaymentTermsId },
  });
  cleanup['paymentTermsDelete'] = paymentTermsDelete.payload;
  assertNoTopLevelErrors(paymentTermsDelete, 'paymentTermsDelete cleanup');
  setupPaymentTermsId = null;

  const draftOrderDelete = await runGraphqlRequest(draftOrderDeleteDocument, { input: { id: draftOrderId } });
  cleanup['draftOrderDelete'] = draftOrderDelete.payload;
  assertNoTopLevelErrors(draftOrderDelete, 'draftOrderDelete cleanup');
  draftOrderId = null;

  const fixture = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    upstreamCalls: [],
    setup: {
      draftOrderCreate: {
        query: draftOrderCreateDocument,
        variables: draftOrderCreateVariables,
        response: draftOrderCreate.payload,
      },
      paymentTermsCreate: {
        query: paymentTermsCreateDocument,
        variables: setupCreateVariables,
        response: setupPaymentTermsCreate.payload,
      },
    },
    cases: {
      create: {
        purpose: 'paymentTermsCreate rejects more than one paymentSchedules entry with a base-scoped public userError.',
        query: paymentTermsCreateDocument,
        variables: createVariables,
        response: createMultipleSchedules.payload,
      },
      update: {
        purpose: 'paymentTermsUpdate rejects more than one paymentSchedules entry with the update unsuccessful code.',
        query: paymentTermsUpdateDocument,
        variables: updateVariables,
        response: updateMultipleSchedules.payload,
      },
    },
    expectedSetupPayload: {
      data: {
        paymentTermsCreate: {
          paymentTerms: {
            id: capturedSetupPaymentTermsId,
          },
          userErrors: [],
        },
      },
    },
    expectedSetupLog: [
      {
        operationName: 'paymentTerms',
        status: 'staged',
        stagedResourceIds: [capturedSetupPaymentTermsId],
      },
    ],
    cleanup,
    notes:
      'Captured on a disposable draft order. Shopify returns field: null and message "Cannot create payment terms with multiple payment schedules." for multiple paymentSchedules on both paymentTermsCreate and paymentTermsUpdate.',
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (setupPaymentTermsId) {
    cleanup['paymentTermsDelete'] = (
      await runGraphqlRequest(paymentTermsDeleteDocument, { input: { paymentTermsId: setupPaymentTermsId } })
    ).payload;
  }
  if (draftOrderId) {
    cleanup['draftOrderDelete'] = (
      await runGraphqlRequest(draftOrderDeleteDocument, { input: { id: draftOrderId } })
    ).payload;
  }
}
