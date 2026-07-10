/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlResult = { status: number; payload: unknown };

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segment-staged-catalog-roots.json');
const createDocument = await readFile(
  path.join('config', 'parity-requests', 'segments', 'segment-staged-catalog-create.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'segments', 'segment-staged-catalog-read.graphql'),
  'utf8',
);

const deleteDocument = `#graphql
  mutation SegmentStagedCatalogCleanup($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function randomSuffix(): string {
  return `${Date.now()}${Math.random().toString(36).slice(2, 8)}`;
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, parts: string[]): unknown {
  let current: unknown = value;
  for (const part of parts) {
    const record = readRecord(current);
    current = record?.[part];
    if (current === undefined) return undefined;
  }
  return current;
}

function readStringPath(payload: unknown, parts: string[], label: string): string {
  const value = readPath(payload, parts);
  if (typeof value !== 'string') {
    throw new Error(`${label} missing string at ${parts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }
  return value;
}

function readUserErrors(payload: unknown, root: string): unknown[] {
  const value = readPath(payload, ['data', root, 'userErrors']);
  return Array.isArray(value) ? value : [];
}

async function assertGraphqlOk(label: string, result: GraphqlResult): Promise<void> {
  if (result.status >= 200 && result.status < 300 && !readRecord(result.payload)?.['errors']) {
    return;
  }
  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

function assertNoUserErrors(label: string, result: GraphqlResult, root: string): void {
  const userErrors = readUserErrors(result.payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function assertObjectPath(payload: unknown, parts: string[], label: string): void {
  if (!readRecord(readPath(payload, parts))) {
    throw new Error(`${label} missing object at ${parts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }
}

const suffix = randomSuffix();
const createVariables = {
  name: `Staged catalog capture ${suffix}`,
  query: "customer_tags CONTAINS 'staged-catalog-capture'",
};
const readVariables = {
  first: 10,
  filterSearch: 'email',
  valueSearch: '',
  valueFilterQueryName: 'customer_tags',
};

let createResponse: GraphqlResult | null = null;
let readResponse: GraphqlResult | null = null;
let cleanupResponse: GraphqlResult | null = null;
let segmentId = '';

try {
  createResponse = await runGraphqlRequest(createDocument, createVariables);
  await assertGraphqlOk('segment-create', createResponse);
  assertNoUserErrors('segment-create', createResponse, 'segmentCreate');
  segmentId = readStringPath(createResponse.payload, ['data', 'segmentCreate', 'segment', 'id'], 'segment id');

  readResponse = await runGraphqlRequest(readDocument, readVariables);
  await assertGraphqlOk('segment-catalog-read', readResponse);
  for (const root of ['filters', 'filterSuggestions', 'valueSuggestions', 'migrations']) {
    assertObjectPath(readResponse.payload, ['data', root], root);
  }
} finally {
  if (segmentId) {
    cleanupResponse = await runGraphqlRequest(deleteDocument, { id: segmentId });
  }
}

if (!createResponse || !readResponse) {
  throw new Error('capture did not complete create and read operations');
}

const capture = {
  scenarioId: 'segment-staged-catalog-roots',
  apiVersion,
  storeDomain,
  capturedAt: new Date().toISOString(),
  operations: {
    create: {
      request: { query: createDocument, variables: createVariables },
      response: createResponse,
    },
    read: {
      request: { query: readDocument, variables: readVariables },
      response: readResponse,
    },
    cleanup: cleanupResponse
      ? {
          request: { query: deleteDocument, variables: { id: segmentId } },
          response: cleanupResponse,
        }
      : null,
  },
  upstreamCalls: [
    {
      operationName: 'SegmentStagedCatalogRead',
      variables: readVariables,
      query: readDocument,
      response: { status: readResponse.status, body: readResponse.payload },
    },
  ],
  notes:
    'Live Shopify evidence that segment catalog roots remain available after a segment has been created. The proxy replay creates its own staged segment through segmentCreate, then uses this upstream read cassette only for the catalog roots in the mixed read.',
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
