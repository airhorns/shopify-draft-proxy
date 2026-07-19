/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedInteraction = {
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  status: number;
  response: unknown;
};

type JsonRecord = Record<string, unknown>;

const scenarioId = 'bulk-operation-cold-catalog-hydration';
const terminalStatuses = new Set(['CANCELED', 'COMPLETED', 'EXPIRED', 'FAILED']);
const pollIntervalMs = readPositiveIntegerEnv('SHOPIFY_CONFORMANCE_BULK_POLL_INTERVAL_MS', 1_500);
const maxPolls = readPositiveIntegerEnv('SHOPIFY_CONFORMANCE_BULK_MAX_POLLS', 60);
const configEnv = {
  ...process.env,
  SHOPIFY_CONFORMANCE_API_VERSION: process.env['SHOPIFY_CONFORMANCE_BULK_API_VERSION'] ?? '2026-04',
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: configEnv,
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'bulk-operations');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const requestDir = path.join('config', 'parity-requests', 'bulk-operations');
const requestPath = path.join(requestDir, `${scenarioId}-run-query.graphql`);
const specPath = path.join('config', 'parity-specs', 'bulk-operations', `${scenarioId}.json`);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const bulkOperationFields = `
  id
  status
  type
  errorCode
  createdAt
  completedAt
  objectCount
  rootObjectCount
  fileSize
  url
  partialDataUrl
  query
`;

const productCreateMutation = `mutation BulkCatalogHydrationProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
      tags
    }
    userErrors {
      field
      message
    }
  }
}`;

const productDeleteMutation = `mutation BulkCatalogHydrationProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}`;

const productSearchQuery = `query BulkCatalogHydrationProductVisible($query: String!) {
  products(first: 1, query: $query) {
    nodes {
      id
    }
  }
}`;

// Keep this byte-identical to the production hydration document. The parity
// cassette matches the exact GraphQL text and variables sent by the proxy.
const productCatalogHydrationQuery =
  'query BulkProductsCatalogHydrate($first: Int!, $after: String) { products(first: $first, after: $after) { nodes { id title tags } pageInfo { hasNextPage endCursor } } }';

const bulkOperationRunQueryMutation = `mutation BulkCatalogHydrationRunQuery($query: String!) {
  bulkOperationRunQuery(query: $query) {
    bulkOperation {
      ${bulkOperationFields}
    }
    userErrors {
      field
      message
      code
    }
  }
}`;

const bulkOperationByIdQuery = `query BulkCatalogHydrationOperationById($id: ID!) {
  bulkOperation(id: $id) {
    ${bulkOperationFields}
  }
}`;

function readPositiveIntegerEnv(name: string, fallback: number): number {
  const rawValue = process.env[name];
  if (!rawValue) return fallback;
  const parsed = Number.parseInt(rawValue, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer when set.`);
  }
  return parsed;
}

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function captureResult(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): CapturedInteraction {
  return {
    operationName,
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

async function capture(
  operationName: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CapturedInteraction> {
  return captureResult(operationName, query, variables, await runGraphqlRequest(query, variables));
}

function readData(interaction: CapturedInteraction): JsonRecord | null {
  return asRecord(asRecord(interaction.response)?.['data']);
}

function readPayload(interaction: CapturedInteraction, field: string): JsonRecord | null {
  return asRecord(readData(interaction)?.[field]);
}

function readUserErrors(interaction: CapturedInteraction, field: string): unknown[] {
  const errors = readPayload(interaction, field)?.['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors(interaction: CapturedInteraction, field: string): void {
  const errors = readUserErrors(interaction, field);
  if (errors.length > 0) {
    throw new Error(`${field} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function readProductId(interaction: CapturedInteraction): string {
  const id = asRecord(readPayload(interaction, 'productCreate')?.['product'])?.['id'];
  if (typeof id !== 'string') throw new Error('productCreate did not return a product id.');
  return id;
}

function readBulkOperation(interaction: CapturedInteraction, payload: boolean): JsonRecord | null {
  if (payload) return asRecord(readPayload(interaction, 'bulkOperationRunQuery')?.['bulkOperation']);
  return asRecord(readData(interaction)?.['bulkOperation']);
}

function readStringField(record: JsonRecord | null, field: string): string {
  const value = record?.[field];
  if (typeof value !== 'string') throw new Error(`${field} was not a string.`);
  return value;
}

async function waitForProductSearch(query: string, productId: string): Promise<CapturedInteraction[]> {
  const probes: CapturedInteraction[] = [];
  for (let index = 0; index < maxPolls; index += 1) {
    if (index > 0) await sleep(pollIntervalMs);
    const probe = await capture('BulkCatalogHydrationProductVisible', productSearchQuery, { query });
    probes.push(probe);
    const nodes = asRecord(readData(probe)?.['products'])?.['nodes'];
    if (Array.isArray(nodes) && nodes.some((node) => asRecord(node)?.['id'] === productId)) return probes;
  }
  throw new Error(`Created product ${productId} was not visible through ${query}.`);
}

async function captureCatalogPages(): Promise<CapturedInteraction[]> {
  const pages: CapturedInteraction[] = [];
  const seenCursors = new Set<string>();
  let after: string | null = null;
  for (let index = 0; index < 10_000; index += 1) {
    const page = await capture('BulkProductsCatalogHydrate', productCatalogHydrationQuery, {
      first: 250,
      after,
      nestedFirst: 250,
      nestedAfter: null,
    });
    pages.push(page);
    const connection = asRecord(readData(page)?.['products']);
    const pageInfo = asRecord(connection?.['pageInfo']);
    if (!Array.isArray(connection?.['nodes']) || typeof pageInfo?.['hasNextPage'] !== 'boolean') {
      throw new Error(`Catalog hydration page was malformed: ${JSON.stringify(page.response)}`);
    }
    if (pageInfo['hasNextPage'] === false) return pages;
    const endCursor = pageInfo['endCursor'];
    if (typeof endCursor !== 'string' || seenCursors.has(endCursor)) {
      throw new Error(`Catalog hydration cursor did not prove progress: ${JSON.stringify(pageInfo)}`);
    }
    seenCursors.add(endCursor);
    after = endCursor;
  }
  throw new Error('Catalog hydration exceeded 10,000 pages.');
}

async function pollBulkOperationToTerminal(id: string): Promise<CapturedInteraction[]> {
  const polls: CapturedInteraction[] = [];
  for (let index = 0; index < maxPolls; index += 1) {
    if (index > 0) await sleep(pollIntervalMs);
    const poll = await capture('BulkCatalogHydrationOperationById', bulkOperationByIdQuery, { id });
    polls.push(poll);
    const status = readBulkOperation(poll, false)?.['status'];
    if (typeof status === 'string' && terminalStatuses.has(status)) return polls;
  }
  throw new Error(`BulkOperation ${id} did not reach a terminal status.`);
}

async function captureBulkResult(url: string): Promise<JsonRecord> {
  const response = await fetch(url);
  const body = await response.text();
  const records = body
    .trim()
    .split('\n')
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line) as unknown);
  return {
    status: response.status,
    contentType: response.headers.get('content-type'),
    byteLength: Buffer.byteLength(body, 'utf8'),
    body,
    records,
  };
}

function buildSpec(): JsonRecord {
  return {
    scenarioId,
    operationNames: ['productCreate', 'productDelete', 'bulkOperationRunQuery', 'bulkOperation'],
    scenarioStatus: 'captured',
    assertionKinds: [
      'payload-shape',
      'downstream-read-parity',
      'bulk-jsonl-artifact-parity',
      'live-hybrid-hydration-parity',
    ],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: [
      'src/proxy/media_products_saved_searches/bulk_operations.rs',
      'tests/graphql_routes/admin_app_shipping.rs',
    ],
    proxyConfig: { readMode: 'live-hybrid' },
    proxyRequest: {
      apiVersion,
      documentPath: requestPath,
      variablesCapturePath: '$.run.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      "Captures a disposable Shopify product, the exact ordinary Admin GraphQL catalog pages required by a cold LiveHybrid product bulk export, and Shopify's terminal JSONL artifact for the same tag-filtered query. The strict artifact target proves the proxy hydrates the captured baseline instead of representing an unobserved catalog as empty.",
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'cold-live-hybrid-run-payload',
          capturePath: '$.run.response.data.bulkOperationRunQuery',
          proxyPath: '$.data.bulkOperationRunQuery',
          expectedDifferences: [
            {
              path: '$.bulkOperation.createdAt',
              matcher: 'iso-timestamp',
              reason: 'Shopify and the proxy create their BulkOperation records on different clocks.',
            },
            {
              path: '$.bulkOperation.id',
              matcher: 'shopify-gid:BulkOperation',
              reason: 'Shopify allocates the captured job id while the proxy allocates a synthetic local id.',
            },
          ],
        },
        {
          name: 'cold-live-hybrid-terminal-counts',
          capturePath: '$.run.terminalOperation',
          proxyPath: '$.data.bulkOperation',
          proxyRequest: {
            apiVersion,
            documentPath: 'config/parity-requests/bulk-operations/bulk-operation-by-id-parity.graphql',
            variables: {
              id: {
                fromProxyResponse: 'cold-live-hybrid-run-payload',
                path: '$.data.bulkOperationRunQuery.bulkOperation.id',
              },
            },
          },
          expectedDifferences: [
            {
              path: '$.completedAt',
              matcher: 'iso-timestamp',
              reason: 'Shopify and the proxy complete their BulkOperation records on different clocks.',
            },
            {
              path: '$.createdAt',
              matcher: 'iso-timestamp',
              reason: 'Shopify and the proxy create their BulkOperation records on different clocks.',
            },
            {
              path: '$.fileSize',
              matcher: 'non-empty-string',
              reason: 'Both artifacts are non-empty but their storage encodings may produce different byte counts.',
            },
            {
              path: '$.id',
              matcher: 'shopify-gid:BulkOperation',
              reason: 'Shopify allocates the captured job id while the proxy allocates a synthetic local id.',
            },
            {
              path: '$.url',
              matcher: 'non-empty-string',
              reason: 'Shopify uses signed storage while the proxy serves an instance-owned local artifact URL.',
            },
          ],
        },
        {
          name: 'cold-live-hybrid-jsonl-contains-captured-baseline-product',
          capturePath: '$.run.result.records[0]',
          proxyPath: '$.body',
          proxyHttpRequest: {
            method: 'GET',
            path: {
              fromProxyResponse: 'cold-live-hybrid-terminal-counts',
              path: '$.data.bulkOperation.url',
            },
          },
          preserveProxyState: true,
          expectedDifferences: [],
        },
      ],
    },
  };
}

const runId = `${Date.now().toString(36)}-${process.pid.toString(36)}`;
const tag = `conformance-bulk-hydration-${runId}`;
const title = `Cold bulk hydration ${runId}`;
const bulkQuery = `{ products(query: "tag:${tag}") { edges { node { id title } } } }`;
let createdProductId: string | null = null;
let cleanup: CapturedInteraction | null = null;
let fixture: JsonRecord | null = null;

try {
  const setup = await capture('BulkCatalogHydrationProductCreate', productCreateMutation, {
    product: { title, tags: ['conformance', tag] },
  });
  assertNoUserErrors(setup, 'productCreate');
  createdProductId = readProductId(setup);
  const searchProbes = await waitForProductSearch(`tag:${tag}`, createdProductId);
  const upstreamCalls = await captureCatalogPages();
  const run = await capture('BulkCatalogHydrationRunQuery', bulkOperationRunQueryMutation, { query: bulkQuery });
  assertNoUserErrors(run, 'bulkOperationRunQuery');
  const operationId = readStringField(readBulkOperation(run, true), 'id');
  const statusPolls = await pollBulkOperationToTerminal(operationId);
  const terminalOperation = readBulkOperation(statusPolls[statusPolls.length - 1] as CapturedInteraction, false);
  if (terminalOperation?.['status'] !== 'COMPLETED') {
    throw new Error(`BulkOperation ${operationId} did not complete: ${JSON.stringify(terminalOperation)}`);
  }
  if (terminalOperation['objectCount'] !== '1' || terminalOperation['rootObjectCount'] !== '1') {
    throw new Error(`Expected one captured product row: ${JSON.stringify(terminalOperation)}`);
  }
  const result = await captureBulkResult(readStringField(terminalOperation, 'url'));
  const records = result['records'];
  if (
    !Array.isArray(records) ||
    records.length !== 1 ||
    asRecord(records[0])?.['id'] !== createdProductId ||
    asRecord(records[0])?.['title'] !== title
  ) {
    throw new Error(`Captured JSONL did not contain the disposable product: ${JSON.stringify(records)}`);
  }
  fixture = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    setup,
    searchProbes,
    upstreamCalls,
    run: {
      ...run,
      statusPolls,
      terminalOperation,
      result,
    },
  };
} finally {
  if (createdProductId !== null) {
    cleanup = await capture('BulkCatalogHydrationProductDelete', productDeleteMutation, {
      input: { id: createdProductId },
    });
  }
}

if (fixture === null) throw new Error('Capture did not produce a fixture.');
fixture['cleanup'] = cleanup;
await mkdir(outputDir, { recursive: true });
await mkdir(requestDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
await writeFile(requestPath, bulkOperationRunQueryMutation, 'utf8');
await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
console.log(`Wrote ${requestPath}`);
console.log(`Wrote ${specPath}`);
