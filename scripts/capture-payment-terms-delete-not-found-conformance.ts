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
const outputPath = path.join(outputDir, 'payment-terms-delete-not-found.json');
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

const paymentTermsDeleteDocument = `#graphql
  mutation PaymentTermsDeleteNotFound($input: PaymentTermsDeleteInput!) {
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

const variables = {
  input: {
    paymentTermsId: 'gid://shopify/PaymentTerms/999999999999999',
  },
};

await mkdir(outputDir, { recursive: true });

const response = await runGraphqlRequest(paymentTermsDeleteDocument, variables);
assertNoTopLevelErrors(response, 'paymentTermsDelete missing id');

const responseData = readRecord(response.payload.data);
const payload = readRecord(responseData?.['paymentTermsDelete']);
const userErrors = Array.isArray(payload?.['userErrors']) ? payload['userErrors'] : [];
const firstError = readRecord(userErrors[0]);
if (payload?.['deletedId'] !== null || !firstError || firstError['code'] !== 'PAYMENT_TERMS_DELETE_UNSUCCESSFUL') {
  throw new Error(
    `paymentTermsDelete not-found returned unexpected payload: ${JSON.stringify(response.payload, null, 2)}`,
  );
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  upstreamCalls: [],
  case: {
    purpose: 'Unknown PaymentTerms id returns the public PaymentTermsDeleteUserError code enum.',
    query: paymentTermsDeleteDocument,
    variables,
    response: response.payload,
  },
  notes:
    'Live Shopify capture for paymentTermsDelete not-found. The selected PaymentTermsDeleteUserError.code value is PAYMENT_TERMS_DELETE_UNSUCCESSFUL.',
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
