/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'b2b-update-unknown-id-resource-not-found';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
const unknownCompanyId = 'gid://shopify/Company/999999999999';
const unknownCompanyLocationId = 'gid://shopify/CompanyLocation/999999999999';
const unknownTaxSettingsLocationId = 'gid://shopify/CompanyLocation/999999999998';

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index)) return undefined;
      current = current[index];
      continue;
    }
    const record = readRecord(current);
    if (!record) return undefined;
    current = record[segment];
  }
  return current;
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertResourceNotFound(result: ConformanceGraphqlResult, root: string): void {
  const payload = readRecord(readPath(result.payload, ['data', root]));
  if (!payload) {
    throw new Error(`${root} did not return a payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const userErrors = readPath(payload, ['userErrors']);
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`${root} did not return userErrors: ${JSON.stringify(payload, null, 2)}`);
  }
  const firstError = readRecord(userErrors[0]);
  if (firstError?.['code'] !== 'RESOURCE_NOT_FOUND') {
    throw new Error(`${root} did not return RESOURCE_NOT_FOUND: ${JSON.stringify(payload, null, 2)}`);
  }
}

async function readDocument(documentPath: string): Promise<string> {
  return await readFile(documentPath, 'utf8');
}

async function runOperation(documentPath: string, variables: JsonRecord, root: string): Promise<RecordedOperation> {
  const query = await readDocument(documentPath);
  const response = await runGraphqlRequest(query, variables);
  assertGraphqlOk(response, root);
  assertResourceNotFound(response, root);
  return {
    request: { query, variables },
    response: response.payload as JsonRecord,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyUpdate = await runOperation(
  'config/parity-requests/b2b/b2b-update-unknown-id-company-update.graphql',
  { companyId: unknownCompanyId },
  'companyUpdate',
);
const companyLocationUpdate = await runOperation(
  'config/parity-requests/b2b/b2b-update-unknown-id-location-update.graphql',
  { companyLocationId: unknownCompanyLocationId },
  'companyLocationUpdate',
);
const companyLocationTaxSettingsUpdate = await runOperation(
  'config/parity-requests/b2b/b2b-update-unknown-id-tax-settings.graphql',
  { companyLocationId: unknownTaxSettingsLocationId },
  'companyLocationTaxSettingsUpdate',
);

await mkdir(path.dirname(fixturePath), { recursive: true });
await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      companyUpdate,
      companyLocationUpdate,
      companyLocationTaxSettingsUpdate,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, scenarioId, fixturePath }, null, 2));
