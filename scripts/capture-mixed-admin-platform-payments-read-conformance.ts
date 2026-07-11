/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const documentPath = 'config/parity-requests/admin-platform/mixed-admin-platform-payments-read.graphql';
const scenarioId = 'mixed-admin-platform-payments-read';
const variables = { type: 'NET' };

function readRecord(value: unknown): JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : {};
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult<unknown>, context: string): void {
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)['errors']) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertMixedRootData(result: ConformanceGraphqlResult<unknown>): void {
  const data = readRecord(result.payload.data);
  if (!('poll' in data) || !('terms' in data)) {
    throw new Error(`Mixed-root capture did not return both response keys: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

const config = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
  exitOnMissing: true,
});
const accessToken = await getValidConformanceAccessToken({
  adminOrigin: config.adminOrigin,
  apiVersion: config.apiVersion,
});
const client = createAdminGraphqlClient({
  adminOrigin: config.adminOrigin,
  apiVersion: config.apiVersion,
  headers: buildAdminAuthHeaders(accessToken),
});
const query = await readFile(documentPath, 'utf8');
const response = await client.runGraphqlRequest(query, variables);
assertNoTopLevelErrors(response, scenarioId);
assertMixedRootData(response);

const fixturePath = path.join(
  'fixtures',
  'conformance',
  config.storeDomain,
  config.apiVersion,
  'admin-platform',
  `${scenarioId}.json`,
);
await mkdir(path.dirname(fixturePath), { recursive: true });
await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain: config.storeDomain,
      apiVersion: config.apiVersion,
      request: {
        query,
        variables,
      },
      variables,
      response: {
        status: response.status,
        payload: response.payload,
      },
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, scenarioId, fixturePath }, null, 2));
