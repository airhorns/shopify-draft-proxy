/* oxlint-disable no-console -- CLI capture scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (object === null) return undefined;
    current = object[part];
  }
  return current;
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors']) !== undefined) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertDeleteRecordNotFound(result: ConformanceGraphqlResult): void {
  const payload = readObject(readPath(result.payload, ['data', 'metaobjectDelete']));
  const userErrors = readPath(payload, ['userErrors']);
  const firstError = Array.isArray(userErrors) ? readObject(userErrors[0]) : null;

  if (
    payload?.['deletedId'] !== null ||
    firstError?.['code'] !== 'RECORD_NOT_FOUND' ||
    firstError?.['message'] !== 'Record not found' ||
    JSON.stringify(firstError?.['field']) !== JSON.stringify(['id'])
  ) {
    throw new Error(`metaobjectDelete did not return RECORD_NOT_FOUND: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: {
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
  };
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-delete-not-found.json');
const documentPath = 'config/parity-requests/metaobjects/metaobject-entry-lifecycle-delete.graphql';
const deleteMutation = await readFile(documentPath, 'utf8');
const variables = { id: 'gid://shopify/Metaobject/does-not-exist' };

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const result = await runGraphqlRaw(deleteMutation, variables);
assertGraphqlOk(result, 'metaobjectDelete fabricated id');
assertDeleteRecordNotFound(result);

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scenarioId: 'metaobject-delete-not-found',
  notes:
    'Captured live Admin GraphQL evidence that metaobjectDelete with a fabricated Metaobject id returns deletedId null and RECORD_NOT_FOUND userError.',
  cases: {
    deleteFabricatedId: captureFromResult('delete-fabricated-id', deleteMutation, variables, result),
  },
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);
