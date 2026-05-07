/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const outputPath = path.join(outputDir, 'payment-terms-create-reference-not-found.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

const paymentTermsCreateDocument = `#graphql
  mutation PaymentTermsReferenceNotFound($referenceId: ID!, $attrs: PaymentTermsCreateInput!) {
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

const attrs = {
  paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
  paymentSchedules: [{ issuedAt: '2026-05-05T00:00:00Z' }],
};

const cases = {
  missingOrder: {
    purpose: 'Unknown Order reference returns a field-null paymentTermsCreate userError with the numeric GID tail.',
    variables: {
      referenceId: 'gid://shopify/Order/123',
      attrs,
    },
    expectedMessage: 'Cannot find the specific Order with id 123.',
  },
  missingDraftOrder: {
    purpose:
      'Unknown DraftOrder reference returns a field-null paymentTermsCreate userError with the numeric GID tail.',
    variables: {
      referenceId: 'gid://shopify/DraftOrder/999999',
      attrs,
    },
    expectedMessage: 'Cannot find the specific Draft order with id 999999.',
  },
} as const;

async function captureCase(name: keyof typeof cases) {
  const spec = cases[name];
  const response = await runGraphqlRequest(paymentTermsCreateDocument, spec.variables);
  assertNoTopLevelErrors(response, `${name} paymentTermsCreate`);

  const responseData = readRecord(response.payload.data);
  const payload = readRecord(responseData?.['paymentTermsCreate']);
  const userErrors = Array.isArray(payload?.['userErrors']) ? payload['userErrors'] : [];
  const firstError = readRecord(userErrors[0]);
  if (
    !firstError ||
    firstError['field'] !== null ||
    firstError['message'] !== spec.expectedMessage ||
    firstError['code'] !== 'PAYMENT_TERMS_CREATION_UNSUCCESSFUL' ||
    payload?.['paymentTerms'] !== null
  ) {
    throw new Error(`${name} returned unexpected payload: ${JSON.stringify(response.payload, null, 2)}`);
  }

  return {
    purpose: spec.purpose,
    query: paymentTermsCreateDocument,
    variables: spec.variables,
    response: response.payload,
  };
}

await mkdir(outputDir, { recursive: true });

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  upstreamCalls: [],
  cases: {
    missingOrder: await captureCase('missingOrder'),
    missingDraftOrder: await captureCase('missingDraftOrder'),
  },
  notes:
    'Live Shopify capture for paymentTermsCreate reference-not-found branches. Unknown Order and DraftOrder GIDs return PAYMENT_TERMS_CREATION_UNSUCCESSFUL userErrors with field: null, type-specific messages, and paymentTerms: null.',
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
