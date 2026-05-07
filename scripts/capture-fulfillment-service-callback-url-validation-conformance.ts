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
const outputPath = path.join(outputDir, 'fulfillment-service-callback-url-validation.json');
const primaryDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-callback-url-validation.graphql',
);
const updateAllowedDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-callback-url-validation-update-allowed.graphql',
);
const updateDisallowedDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-callback-url-validation-update-disallowed.graphql',
);

const deleteDocument = `#graphql
  mutation FulfillmentServiceCallbackUrlValidationCleanup($id: ID!) {
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

function readFulfillmentServiceId(primary: CapturedRequest, root: string): string | null {
  const payload = primary.response.payload;
  if (!isObject(payload.data)) return null;
  const createPayload = payload.data[root];
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

function assertCallbackBlocked(captureResult: CapturedRequest, root: string): void {
  const errors = userErrors(captureResult, root);
  const first = errors[0];
  if (
    JSON.stringify(first?.['field']) !== JSON.stringify(['callbackUrl']) ||
    first?.['message'] !== 'Callback url is not allowed'
  ) {
    throw new Error(`${root} expected callbackUrl block, got ${JSON.stringify(errors)}`);
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
const primary = await capture(primaryDocumentPath, {
  validHttpsName: `Hermes Callback HTTPS ${suffix}`,
  validHttpsCallbackUrl: 'https://mock.shop/fulfillment-service-callback',
  validHttpName: `Hermes Callback HTTP ${suffix}`,
  validHttpCallbackUrl: 'http://mock.shop/fulfillment-service-callback',
  originName: `Hermes Callback Origin ${suffix}`,
  originCallbackUrl: `${adminOrigin}/fulfillment-service-callback`,
  ftpName: `Hermes Callback FTP ${suffix}`,
  ftpCallbackUrl: 'ftp://mock.shop/fulfillment-service-callback',
  exampleName: `Hermes Callback Example ${suffix}`,
  exampleCallbackUrl: 'https://example.com/fulfillment-service-callback',
  shopifyName: `Hermes Callback Shopify ${suffix}`,
  shopifyCallbackUrl: 'https://shopify.com/fulfillment-service-callback',
});

assertNoUserErrors(primary, 'validHttpsCreate');
assertNoUserErrors(primary, 'validHttpCreate');
assertNoUserErrors(primary, 'originCreate');
assertProtocolBlocked(primary, 'ftpCreate', 'ftp');
assertCallbackBlocked(primary, 'exampleCreate');
assertCallbackBlocked(primary, 'shopifyCreate');

const validHttpsId = readFulfillmentServiceId(primary, 'validHttpsCreate');
const validHttpId = readFulfillmentServiceId(primary, 'validHttpCreate');
const originId = readFulfillmentServiceId(primary, 'originCreate');
const cleanupIds = [validHttpsId, validHttpId, originId].filter((id): id is string => id !== null);
if (cleanupIds.length !== 3) {
  throw new Error(`Expected three valid fulfillment services, got ${JSON.stringify(cleanupIds)}`);
}

let updateAllowed: CapturedRequest | null = null;
let updateDisallowed: CapturedRequest | null = null;
const cleanup: Awaited<ReturnType<typeof captureAdHoc>>[] = [];
try {
  updateAllowed = await capture(updateAllowedDocumentPath, {
    id: validHttpsId,
    callbackUrl: 'http://mock.shop/fulfillment-service-callback-updated',
  });
  assertNoUserErrors(updateAllowed, 'fulfillmentServiceUpdate');

  updateDisallowed = await capture(updateDisallowedDocumentPath, {
    id: validHttpId,
    callbackUrl: 'https://example.com/fulfillment-service-callback-updated',
  });
  assertCallbackBlocked(updateDisallowed, 'fulfillmentServiceUpdate');
} finally {
  for (const id of cleanupIds) {
    cleanup.push(await captureAdHoc(deleteDocument, { id }));
  }
}

if (updateAllowed === null || updateDisallowed === null) {
  throw new Error('Expected update validation captures to be present.');
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
        'Captures current app-scoped FulfillmentService callbackUrl validation against live Shopify Admin GraphQL.',
        'The current app credential accepts valid http:// and https:// callback URLs in the mock.shop host family and the configured shop origin host.',
        'The same credential rejects ftp://mock.shop with a protocol-specific payload userError and rejects https://example.com plus https://shopify.com with the generic not-allowed callbackUrl userError.',
        'The successful creates exist to target update validation and are deleted during cleanup.',
      ],
      primary,
      updateAllowed,
      updateDisallowed,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
