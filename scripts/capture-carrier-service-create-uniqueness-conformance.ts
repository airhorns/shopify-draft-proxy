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
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'carrier-service-create-uniqueness.json');
const documentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-create-uniqueness.graphql',
);

const deleteDocument = `#graphql
  mutation CarrierServiceCreateUniquenessCleanup($id: ID!) {
    carrierServiceDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function readObject(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function carrierServiceCreatePayload(payload: ConformanceGraphqlPayload): JsonRecord {
  const data = readObject(payload.data);
  const mutationPayload = readObject(data?.['carrierServiceCreate']);
  if (!mutationPayload) {
    throw new Error(`Expected carrierServiceCreate payload: ${JSON.stringify(payload)}`);
  }
  return mutationPayload;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: ConformanceGraphqlResult, context: string): void {
  assertNoTopLevelErrors(result, context);
  const payload = carrierServiceCreatePayload(result.payload);
  const userErrors = payload['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors)}`);
}

function assertConfiguredUserError(result: ConformanceGraphqlResult, expectedMessage: string, context: string): void {
  assertNoTopLevelErrors(result, context);
  const payload = carrierServiceCreatePayload(result.payload);
  if (payload['carrierService'] !== null) {
    throw new Error(`${context} expected null carrierService; got ${JSON.stringify(payload['carrierService'])}`);
  }
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 1) {
    throw new Error(`${context} expected exactly one userError; got ${JSON.stringify(userErrors)}`);
  }
  const error = readObject(userErrors[0]);
  if (
    error?.['field'] !== null ||
    error?.['message'] !== expectedMessage ||
    error?.['code'] !== 'CARRIER_SERVICE_CREATE_FAILED'
  ) {
    throw new Error(`${context} expected configured userError; got ${JSON.stringify(error)}`);
  }
}

function readCarrierServiceId(result: ConformanceGraphqlResult, context: string): string {
  const payload = carrierServiceCreatePayload(result.payload);
  const service = readObject(payload['carrierService']);
  const id = service?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`${context} expected carrierService id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function readOptionalCarrierServiceId(result: ConformanceGraphqlResult): string | null {
  const payload = carrierServiceCreatePayload(result.payload);
  const service = readObject(payload['carrierService']);
  const id = service?.['id'];
  return typeof id === 'string' ? id : null;
}

async function readRequest(): Promise<string> {
  return await readFile(path.join(process.cwd(), documentPath), 'utf8');
}

async function capture(document: string, variables: JsonRecord): Promise<CapturedRequest> {
  return {
    documentPath,
    variables,
    response: await client.runGraphqlRequest(document, variables),
  };
}

async function cleanupCarrierService(id: string): Promise<ConformanceGraphqlResult | null> {
  try {
    return await client.runGraphqlRequest(deleteDocument.replace(/^#graphql\n/u, '').trim(), { id });
  } catch (error) {
    console.error(`Failed to cleanup carrier service ${id}:`, error);
    return null;
  }
}

const document = await readRequest();
const suffix = Date.now().toString(36);
const name = `Hermes Carrier Uniqueness ${suffix}`;
const firstCreateVariables = {
  input: {
    name,
    callbackUrl: 'https://mock.shop/carrier-service-rates',
    supportsServiceDiscovery: false,
    active: true,
  },
};
const duplicateCreateVariables = {
  input: {
    name,
    callbackUrl: 'https://mock.shop/carrier-service-rates-2',
    supportsServiceDiscovery: false,
    active: true,
  },
};

let firstCreate: CapturedRequest | null = null;
let duplicateCreate: CapturedRequest | null = null;
let cleanup: ConformanceGraphqlResult | null = null;
const cleanupIds: string[] = [];

try {
  firstCreate = await capture(document, firstCreateVariables);
  assertNoUserErrors(firstCreate.response, 'first create');
  cleanupIds.push(readCarrierServiceId(firstCreate.response, 'first create'));

  duplicateCreate = await capture(document, duplicateCreateVariables);
  assertConfiguredUserError(duplicateCreate.response, `${name} is already configured`, 'duplicate create');
  const unexpectedDuplicateId = readOptionalCarrierServiceId(duplicateCreate.response);
  if (unexpectedDuplicateId) cleanupIds.push(unexpectedDuplicateId);
} finally {
  for (const id of cleanupIds) {
    cleanup = await cleanupCarrierService(id);
  }
}

if (firstCreate === null || duplicateCreate === null) {
  throw new Error('Expected first and duplicate create captures to be present.');
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
        'Captures DeliveryCarrierService active per-app uniqueness for carrierServiceCreate against live Shopify Admin GraphQL.',
        'The first create stages an active carrier service for the calling app; the second create with the same name returns a base CARRIER_SERVICE_CREATE_FAILED userError.',
        'The created carrier service is deleted during cleanup.',
      ],
      firstCreate,
      duplicateCreate,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
