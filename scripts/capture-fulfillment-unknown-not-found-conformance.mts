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
  response: {
    status: number;
    payload: JsonRecord;
  };
};

const scenarioId = 'fulfillment-unknown-not-found';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders', `${scenarioId}.json`);
const unknownCancelFulfillmentId = 'gid://shopify/Fulfillment/999999999999991';
const unknownTrackingFulfillmentId = 'gid://shopify/Fulfillment/999999999999992';

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

function assertUserErrorPayload(
  result: ConformanceGraphqlResult,
  root: string,
  expectedField: string[],
  expectedMessage: string,
): void {
  if (result.status !== 200 || result.payload.errors) {
    throw new Error(`${root} did not return HTTP 200 data payload: ${JSON.stringify(result, null, 2)}`);
  }
  const payload = readRecord(readPath(result.payload, ['data', root]));
  if (!payload) {
    throw new Error(`${root} did not return a mutation payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
  if (payload['fulfillment'] !== null) {
    throw new Error(`${root} did not return null fulfillment: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = readPath(payload, ['userErrors']);
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`${root} did not return exactly one userError: ${JSON.stringify(payload, null, 2)}`);
  }
  const error = readRecord(userErrors[0]);
  if (!error) {
    throw new Error(`${root} userError is not an object: ${JSON.stringify(payload, null, 2)}`);
  }
  const field = error['field'];
  if (JSON.stringify(field) !== JSON.stringify(expectedField) || error['message'] !== expectedMessage) {
    throw new Error(`${root} userError mismatch: ${JSON.stringify(payload, null, 2)}`);
  }
  if ('code' in error) {
    throw new Error(`${root} unexpectedly returned a userError code: ${JSON.stringify(payload, null, 2)}`);
  }
}

async function readDocument(documentPath: string): Promise<string> {
  return await readFile(documentPath, 'utf8');
}

async function runOperation(
  documentPath: string,
  variables: JsonRecord,
  root: string,
  expectedField: string[],
  expectedMessage: string,
): Promise<RecordedOperation> {
  const query = await readDocument(documentPath);
  const response = await runGraphqlRequest(query, variables);
  assertUserErrorPayload(response, root, expectedField, expectedMessage);
  return {
    request: { query, variables },
    response: {
      status: response.status,
      payload: response.payload as JsonRecord,
    },
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fulfillmentCancel = await runOperation(
  'config/parity-requests/orders/fulfillmentCancel-unknown-id-not-found.graphql',
  { id: unknownCancelFulfillmentId },
  'fulfillmentCancel',
  ['id'],
  'Fulfillment not found.',
);
const fulfillmentTrackingInfoUpdate = await runOperation(
  'config/parity-requests/orders/fulfillmentTrackingInfoUpdate-unknown-id-not-found.graphql',
  {
    fulfillmentId: unknownTrackingFulfillmentId,
    trackingInfoInput: {
      company: 'UPS',
      number: 'UNKNOWN-TRACK',
      url: 'https://tracking.example/UNKNOWN-TRACK',
    },
  },
  'fulfillmentTrackingInfoUpdate',
  ['fulfillmentId'],
  'Fulfillment does not exist.',
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
      fulfillmentCancel,
      fulfillmentTrackingInfoUpdate,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, scenarioId, fixturePath }, null, 2));
