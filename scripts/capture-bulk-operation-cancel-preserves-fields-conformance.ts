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
  completedAt?: unknown;
  objectCount?: unknown;
  rootObjectCount?: unknown;
  fileSize?: unknown;
  url?: unknown;
};

const scenarioId = 'bulk-operation-cancel-preserves-fields';
const terminalStatuses = new Set(['CANCELED', 'COMPLETED', 'EXPIRED', 'FAILED']);
const progressPollIntervalMs = readPositiveIntegerEnv('SHOPIFY_CONFORMANCE_BULK_PROGRESS_POLL_INTERVAL_MS', 500);
const progressMaxPolls = readPositiveIntegerEnv('SHOPIFY_CONFORMANCE_BULK_PROGRESS_MAX_POLLS', 120);
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

const bulkOperationByIdQuery = `#graphql
  query BulkOperationByIdCapture($id: ID!) {
    bulkOperation(id: $id) {
      ${bulkOperationFields}
    }
  }
`;

const bulkOperationHydrateCassetteQuery =
  'query BulkOperationHydrate($id: ID!) { bulkOperation(id: $id) { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query } }';

const bulkOperationCancelMutation = `#graphql
  mutation BulkOperationCancelCapture($id: ID!) {
    bulkOperationCancel(id: $id) {
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

const bulkOperationRunQueryMutation = `#graphql
  mutation BulkOperationRunQueryCapture($query: String!) {
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

const progressBulkQuery = `#graphql
{
  products {
    edges {
      node {
        id
        variants {
          edges {
            node {
              id
            }
          }
        }
        metafields {
          edges {
            node {
              id
            }
          }
        }
        collections {
          edges {
            node {
              id
            }
          }
        }
        media {
          edges {
            node {
              id
            }
          }
        }
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

function readBulkOperationFromField(interaction: CapturedInteraction, fieldName: string): BulkOperationNode | null {
  const data = readData(interaction);
  return asRecord(data?.[fieldName]);
}

function readPayloadBulkOperation(
  interaction: CapturedInteraction,
  payloadFieldName: string,
): BulkOperationNode | null {
  const data = readData(interaction);
  const payload = asRecord(data?.[payloadFieldName]);
  return asRecord(payload?.['bulkOperation']);
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

function readBulkOperationStringField(node: BulkOperationNode | null, field: keyof BulkOperationNode): string | null {
  const value = node?.[field];
  return typeof value === 'string' ? value : null;
}

function hasPreservationEvidence(node: BulkOperationNode | null): boolean {
  return (
    (readBulkOperationStringField(node, 'objectCount') ?? '0') !== '0' ||
    (readBulkOperationStringField(node, 'rootObjectCount') ?? '0') !== '0' ||
    readBulkOperationStringField(node, 'fileSize') !== null ||
    readBulkOperationStringField(node, 'url') !== null ||
    readBulkOperationStringField(node, 'completedAt') !== null
  );
}

function hydrateCassetteCallFromOperation(operation: BulkOperationNode): Record<string, unknown> {
  const id = readBulkOperationId(operation);
  if (id === null) {
    throw new Error('Cannot build BulkOperation hydrate cassette without an id.');
  }

  return {
    operationName: 'BulkOperationHydrate',
    variables: { id },
    query: bulkOperationHydrateCassetteQuery,
    response: {
      status: 200,
      body: {
        data: {
          bulkOperation: operation,
        },
      },
    },
  };
}

async function captureProgressCancelLifecycle(): Promise<Record<string, unknown>> {
  const run = await capture('BulkOperationRunQueryCapture', bulkOperationRunQueryMutation, {
    query: progressBulkQuery,
  });
  const bulkOperation = readPayloadBulkOperation(run, 'bulkOperationRunQuery');
  const id = readBulkOperationId(bulkOperation);
  const userErrors = readUserErrors(run, 'bulkOperationRunQuery');
  const lifecycle: Record<string, unknown> = {
    run,
    id,
    userErrors,
  };

  if (id === null || userErrors.length > 0) {
    throw new Error(
      JSON.stringify({
        reason: 'bulkOperationRunQuery did not return a usable BulkOperation id with empty userErrors.',
        userErrors,
        response: run.response,
      }),
    );
  }

  const progressPolls: CapturedInteraction[] = [];
  let preCancelObservation: CapturedInteraction | null = null;
  for (let index = 0; index < progressMaxPolls; index += 1) {
    if (index > 0) {
      await sleep(progressPollIntervalMs);
    }

    const poll = await capture(`BulkOperationProgressPoll${index + 1}`, bulkOperationByIdQuery, { id });
    progressPolls.push(poll);
    const operation = readBulkOperationFromField(poll, 'bulkOperation');
    const status = readBulkOperationStatus(operation);
    if (status !== null && !terminalStatuses.has(status) && hasPreservationEvidence(operation)) {
      preCancelObservation = poll;
      break;
    }
    if (status !== null && terminalStatuses.has(status)) {
      break;
    }
  }

  lifecycle['progressPolls'] = progressPolls;
  if (preCancelObservation === null) {
    const statuses = progressPolls
      .map((poll) => readBulkOperationStatus(readBulkOperationFromField(poll, 'bulkOperation')))
      .filter((status): status is string => status !== null);
    const lastPoll = progressPolls.at(-1);
    const lastOperation = lastPoll ? readBulkOperationFromField(lastPoll, 'bulkOperation') : null;
    throw new Error(
      JSON.stringify({
        reason:
          'No non-terminal BulkOperation with non-zero counters or artifact fields was observed before terminal status.',
        observedStatuses: statuses,
        lastOperation,
      }),
    );
  }

  lifecycle['preCancelObservation'] = preCancelObservation;
  lifecycle['cancelAttempt'] = await capture('BulkOperationCancelCapture', bulkOperationCancelMutation, { id });
  lifecycle['readAfterCancel'] = await capture('BulkOperationByIdCapture', bulkOperationByIdQuery, { id });
  return lifecycle;
}

await mkdir(outputDir, { recursive: true });

const lifecycle = await captureProgressCancelLifecycle();
const preCancelOperation = readBulkOperationFromField(
  lifecycle['preCancelObservation'] as CapturedInteraction,
  'bulkOperation',
);
if (preCancelOperation === null) {
  throw new Error('Progress cancel capture did not retain a pre-cancel BulkOperation observation.');
}

const fixture = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  pollConfig: {
    progressPollIntervalMs,
    progressMaxPolls,
  },
  lifecycle,
  upstreamCalls: [hydrateCassetteCallFromOperation(preCancelOperation)],
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
