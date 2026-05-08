/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'saved-searches');

function readObject(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertCustomerDeprecationOnly(payload: ConformanceGraphqlPayload): void {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.['savedSearchCreate']);
  if (!mutationPayload) {
    throw new Error(`Expected savedSearchCreate payload: ${JSON.stringify(payload, null, 2)}`);
  }
  if (mutationPayload['savedSearch'] !== null) {
    throw new Error(`Expected savedSearchCreate.savedSearch to be null: ${JSON.stringify(mutationPayload, null, 2)}`);
  }

  const userErrors = mutationPayload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`Expected exactly one CUSTOMER deprecation userError: ${JSON.stringify(userErrors, null, 2)}`);
  }

  const error = readObject(userErrors[0]);
  if (
    error?.['field'] !== null ||
    error['message'] !== 'Customer saved searches have been deprecated. Use Segmentation API instead.'
  ) {
    throw new Error(`Unexpected CUSTOMER deprecation userError: ${JSON.stringify(error, null, 2)}`);
  }
}

async function readRequest(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'saved-searches', name), 'utf8');
}

const document = await readRequest('saved-search-customer-deprecation-early-return.graphql');
const variables = {
  input: {
    resourceType: 'CUSTOMER',
    name: '12345678901234567890123456789012345678901',
    query: 'collection_id:"123" tag:"AAA"',
  },
};

const savedSearchCustomerDeprecationEarlyReturn = await client.runGraphqlRequest(document, variables);
assertNoTopLevelErrors(
  savedSearchCustomerDeprecationEarlyReturn,
  'saved-search CUSTOMER deprecation early-return create capture',
);
assertCustomerDeprecationOnly(savedSearchCustomerDeprecationEarlyReturn.payload);

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  notes: [
    'savedSearchCreate with resourceType CUSTOMER returns the CUSTOMER deprecation userError before name-length or query validation.',
    'The submitted name is 41 characters and the query uses PRODUCT-only incompatible filters, but Shopify returns only the CUSTOMER deprecation userError.',
  ],
  savedSearchCustomerDeprecationEarlyReturn: {
    documentPath: 'config/parity-requests/saved-searches/saved-search-customer-deprecation-early-return.graphql',
    variables,
    payload: savedSearchCustomerDeprecationEarlyReturn.payload,
  },
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
const fixturePath = path.join(outputDir, 'saved-search-customer-deprecation-early-return.json');
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath }, null, 2));
