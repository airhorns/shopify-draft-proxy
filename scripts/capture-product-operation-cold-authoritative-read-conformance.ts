/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

function record(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function responseData(payload: unknown): JsonRecord {
  return record(record(payload)?.['data']) ?? {};
}

function nestedRecord(value: unknown, ...pathSegments: string[]): JsonRecord | null {
  let current: unknown = value;
  for (const segment of pathSegments) {
    current = record(current)?.[segment];
  }
  return record(current);
}

function stringAt(value: unknown, ...pathSegments: string[]): string | null {
  let current: unknown = value;
  for (const segment of pathSegments) {
    current = record(current)?.[segment];
  }
  return typeof current === 'string' ? current : null;
}

function assertSuccessful(label: string, status: number, payload: unknown): void {
  const errors = record(payload)?.['errors'];
  if (status >= 200 && status < 300 && errors === undefined) {
    return;
  }
  throw new Error(`${label} failed with HTTP ${status}: ${JSON.stringify(payload)}`);
}

function assertNoUserErrors(label: string, payload: unknown, root: string): void {
  const userErrors = nestedRecord(responseData(payload), root)?.['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null)}`);
}

function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const requestPath = path.join(
  'config',
  'parity-requests',
  'products',
  'productOperation-cold-authoritative-read.graphql',
);
const operationReadQuery = await readFile(requestPath, 'utf8');
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-operation-cold-authoritative-read.json');

const setupMutation = `#graphql
  mutation ProductOperationColdAuthoritativeSetup($input: ProductSetInput!) {
    productSet(input: $input, synchronous: false) {
      product {
        id
      }
      productSetOperation {
        id
        status
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;
const cleanupMutation = `#graphql
  mutation ProductOperationColdAuthoritativeCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

await mkdir(outputDir, { recursive: true });

const runId = Date.now().toString();
const setupVariables = {
  input: {
    title: `Cold authoritative operation ${runId}`,
    status: 'DRAFT',
  },
};
let productId: string | null = null;
let cleanupResponse: Awaited<ReturnType<typeof runGraphqlRequest>> | null = null;

try {
  const setupResponse = await runGraphqlRequest(setupMutation, setupVariables);
  assertSuccessful('asynchronous productSet setup', setupResponse.status, setupResponse.payload);
  assertNoUserErrors('asynchronous productSet setup', setupResponse.payload, 'productSet');

  const operationId = stringAt(responseData(setupResponse.payload), 'productSet', 'productSetOperation', 'id');
  if (!operationId) {
    throw new Error('Asynchronous productSet setup did not return a ProductSetOperation id.');
  }

  const operationVariables = { id: operationId };
  let operationResponse: Awaited<ReturnType<typeof runGraphqlRequest>> | null = null;
  for (let attempt = 0; attempt < 15; attempt += 1) {
    operationResponse = await runGraphqlRequest(operationReadQuery, operationVariables);
    assertSuccessful('productOperation poll', operationResponse.status, operationResponse.payload);
    const operation = nestedRecord(responseData(operationResponse.payload), 'productOperation');
    if (operation?.['status'] === 'COMPLETE') {
      productId = stringAt(operation, 'product', 'id');
      break;
    }
    await sleep(1000);
  }
  if (
    !operationResponse ||
    stringAt(responseData(operationResponse.payload), 'productOperation', 'status') !== 'COMPLETE'
  ) {
    throw new Error(`ProductSetOperation ${operationId} did not reach COMPLETE during capture.`);
  }
  if (!productId) {
    throw new Error(`Completed ProductSetOperation ${operationId} did not expose its product.`);
  }

  const cleanupResult = await runGraphqlRequest(cleanupMutation, { input: { id: productId } });
  assertSuccessful('productOperation capture cleanup', cleanupResult.status, cleanupResult.payload);
  assertNoUserErrors('productOperation capture cleanup', cleanupResult.payload, 'productDelete');
  cleanupResponse = cleanupResult;

  const fixture = {
    scenarioId: 'product-operation-cold-authoritative-read',
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    setup: {
      query: setupMutation,
      variables: setupVariables,
      response: setupResponse,
    },
    operationRead: {
      query: operationReadQuery,
      variables: operationVariables,
      response: operationResponse,
    },
    cleanup: {
      query: cleanupMutation,
      variables: { input: { id: productId } },
      response: cleanupResponse,
    },
    upstreamCalls: [
      {
        method: 'POST',
        apiSurface: 'admin',
        apiVersion,
        path: `/admin/api/${apiVersion}/graphql.json`,
        operationName: 'ProductOperationColdAuthoritativeRead',
        query: operationReadQuery,
        variables: operationVariables,
        response: {
          status: operationResponse.status,
          body: operationResponse.payload,
        },
      },
    ],
    notes:
      'Captured from a disposable asynchronous productSet. The productOperation request and upstream cassette query are the exact same checked-in GraphQL document.',
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath, operationId, productId }, null, 2));
} finally {
  if (productId && cleanupResponse === null) {
    await runGraphqlRequest(cleanupMutation, { input: { id: productId } });
  }
}
