/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedRequest = {
  documentPath: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'fulfillment-service-callback-url-update-protocol-validation.json');
const setupCreateDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-callback-url-update-protocol-create.graphql',
);
const updateProtocolDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-callback-url-validation-update-protocol.graphql',
);

const deleteDocument = `#graphql
  mutation FulfillmentServiceCallbackUrlUpdateProtocolCleanup($id: ID!) {
    fulfillmentServiceDelete(id: $id, inventoryAction: DELETE) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

async function readText(relativePath: string): Promise<string> {
  return readFile(path.join(process.cwd(), relativePath), 'utf8');
}

async function capture(documentPath: string, variables: JsonRecord): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  return {
    documentPath,
    variables,
    response: await runGraphqlRequest(document, variables),
  };
}

async function captureAdHoc(
  query: string,
  variables: JsonRecord,
): Promise<{
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
}> {
  const trimmed = query.replace(/^#graphql\n/u, '').trim();
  return {
    query: trimmed,
    variables,
    response: await runGraphqlRequest(trimmed, variables),
  };
}

function isObject(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readFulfillmentServiceId(createCapture: CapturedRequest): string | null {
  const payload = createCapture.response.payload;
  if (!isObject(payload.data)) return null;
  const createPayload = payload.data['fulfillmentServiceCreate'];
  if (!isObject(createPayload)) return null;
  const fulfillmentService = createPayload['fulfillmentService'];
  if (!isObject(fulfillmentService)) return null;
  const id = fulfillmentService['id'];
  return typeof id === 'string' ? id : null;
}

function userErrors(captureResult: CapturedRequest, root: string): JsonRecord[] {
  const payload = captureResult.response.payload;
  const data = payload.data;
  if (!isObject(data)) return [];
  const rootPayload = data[root];
  if (!isObject(rootPayload)) return [];
  const errors = rootPayload['userErrors'];
  return Array.isArray(errors) ? errors.filter(isObject) : [];
}

function assertNoUserErrors(captureResult: CapturedRequest, root: string): void {
  const errors = userErrors(captureResult, root);
  if (errors.length !== 0) {
    throw new Error(`${root} expected no userErrors, got ${JSON.stringify(errors)}`);
  }
}

function assertProtocolBlocked(captureResult: CapturedRequest, root: string, protocol: string): void {
  const errors = userErrors(captureResult, root);
  const first = errors[0];
  if (
    JSON.stringify(first?.['field']) !== JSON.stringify(['callbackUrl']) ||
    first?.['message'] !== `Callback url protocol ${protocol}:// is not supported`
  ) {
    throw new Error(`${root} expected ${protocol} protocol block, got ${JSON.stringify(errors)}`);
  }
}

const suffix = `${Date.now()}`;
const setupCreate = await capture(setupCreateDocumentPath, {
  name: `Hermes Callback Update Protocol ${suffix}`,
  callbackUrl: 'https://mock.shop/fulfillment-service-callback',
});
assertNoUserErrors(setupCreate, 'fulfillmentServiceCreate');

const serviceId = readFulfillmentServiceId(setupCreate);
if (serviceId === null) {
  throw new Error('Expected setup fulfillment service ID to be present.');
}

let updateProtocol: CapturedRequest | null = null;
const cleanup: Awaited<ReturnType<typeof captureAdHoc>>[] = [];
try {
  updateProtocol = await capture(updateProtocolDocumentPath, {
    id: serviceId,
    callbackUrl: 'ftp://mock.shop/fulfillment-service-callback-updated',
  });
  assertProtocolBlocked(updateProtocol, 'fulfillmentServiceUpdate', 'ftp');
} finally {
  cleanup.push(await captureAdHoc(deleteDocument, { id: serviceId }));
}

if (updateProtocol === null) {
  throw new Error('Expected update protocol validation capture to be present.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Captures fulfillmentServiceUpdate callbackUrl protocol validation against live Shopify Admin GraphQL.',
        'A valid mock.shop fulfillment service is created only as setup for the update validation branch.',
        'Shopify rejects ftp:// callbackUrl updates with a protocol-specific payload userError on field ["callbackUrl"].',
        'The created fulfillment service is deleted during cleanup.',
      ],
      setupCreate,
      updateProtocol,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
