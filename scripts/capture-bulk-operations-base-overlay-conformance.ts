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

const scenarioId = 'bulk-operations-live-hybrid-base-overlay';
const preRunFirst = 10;
const pollIntervalMs = readPositiveIntegerEnv('SHOPIFY_CONFORMANCE_BULK_OVERLAY_POLL_INTERVAL_MS', 1_500);
const maxPolls = readPositiveIntegerEnv('SHOPIFY_CONFORMANCE_BULK_OVERLAY_MAX_POLLS', 8);
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
const specPath = path.join('config', 'parity-specs', 'bulk-operations', `${scenarioId}.json`);
const preRunDocumentPath = path.join(requestDir, 'bulk-operations-base-overlay-pre-run.graphql');
const postRunDocumentPath = path.join(requestDir, 'bulk-operations-base-overlay-post-run.graphql');
const runQueryDocumentPath = path.join(requestDir, 'bulk-operations-base-overlay-run-query.graphql');

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

const preRunWindowQuery = `#graphql
query BulkOperationsBaseOverlayPreRun($first: Int!, $sortKey: BulkOperationsSortKeys, $reverse: Boolean) {
  bulkOperations(first: $first, sortKey: $sortKey, reverse: $reverse) {
    nodes {
      ${bulkOperationFields}
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}
`;

const postRunWindowQuery = `#graphql
query BulkOperationsBaseOverlayPostRun($first: Int!, $sortKey: BulkOperationsSortKeys, $reverse: Boolean) {
  bulkOperations(first: $first, sortKey: $sortKey, reverse: $reverse) {
    nodes {
      ${bulkOperationFields}
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}
`;

const runQueryMutation = `#graphql
mutation BulkOperationRunQueryBaseOverlay($query: String!) {
  bulkOperationRunQuery(query: $query) {
    bulkOperation {
      ${bulkOperationFields}
    }
    userErrors {
      field
      message
    }
  }
}
`;

const safeBulkQuery = '{ products { edges { node { id } } } }';

function readPositiveIntegerEnv(name: string, fallback: number): number {
  const rawValue = process.env[name];
  if (!rawValue) {
    return fallback;
  }

  const parsed = Number.parseInt(rawValue, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer when set.`);
  }

  return parsed;
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
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  const result = await runGraphqlRequest(query, variables);
  return captureResult(operationName, query, variables, result);
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function dataRecord(interaction: CapturedInteraction): Record<string, unknown> | null {
  return asRecord(asRecord(interaction.response)?.['data']);
}

function readBulkOperationsNodes(interaction: CapturedInteraction): Record<string, unknown>[] {
  const data = dataRecord(interaction);
  const bulkOperations = asRecord(data?.['bulkOperations']);
  const nodes = bulkOperations?.['nodes'];
  return Array.isArray(nodes)
    ? nodes.map(asRecord).filter((node): node is Record<string, unknown> => node !== null)
    : [];
}

function readRunBulkOperation(interaction: CapturedInteraction): Record<string, unknown> | null {
  const data = dataRecord(interaction);
  const payload = asRecord(data?.['bulkOperationRunQuery']);
  return asRecord(payload?.['bulkOperation']);
}

function readRunUserErrors(interaction: CapturedInteraction): unknown[] {
  const data = dataRecord(interaction);
  const payload = asRecord(data?.['bulkOperationRunQuery']);
  const userErrors = payload?.['userErrors'];
  return Array.isArray(userErrors) ? userErrors : [];
}

function nodeId(node: Record<string, unknown> | null): string | null {
  const id = node?.['id'];
  return typeof id === 'string' ? id : null;
}

function assertNoGraphqlErrors(label: string, interaction: CapturedInteraction): void {
  if (interaction.status >= 400) {
    throw new Error(`${label} returned HTTP ${interaction.status}.`);
  }
  const response = asRecord(interaction.response);
  const errors = response?.['errors'];
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned GraphQL errors: ${JSON.stringify(errors)}`);
  }
}

async function capturePostRunWindowUntilVisible(
  runId: string,
  variables: Record<string, unknown>,
): Promise<{ interaction: CapturedInteraction; pollCount: number }> {
  let latest: CapturedInteraction | null = null;

  for (let index = 0; index < maxPolls; index += 1) {
    if (index > 0) {
      await sleep(pollIntervalMs);
    }

    latest = await capture('BulkOperationsBaseOverlayPostRun', postRunWindowQuery, variables);
    assertNoGraphqlErrors(`post-run bulkOperations poll ${index + 1}`, latest);
    if (readBulkOperationsNodes(latest).some((node) => nodeId(node) === runId)) {
      return { interaction: latest, pollCount: index + 1 };
    }
  }

  throw new Error(
    `bulkOperations post-run window did not include ${runId} after ${maxPolls} poll(s). Last response: ${JSON.stringify(
      latest?.response,
    )}`,
  );
}

const preRunVariables = {
  first: preRunFirst,
  sortKey: 'CREATED_AT',
  reverse: false,
};
const preRunWindow = await capture('BulkOperationsBaseOverlayPreRun', preRunWindowQuery, preRunVariables);
assertNoGraphqlErrors('pre-run bulkOperations window', preRunWindow);
const preRunNodes = readBulkOperationsNodes(preRunWindow);
if (preRunNodes.length === 0) {
  throw new Error('Pre-run bulkOperations window is empty; cannot prove historical operations remain visible.');
}

const run = await capture('BulkOperationRunQueryBaseOverlay', runQueryMutation, { query: safeBulkQuery });
assertNoGraphqlErrors('bulkOperationRunQuery', run);
const runOperation = readRunBulkOperation(run);
const runId = nodeId(runOperation);
const runUserErrors = readRunUserErrors(run);
if (runId === null || runUserErrors.length > 0) {
  throw new Error(
    `bulkOperationRunQuery did not return a usable BulkOperation id with empty userErrors: ${JSON.stringify(
      run.response,
    )}`,
  );
}

const postRunVariables = {
  first: preRunNodes.length + 1,
  sortKey: 'CREATED_AT',
  reverse: false,
};
const { interaction: postRunWindow, pollCount: postRunPollCount } = await capturePostRunWindowUntilVisible(
  runId,
  postRunVariables,
);
const postRunNodes = readBulkOperationsNodes(postRunWindow);
const preRunIds = new Set(preRunNodes.map(nodeId).filter((id): id is string => id !== null));
const historicalIdsStillVisible = postRunNodes
  .map(nodeId)
  .filter((id): id is string => id !== null && id !== runId && preRunIds.has(id));
if (historicalIdsStillVisible.length === 0) {
  throw new Error(
    `Post-run bulkOperations window did not retain any pre-run historical ids. Pre-run ids: ${JSON.stringify([
      ...preRunIds,
    ])}; post-run ids: ${JSON.stringify(postRunNodes.map(nodeId))}`,
  );
}

await mkdir(outputDir, { recursive: true });
await mkdir(requestDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId,
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      pollConfig: {
        pollIntervalMs,
        maxPolls,
        postRunPollCount,
      },
      runBulkOperationId: runId,
      historicalIdsStillVisible,
      preRunWindow,
      run,
      postRunWindow,
      upstreamCalls: [preRunWindow, postRunWindow],
    },
    null,
    2,
  )}\n`,
  'utf8',
);
await writeFile(preRunDocumentPath, preRunWindowQuery, 'utf8');
await writeFile(postRunDocumentPath, postRunWindowQuery, 'utf8');
await writeFile(runQueryDocumentPath, runQueryMutation, 'utf8');
await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
console.log(`Wrote ${preRunDocumentPath}`);
console.log(`Wrote ${postRunDocumentPath}`);
console.log(`Wrote ${runQueryDocumentPath}`);
console.log(`Wrote ${specPath}`);

function buildSpec(): Record<string, unknown> {
  return {
    scenarioId,
    operationNames: ['bulkOperationRunQuery', 'bulkOperations'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'downstream-read-parity', 'live-hybrid-overlay-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['src/proxy/media_products_saved_searches/bulk_operations.rs'],
    proxyRequest: {
      documentPath: runQueryDocumentPath,
      apiVersion,
      variablesCapturePath: '$.run.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Captures a real bulkOperations window before and after bulkOperationRunQuery. The post-run parity target stages the mutation locally, hydrates the recorded Shopify post-run window through the LiveHybrid upstream cassette, and strictly compares the captured upstream nodes so historical BulkOperation records remain visible instead of being hidden by staged local jobs.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'run-query-created-operation',
          capturePath: '$.run.response.data.bulkOperationRunQuery.bulkOperation',
          proxyPath: '$.data.bulkOperationRunQuery.bulkOperation',
          expectedDifferences: [
            {
              path: '$.createdAt',
              matcher: 'iso-timestamp',
              reason:
                "The local proxy stages a new synthetic BulkOperation instead of reusing Shopify's captured timestamp.",
            },
            {
              path: '$.id',
              matcher: 'shopify-gid:BulkOperation',
              reason: 'BulkOperation IDs are shop-generated in Shopify and synthetic in the local proxy.',
            },
          ],
        },
        {
          name: 'run-query-user-errors',
          capturePath: '$.run.response.data.bulkOperationRunQuery.userErrors',
          proxyPath: '$.data.bulkOperationRunQuery.userErrors',
        },
        {
          name: 'post-run-window-keeps-upstream-bulk-operations',
          capturePath: '$.postRunWindow.response.data.bulkOperations.nodes',
          proxyPath: '$.data.bulkOperations.nodes',
          proxyRequest: {
            documentPath: postRunDocumentPath,
            apiVersion,
            variablesCapturePath: '$.postRunWindow.variables',
          },
        },
      ],
    },
  };
}
