/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
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

type CleanupResult =
  | CapturedRequest
  | {
      documentPath: string;
      variables: JsonRecord;
      error: string;
    };

const scenarioId = 'fulfillment-service-requires-shipping-method-default';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const createOmittedDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-requires-shipping-default-create-omitted.graphql',
);
const createFalseDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-requires-shipping-default-create-false.graphql',
);
const updateOmittedDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-requires-shipping-default-update-omitted.graphql',
);
const readDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-requires-shipping-default-read.graphql',
);
const deleteDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'fulfillment-service-lifecycle-delete.graphql',
);

async function readText(relativePath: string): Promise<string> {
  return readFile(path.join(process.cwd(), relativePath), 'utf8');
}

async function capture(documentPath: string, variables: JsonRecord): Promise<CapturedRequest> {
  const document = await readText(documentPath);
  return {
    documentPath,
    variables,
    response: await client.runGraphqlRequest(document, variables),
  };
}

function isObject(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function payloadData(captured: CapturedRequest): JsonRecord {
  const data = captured.response.payload.data;
  if (!isObject(data)) {
    throw new Error(`Expected payload data for ${captured.documentPath}: ${JSON.stringify(captured.response.payload)}`);
  }
  return data;
}

function mutationPayload(
  captured: CapturedRequest,
  root: 'fulfillmentServiceCreate' | 'fulfillmentServiceUpdate',
): JsonRecord {
  const payload = payloadData(captured)[root];
  if (!isObject(payload)) {
    throw new Error(`Expected ${root} payload: ${JSON.stringify(captured.response.payload)}`);
  }
  return payload;
}

function readFulfillmentService(
  captured: CapturedRequest,
  root: 'fulfillmentServiceCreate' | 'fulfillmentServiceUpdate',
): JsonRecord {
  const service = mutationPayload(captured, root)['fulfillmentService'];
  if (!isObject(service)) {
    throw new Error(`Expected ${root}.fulfillmentService object: ${JSON.stringify(captured.response.payload)}`);
  }
  return service;
}

function readFulfillmentServiceResult(captured: CapturedRequest): JsonRecord {
  const service = payloadData(captured)['fulfillmentService'];
  if (!isObject(service)) {
    throw new Error(`Expected fulfillmentService read object: ${JSON.stringify(captured.response.payload)}`);
  }
  return service;
}

function assertNoTopLevelErrors(captured: CapturedRequest, context: string): void {
  if (
    captured.response.status < 200 ||
    captured.response.status >= 300 ||
    Array.isArray(captured.response.payload.errors)
  ) {
    throw new Error(`${context} failed: ${JSON.stringify(captured.response, null, 2)}`);
  }
}

function assertNoUserErrors(
  captured: CapturedRequest,
  root: 'fulfillmentServiceCreate' | 'fulfillmentServiceUpdate',
  context: string,
): void {
  assertNoTopLevelErrors(captured, context);
  const userErrors = mutationPayload(captured, root)['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors)}`);
}

function assertRequiresShippingMethod(service: JsonRecord, expected: boolean, context: string): void {
  if (service['requiresShippingMethod'] !== expected) {
    throw new Error(`Expected ${context} requiresShippingMethod=${expected}: ${JSON.stringify(service)}`);
  }
}

function readFulfillmentServiceId(service: JsonRecord, context: string): string {
  const id = service['id'];
  if (typeof id !== 'string') {
    throw new Error(`Expected ${context} fulfillment service id: ${JSON.stringify(service)}`);
  }
  return id;
}

async function cleanup(id: string): Promise<CleanupResult> {
  try {
    return await capture(deleteDocumentPath, { id });
  } catch (error) {
    return {
      documentPath: deleteDocumentPath,
      variables: { id },
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

const token = `fs-rsm-default-${Date.now().toString(36)}`;
const cleanupIds: string[] = [];
const cleanupResults: CleanupResult[] = [];

let createOmitted: CapturedRequest | null = null;
let readAfterCreateOmitted: CapturedRequest | null = null;
let setupExplicitFalse: CapturedRequest | null = null;
let updateOmittedAfterFalse: CapturedRequest | null = null;
let readAfterUpdateOmitted: CapturedRequest | null = null;

try {
  createOmitted = await capture(createOmittedDocumentPath, { name: `FS Requires Shipping Omitted ${token}` });
  assertNoUserErrors(createOmitted, 'fulfillmentServiceCreate', 'create omitted requiresShippingMethod');
  const omittedService = readFulfillmentService(createOmitted, 'fulfillmentServiceCreate');
  assertRequiresShippingMethod(omittedService, true, 'create omitted requiresShippingMethod');
  const omittedServiceId = readFulfillmentServiceId(omittedService, 'create omitted requiresShippingMethod');
  cleanupIds.push(omittedServiceId);

  readAfterCreateOmitted = await capture(readDocumentPath, { id: omittedServiceId });
  assertNoTopLevelErrors(readAfterCreateOmitted, 'read after omitted create');
  assertRequiresShippingMethod(readFulfillmentServiceResult(readAfterCreateOmitted), true, 'read after omitted create');

  setupExplicitFalse = await capture(createFalseDocumentPath, { name: `FS Requires Shipping False ${token}` });
  assertNoUserErrors(setupExplicitFalse, 'fulfillmentServiceCreate', 'setup explicit false');
  const falseService = readFulfillmentService(setupExplicitFalse, 'fulfillmentServiceCreate');
  assertRequiresShippingMethod(falseService, false, 'setup explicit false');
  const falseServiceId = readFulfillmentServiceId(falseService, 'setup explicit false');
  cleanupIds.push(falseServiceId);

  updateOmittedAfterFalse = await capture(updateOmittedDocumentPath, {
    id: falseServiceId,
    name: `FS Requires Shipping Omitted Update ${token}`,
  });
  assertNoUserErrors(updateOmittedAfterFalse, 'fulfillmentServiceUpdate', 'omitted update after false');
  assertRequiresShippingMethod(
    readFulfillmentService(updateOmittedAfterFalse, 'fulfillmentServiceUpdate'),
    true,
    'omitted update after false',
  );

  readAfterUpdateOmitted = await capture(readDocumentPath, { id: falseServiceId });
  assertNoTopLevelErrors(readAfterUpdateOmitted, 'read after omitted update');
  assertRequiresShippingMethod(readFulfillmentServiceResult(readAfterUpdateOmitted), true, 'read after omitted update');
} finally {
  for (const id of cleanupIds.reverse()) {
    cleanupResults.push(await cleanup(id));
  }
}

if (
  !createOmitted ||
  !readAfterCreateOmitted ||
  !setupExplicitFalse ||
  !updateOmittedAfterFalse ||
  !readAfterUpdateOmitted
) {
  throw new Error('Expected all fulfillment-service requiresShippingMethod captures to be present.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId,
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      token,
      notes: [
        `Captured against ${storeDomain} using Admin GraphQL ${apiVersion}.`,
        'fulfillmentServiceCreate omits requiresShippingMethod and records Shopify applying the public GraphQL default true.',
        'A second fulfillment service is created with requiresShippingMethod false; fulfillmentServiceUpdate then omits the argument and records Shopify resetting it to true.',
        'Both mutation payloads and downstream fulfillmentService(id:) reads are captured before cleanup.',
      ],
      createOmitted,
      readAfterCreateOmitted,
      setupExplicitFalse,
      updateOmittedAfterFalse,
      readAfterUpdateOmitted,
      cleanup: cleanupResults,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, storeDomain, apiVersion, fixturePath: outputPath }, null, 2));
