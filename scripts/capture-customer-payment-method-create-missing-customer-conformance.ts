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
const outputPath = path.join(outputDir, 'customer-payment-method-create-missing-customer.json');
const creditCardDocumentPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'customer-payment-method-credit-card-create-missing-customer.graphql',
);
const remoteCreateDocumentPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'customer-payment-method-remote-create-missing-customer.graphql',
);
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

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readOperationPayload(result: ConformanceGraphqlResult, root: string): JsonRecord {
  const data = readRecord(result.payload.data);
  const payload = readRecord(data?.[root]);
  if (!payload) throw new Error(`${root} returned no payload: ${JSON.stringify(result.payload, null, 2)}`);
  return payload;
}

function assertUserError(
  payload: JsonRecord,
  expected: {
    root: string;
    methodField: string;
    field: string[];
    message: string;
    code?: string;
  },
): void {
  if (payload[expected.methodField] !== null) {
    throw new Error(`${expected.root} unexpectedly returned a payment method: ${JSON.stringify(payload, null, 2)}`);
  }

  const userErrors = Array.isArray(payload['userErrors']) ? payload['userErrors'] : [];
  const firstError = readRecord(userErrors[0]);
  if (!firstError) {
    throw new Error(`${expected.root} returned no userErrors: ${JSON.stringify(payload, null, 2)}`);
  }

  const matchesField = JSON.stringify(firstError['field']) === JSON.stringify(expected.field);
  const matchesMessage = firstError['message'] === expected.message;
  const matchesCode = expected.code === undefined || firstError['code'] === expected.code;
  if (!matchesField || !matchesMessage || !matchesCode) {
    throw new Error(
      `${expected.root} returned unexpected userError: ${JSON.stringify(firstError, null, 2)}`,
    );
  }
}

const creditCardDocument = await readFile(creditCardDocumentPath, 'utf8');
const remoteCreateDocument = await readFile(remoteCreateDocumentPath, 'utf8');

await mkdir(outputDir, { recursive: true });

const creditCardResponse = await runGraphqlRequest(creditCardDocument);
assertNoTopLevelErrors(creditCardResponse, 'customerPaymentMethodCreditCardCreate missing customer');
const creditCardPayload = readOperationPayload(
  creditCardResponse,
  'customerPaymentMethodCreditCardCreate',
);
assertUserError(creditCardPayload, {
  root: 'customerPaymentMethodCreditCardCreate',
  methodField: 'customerPaymentMethod',
  field: ['customerId'],
  message: 'Customer does not exist',
});

const remoteCreateResponse = await runGraphqlRequest(remoteCreateDocument);
assertNoTopLevelErrors(remoteCreateResponse, 'customerPaymentMethodRemoteCreate missing customer');
const remoteCreatePayload = readOperationPayload(
  remoteCreateResponse,
  'customerPaymentMethodRemoteCreate',
);
assertUserError(remoteCreatePayload, {
  root: 'customerPaymentMethodRemoteCreate',
  methodField: 'customerPaymentMethod',
  field: ['customerId'],
  message: 'is invalid',
  code: 'INVALID',
});

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  upstreamCalls: [],
  operations: {
    creditCardMissingCustomer: {
      purpose:
        'customerPaymentMethodCreditCardCreate rejects a missing customer before session or billing-address validation.',
      query: creditCardDocument,
      variables: {},
      response: creditCardResponse.payload,
    },
    remoteCreateMissingCustomer: {
      purpose:
        'customerPaymentMethodRemoteCreate rejects a missing customer before remote-reference validation.',
      query: remoteCreateDocument,
      variables: {},
      response: remoteCreateResponse.payload,
    },
  },
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
