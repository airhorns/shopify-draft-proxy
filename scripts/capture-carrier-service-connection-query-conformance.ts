/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

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
const outputPath = path.join(outputDir, 'carrier-service-connection-query.json');
const createDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-connection-query-create.graphql',
);
const readDocumentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-connection-query-read.graphql',
);

const deleteDocument = `#graphql
  mutation CarrierServiceConnectionQueryCleanup($id: ID!) {
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

function assertNoUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  assertNoTopLevelErrors(result, context);
  const data = readObject(result.payload.data);
  const payload = readObject(data?.[root]);
  const userErrors = payload?.['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors)}`);
}

function readCarrierService(result: ConformanceGraphqlResult, context: string): JsonRecord {
  const payload = carrierServiceCreatePayload(result.payload);
  const service = readObject(payload['carrierService']);
  if (!service || typeof service['id'] !== 'string') {
    throw new Error(`${context} expected carrierService id: ${JSON.stringify(result.payload)}`);
  }
  return service;
}

function resourceTail(id: string): string {
  return id.split('/').at(-1)?.split('?')[0] ?? id;
}

async function readRequest(documentPath: string): Promise<string> {
  return await readFile(path.join(process.cwd(), documentPath), 'utf8');
}

async function capture(documentPath: string, variables: JsonRecord): Promise<CapturedRequest> {
  const document = await readRequest(documentPath);
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

const suffix = Date.now().toString(36);
const firstCreateVariables = {
  input: {
    name: `Hermes Carrier Connection ${suffix} A`,
    callbackUrl: 'https://mock.shop/carrier-service-connection-a',
    supportsServiceDiscovery: false,
    active: false,
  },
};
const secondCreateVariables = {
  input: {
    name: `Hermes Carrier Connection ${suffix} B`,
    callbackUrl: 'https://mock.shop/carrier-service-connection-b',
    supportsServiceDiscovery: false,
    active: false,
  },
};

let firstCreate: CapturedRequest | null = null;
let secondCreate: CapturedRequest | null = null;
let sortedReverseRead: CapturedRequest | null = null;
const cleanupResults: ConformanceGraphqlResult[] = [];
const cleanupIds: string[] = [];

try {
  firstCreate = await capture(createDocumentPath, firstCreateVariables);
  assertNoUserErrors(firstCreate.response, 'carrierServiceCreate', 'first create');
  const firstService = readCarrierService(firstCreate.response, 'first create');
  cleanupIds.push(firstService['id'] as string);

  await delay(1_100);

  secondCreate = await capture(createDocumentPath, secondCreateVariables);
  assertNoUserErrors(secondCreate.response, 'carrierServiceCreate', 'second create');
  const secondService = readCarrierService(secondCreate.response, 'second create');
  cleanupIds.push(secondService['id'] as string);

  sortedReverseRead = await capture(readDocumentPath, {
    first: 2,
    query: 'active:false',
    sortKey: 'ID',
    reverse: true,
  });
  assertNoTopLevelErrors(sortedReverseRead.response, 'sorted reverse read');
  const data = readObject(sortedReverseRead.response.payload.data);
  const connection = readObject(data?.['carrierServices']);
  const nodes = Array.isArray(connection?.['nodes']) ? connection['nodes'] : [];
  const firstNode = readObject(nodes[0]);
  const secondNode = readObject(nodes[1]);
  if (firstNode?.['id'] !== secondService['id'] || secondNode?.['id'] !== firstService['id']) {
    throw new Error(
      `Expected ID reverse read to return higher-id carrier first: ${JSON.stringify(sortedReverseRead.response.payload)}`,
    );
  }
} finally {
  for (const id of cleanupIds) {
    const cleanup = await cleanupCarrierService(id);
    if (cleanup) cleanupResults.push(cleanup);
  }
}

if (firstCreate === null || secondCreate === null || sortedReverseRead === null) {
  throw new Error('Expected create and read captures to be present.');
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
        'Captures DeliveryCarrierService connection query behavior after creating two disposable inactive carrier services through live Shopify Admin GraphQL.',
        'The connection read uses active:false, sortKey ID, and reverse:true; Shopify returns the second created carrier before the first.',
        `The read filter intentionally uses active:false instead of id-specific filters so the same request can replay against proxy-synthetic carrier IDs. Captured live carrier tails were ${resourceTail(cleanupIds[0] ?? '')} and ${resourceTail(cleanupIds[1] ?? '')}.`,
        'Both created carrier services are deleted during cleanup.',
      ],
      firstCreate,
      secondCreate,
      sortedReverseRead,
      cleanup: cleanupResults,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
