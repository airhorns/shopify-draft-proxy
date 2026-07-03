/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'payments');
const fixturePath = path.join(fixtureDir, 'order-create-mandate-payment-validation.json');
const missingMandateRequestPath = path.join(
  'config',
  'parity-requests',
  'payments',
  'order_create_mandate_payment_missing_mandate.graphql',
);

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

const missingMandateDocument = await readFile(missingMandateRequestPath, 'utf8');
const missingMandateVariables = {
  id: 'gid://shopify/Order/1',
  idempotencyKey: 'missing-mandate',
};
const missingMandate: ConformanceGraphqlResult<JsonRecord> = await runGraphqlRequest<JsonRecord>(
  missingMandateDocument,
  missingMandateVariables,
);

await writeJson(fixturePath, {
  scenarioId: 'order-create-mandate-payment-validation',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  source: 'live-shopify-admin-graphql',
  notes:
    'Live public Admin API schema-validation capture for orderCreateMandatePayment with omitted mandateId. Successful mandate payment remains runtime-test-backed until a real CustomerPaymentMethod/PaymentMandate can be provisioned for the conformance app.',
  missingMandate: {
    query: missingMandateDocument.trim(),
    variables: missingMandateVariables,
    response: missingMandate.payload,
  },
  upstreamCalls: [],
});

console.log(JSON.stringify({ fixturePath }, null, 2));
