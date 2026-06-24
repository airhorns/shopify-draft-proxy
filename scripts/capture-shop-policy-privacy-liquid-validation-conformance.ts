/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'store-properties');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || readObject(result.payload)?.['errors']) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertCapturedUserError(result: ConformanceGraphqlResult): void {
  const payload = readObject(result.payload);
  const data = readObject(payload?.['data']);
  const update = readObject(data?.['shopPolicyUpdate']);
  const userErrors = update?.['userErrors'];
  const expected = [
    {
      field: ['shopPolicy', 'body'],
      message: "Body Liquid syntax error: Unknown tag 'unknownTag'",
      code: null,
    },
  ];
  if (JSON.stringify(update?.['shopPolicy']) !== 'null' || JSON.stringify(userErrors) !== JSON.stringify(expected)) {
    throw new Error(`Unexpected privacy Liquid validation payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

const mutation = await readFile(
  path.join('config', 'parity-requests', 'store-properties', 'shopPolicyUpdate-user-error-codes.graphql'),
  'utf8',
);
const variables = {
  shopPolicy: {
    type: 'PRIVACY_POLICY',
    body: '{% unknownTag %}',
  },
};

const invalidPrivacyLiquidValidation = await runGraphqlRequest(mutation, variables);
assertNoTopLevelErrors(invalidPrivacyLiquidValidation, 'privacy-policy Liquid syntax shopPolicyUpdate validation');
assertCapturedUserError(invalidPrivacyLiquidValidation);

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  invalidPrivacyLiquidValidation: {
    operationName: 'ShopPolicyUpdate',
    query: mutation,
    variables,
    response: invalidPrivacyLiquidValidation.payload,
  },
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'shop-policy-update-privacy-liquid-validation.json');
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
