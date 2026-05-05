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

type BulkOperationNode = {
  id?: unknown;
  status?: unknown;
  url?: unknown;
  rootObjectCount?: unknown;
};

const scenarioId = 'bulk-operation-run-query-group-objects';
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

const bulkOperationRunQueryGroupObjectsTrueMutation = `#graphql
  mutation BulkOperationRunQueryGroupObjectsTrue($query: String!) {
    bulkOperationRunQuery(query: $query, groupObjects: true) {
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

const bulkOperationRunQueryGroupObjectsDefaultMutation = `#graphql
  mutation BulkOperationRunQueryGroupObjectsDefault($query: String!) {
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

const bulkOperationByIdQuery = `#graphql
  query BulkOperationByIdCapture($id: ID!) {
    bulkOperation(id: $id) {
      ${bulkOperationFields}
    }
  }
`;

const safeBulkQuery = `#graphql
{
  products {
    edges {
      node {
        id
        title
      }
    }
  }
}`;

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
  variables: Record<string, unknown> = {},
): Promise<CapturedInteraction> {
  const result = await runGraphqlRequest(query, variables);
  return captureResult(operationName, query, variables, result);
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readData(interaction: CapturedInteraction): Record<string, unknown> | null {
  const response = asRecord(interaction.response);
  return asRecord(response?.['data']);
}

function readPayloadBulkOperation(
  interaction: CapturedInteraction,
  payloadFieldName: string,
): BulkOperationNode | null {
  const data = readData(interaction);
  const payload = asRecord(data?.[payloadFieldName]);
  return asRecord(payload?.['bulkOperation']);
}

function readBulkOperationFromField(interaction: CapturedInteraction, fieldName: string): BulkOperationNode | null {
  const data = readData(interaction);
  return asRecord(data?.[fieldName]);
}

function readUserErrors(interaction: CapturedInteraction, payloadFieldName: string): unknown[] {
  const data = readData(interaction);
  const payload = asRecord(data?.[payloadFieldName]);
  const userErrors = payload?.['userErrors'];
  return Array.isArray(userErrors) ? userErrors : [];
}

function readBulkOperationId(node: BulkOperationNode | null): string | null {
  return typeof node?.id === 'string' ? node.id : null;
}

function readBulkOperationStatus(node: BulkOperationNode | null): string | null {
  return typeof node?.status === 'string' ? node.status : null;
}

function readBulkOperationUrl(node: BulkOperationNode | null): string | null {
  return typeof node?.url === 'string' ? node.url : null;
}

function readRootObjectCount(node: BulkOperationNode | null): number {
  const value = node?.rootObjectCount;
  if (typeof value !== 'string') {
    return 0;
  }
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : 0;
}

async function pollBulkOperationToTerminal(id: string): Promise<CapturedInteraction[]> {
  const polls: CapturedInteraction[] = [];

  for (let index = 0; index < maxPolls; index += 1) {
    if (index > 0) {
      await sleep(pollIntervalMs);
    }

    const poll = await capture(`BulkOperationStatusPoll${index + 1}`, bulkOperationByIdQuery, { id });
    polls.push(poll);
    const status = readBulkOperationStatus(readBulkOperationFromField(poll, 'bulkOperation'));
    if (status !== null && terminalStatuses.has(status)) {
      break;
    }
  }

  return polls;
}

function findTerminalBulkOperation(polls: CapturedInteraction[]): BulkOperationNode | null {
  for (const poll of polls) {
    const operation = readBulkOperationFromField(poll, 'bulkOperation');
    const status = readBulkOperationStatus(operation);
    if (status !== null && terminalStatuses.has(status)) {
      return operation;
    }
  }

  return null;
}

async function captureBulkOperationResult(url: string): Promise<Record<string, unknown>> {
  const response = await fetch(url);
  const text = await response.text();
  const records = text
    .trim()
    .split('\n')
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line) as unknown);

  return {
    status: response.status,
    contentType: response.headers.get('content-type'),
    byteLength: Buffer.byteLength(text, 'utf8'),
    records,
  };
}

async function captureRunQueryLifecycle(operationName: string, mutation: string): Promise<Record<string, unknown>> {
  const run = await capture(operationName, mutation, { query: safeBulkQuery });
  const bulkOperation = readPayloadBulkOperation(run, 'bulkOperationRunQuery');
  const id = readBulkOperationId(bulkOperation);
  const userErrors = readUserErrors(run, 'bulkOperationRunQuery');
  const lifecycle: Record<string, unknown> = {
    run,
    id,
    userErrors,
  };

  if (id === null || userErrors.length > 0) {
    lifecycle['skippedStatusPolling'] = {
      reason: 'bulkOperationRunQuery did not return a usable BulkOperation id with empty userErrors.',
    };
    return lifecycle;
  }

  lifecycle['statusPolls'] = await pollBulkOperationToTerminal(id);
  const terminalOperation = findTerminalBulkOperation(lifecycle['statusPolls'] as CapturedInteraction[]);
  if (terminalOperation) {
    lifecycle['terminalOperation'] = terminalOperation;
    const resultUrl = readBulkOperationUrl(terminalOperation);
    if (resultUrl) {
      lifecycle['result'] = await captureBulkOperationResult(resultUrl);
    }
  }

  return lifecycle;
}

function synthesizeProductCountCall(source: Record<string, unknown>, label: string): Record<string, unknown> {
  return {
    operationName: 'BulkOperationRunQueryProductCount',
    variables: {},
    query: `hand-synthesized from ${scenarioId}.${label}.terminalOperation`,
    response: {
      status: 200,
      body: {
        data: {
          productsCount: {
            count: readRootObjectCount(asRecord(source['terminalOperation'])),
          },
        },
      },
    },
  };
}

await mkdir(outputDir, { recursive: true });

const runQueryGroupObjectsTrue = await captureRunQueryLifecycle(
  'BulkOperationRunQueryGroupObjectsTrue',
  bulkOperationRunQueryGroupObjectsTrueMutation,
);
const runQueryGroupObjectsDefault = await captureRunQueryLifecycle(
  'BulkOperationRunQueryGroupObjectsDefault',
  bulkOperationRunQueryGroupObjectsDefaultMutation,
);

const fixture: Record<string, unknown> = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  pollConfig: {
    pollIntervalMs,
    maxPolls,
  },
  runs: {
    runQueryGroupObjectsTrue,
    runQueryGroupObjectsDefault,
  },
  upstreamCalls: [
    synthesizeProductCountCall(runQueryGroupObjectsTrue, 'runQueryGroupObjectsTrue'),
    synthesizeProductCountCall(runQueryGroupObjectsDefault, 'runQueryGroupObjectsDefault'),
  ],
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
