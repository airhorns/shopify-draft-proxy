/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const outputPath = path.join(outputDir, 'payment-terms-create-missing-template-id.json');
const createMissingDocumentPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'payment-terms-create-missing-template-id.graphql',
);
const updateSetupDocumentPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'payment-terms-update-missing-template-id-setup.graphql',
);
const updateMissingDocumentPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'payment-terms-update-missing-template-id.graphql',
);
const createMissingDocument = await readFile(createMissingDocumentPath, 'utf8');
const updateSetupDocument = await readFile(updateSetupDocumentPath, 'utf8');
const updateMissingDocument = await readFile(updateMissingDocumentPath, 'utf8');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertCreateMissingTemplateCoercion(result: ConformanceGraphqlResult): void {
  const errors = Array.isArray(result.payload.errors) ? result.payload.errors : [];
  if (result.status < 200 || result.status >= 300 || result.payload.data !== undefined || errors.length !== 1) {
    throw new Error(`Expected one top-level create coercion error and no data: ${JSON.stringify(result, null, 2)}`);
  }
  const error = readRecord(errors[0]);
  if (
    typeof error?.['message'] !== 'string' ||
    !error['message'].includes('paymentTermsTemplateId') ||
    readRecord(error['extensions'])?.['code'] !== 'INVALID_VARIABLE'
  ) {
    throw new Error(`Unexpected create missing-template coercion error: ${JSON.stringify(errors[0], null, 2)}`);
  }
}

function paymentTermsPayload(result: ConformanceGraphqlResult, root: string): JsonRecord {
  const data = readRecord(result.payload.data);
  const payload = readRecord(data?.[root]);
  if (!payload) {
    throw new Error(`${root} payload missing: ${JSON.stringify(result, null, 2)}`);
  }
  return payload;
}

const draftOrderCreateDocument = `#graphql
mutation PaymentTermsMissingTemplateDraftCreate($input: DraftOrderInput!) {
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

const paymentTermsDeleteDocument = `#graphql
mutation PaymentTermsMissingTemplateTermsCleanup($input: PaymentTermsDeleteInput!) {
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
mutation PaymentTermsMissingTemplateDraftCleanup($input: DraftOrderDeleteInput!) {
  draftOrderDelete(input: $input) {
    deletedId
    userErrors {
      field
      message
    }
  }
}
`;

await mkdir(outputDir, { recursive: true });

const createMissingVariables = {
  referenceId: 'gid://shopify/Order/1',
  attrs: {
    paymentSchedules: [{ issuedAt: '2026-01-01T00:00:00Z' }],
  },
};
const createMissingResponse = await runGraphqlRequest(createMissingDocument, createMissingVariables);
assertCreateMissingTemplateCoercion(createMissingResponse);

let draftOrderId: string | null = null;
let paymentTermsId: string | null = null;
const cleanup: JsonRecord = {};
const runId = Date.now();
const draftOrderVariables = {
  input: {
    email: `payment-terms-update-missing-template-${runId}@example.com`,
    lineItems: [
      {
        title: 'Payment terms update missing template',
        quantity: 1,
        originalUnitPrice: '12.00',
      },
    ],
  },
};
const setupVariables = {
  referenceId: '',
  attrs: {
    paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
    paymentSchedules: [{ issuedAt: '2026-01-01T00:00:00Z' }],
  },
};
const updateMissingVariables = {
  input: {
    paymentTermsId: '',
    paymentTermsAttributes: {
      paymentSchedules: [{ issuedAt: '2026-01-02T00:00:00Z' }],
    },
  },
};

try {
  const draftOrderCreate = await runGraphqlRequest(draftOrderCreateDocument, draftOrderVariables);
  assertNoTopLevelErrors(draftOrderCreate, 'draftOrderCreate setup');
  const draftOrderCreatePayload = paymentTermsPayload(draftOrderCreate, 'draftOrderCreate');
  const draftOrder = readRecord(draftOrderCreatePayload['draftOrder']);
  draftOrderId = typeof draftOrder?.['id'] === 'string' ? draftOrder['id'] : null;
  if (!draftOrderId) {
    throw new Error(`draftOrderCreate did not return an id: ${JSON.stringify(draftOrderCreate, null, 2)}`);
  }

  setupVariables.referenceId = draftOrderId;
  const setup = await runGraphqlRequest(updateSetupDocument, setupVariables);
  assertNoTopLevelErrors(setup, 'paymentTermsCreate setup');
  const setupPayload = paymentTermsPayload(setup, 'paymentTermsCreate');
  const createdTerms = readRecord(setupPayload['paymentTerms']);
  paymentTermsId = typeof createdTerms?.['id'] === 'string' ? createdTerms['id'] : null;
  if (!paymentTermsId) {
    throw new Error(`paymentTermsCreate setup did not return payment terms: ${JSON.stringify(setup, null, 2)}`);
  }

  updateMissingVariables.input.paymentTermsId = paymentTermsId;
  const updateMissing = await runGraphqlRequest(updateMissingDocument, updateMissingVariables);
  assertNoTopLevelErrors(updateMissing, 'paymentTermsUpdate missing template');
  const updatePayload = paymentTermsPayload(updateMissing, 'paymentTermsUpdate');
  const userErrors = Array.isArray(updatePayload['userErrors']) ? updatePayload['userErrors'] : [];
  if (userErrors.length !== 0) {
    throw new Error(
      `paymentTermsUpdate missing template returned userErrors: ${JSON.stringify(updateMissing, null, 2)}`,
    );
  }

  cleanup['paymentTermsDelete'] = (
    await runGraphqlRequest(paymentTermsDeleteDocument, { input: { paymentTermsId } })
  ).payload;
  paymentTermsId = null;
  cleanup['draftOrderDelete'] = (
    await runGraphqlRequest(draftOrderDeleteDocument, { input: { id: draftOrderId } })
  ).payload;
  draftOrderId = null;

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    cases: {
      createMissingTemplateId: {
        request: {
          query: createMissingDocument,
          variables: createMissingVariables,
        },
        response: createMissingResponse.payload,
      },
      updateMissingTemplateId: {
        setup: {
          draftOrderCreate: {
            query: draftOrderCreateDocument,
            variables: draftOrderVariables,
            response: draftOrderCreate.payload,
          },
          paymentTermsCreate: {
            query: updateSetupDocument,
            variables: setupVariables,
            response: setup.payload,
          },
        },
        request: {
          query: updateMissingDocument,
          variables: updateMissingVariables,
        },
        response: updateMissing.payload,
        cleanup,
      },
    },
    upstreamCalls: [],
    notes:
      'Live 2026-04 capture. paymentTermsCreate without paymentTermsTemplateId fails during variable coercion. paymentTermsUpdate uses nullable PaymentTermsInput.paymentTermsTemplateId, so omitting it on an existing Net 30 payment term succeeds and recomputes the schedule from the supplied issuedAt.',
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (paymentTermsId) {
    cleanup['paymentTermsDelete'] = (
      await runGraphqlRequest(paymentTermsDeleteDocument, { input: { paymentTermsId } })
    ).payload;
  }
  if (draftOrderId) {
    cleanup['draftOrderDelete'] = (
      await runGraphqlRequest(draftOrderDeleteDocument, { input: { id: draftOrderId } })
    ).payload;
  }
}
