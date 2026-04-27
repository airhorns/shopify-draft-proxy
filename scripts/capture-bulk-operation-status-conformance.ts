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
};

const terminalStatuses = new Set(['CANCELED', 'COMPLETED', 'EXPIRED', 'FAILED']);
const pollIntervalMs = readPositiveIntegerEnv('SHOPIFY_CONFORMANCE_BULK_POLL_INTERVAL_MS', 1_500);
const maxPolls = readPositiveIntegerEnv('SHOPIFY_CONFORMANCE_BULK_MAX_POLLS', 60);

const unknownBulkOperationId = 'gid://shopify/BulkOperation/0';
const malformedBulkOperationId = 'not-a-shopify-gid';
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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputPath = path.join(outputDir, 'bulk-operation-status-catalog-cancel.json');

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

const bulkOperationsCatalogQuery = `#graphql
  query BulkOperationsCatalogCapture(
    $first: Int
    $last: Int
    $after: String
    $before: String
    $query: String
    $sortKey: BulkOperationsSortKeys
    $reverse: Boolean
  ) {
    bulkOperations(
      first: $first
      last: $last
      after: $after
      before: $before
      query: $query
      sortKey: $sortKey
      reverse: $reverse
    ) {
      edges {
        cursor
        node {
          ${bulkOperationFields}
        }
      }
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

const currentBulkOperationQuery = `#graphql
  query CurrentBulkOperationCapture($type: BulkOperationType) {
    currentBulkOperation(type: $type) {
      ${bulkOperationFields}
    }
  }
`;

const bulkOperationMissingIdQuery = `#graphql
  query BulkOperationMissingIdValidation {
    bulkOperation {
      id
    }
  }
`;

const bulkOperationsMissingWindowQuery = `#graphql
  query BulkOperationsMissingWindowValidation {
    bulkOperations {
      nodes {
        id
      }
    }
  }
`;

const bulkOperationsFirstAndLastQuery = `#graphql
  query BulkOperationsFirstAndLastValidation {
    bulkOperations(first: 1, last: 1) {
      nodes {
        id
      }
    }
  }
`;

const bulkOperationsInvalidSearchQuery = `#graphql
  query BulkOperationsInvalidSearchValidation {
    bulkOperations(first: 1, query: "created_at:>=not-a-date") {
      nodes {
        id
      }
    }
  }
`;

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

const bulkOperationCancelMissingIdMutation = `#graphql
  mutation BulkOperationCancelMissingIdValidation {
    bulkOperationCancel {
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

const bulkOperationRunQueryMissingQueryMutation = `#graphql
  mutation BulkOperationRunQueryMissingQueryValidation {
    bulkOperationRunQuery {
      userErrors {
        field
        message
      }
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

const invalidBulkQueryWithoutConnection = `#graphql
{
  shop {
    id
  }
}`;

const fixture: Record<string, unknown> = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  pollConfig: {
    pollIntervalMs,
    maxPolls,
  },
  reads: {},
  validations: {},
  lifecycle: {},
};

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
  const node = asRecord(data?.[fieldName]);
  return node;
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

function numericIdFromGid(gid: string): string {
  return gid.slice(gid.lastIndexOf('/') + 1);
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

async function captureRunQueryLifecycle(
  label: string,
  query: string,
  options: { cancelImmediately?: boolean } = {},
): Promise<Record<string, unknown>> {
  const run = await capture('BulkOperationRunQueryCapture', bulkOperationRunQueryMutation, { query });
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

  if (options.cancelImmediately === true) {
    lifecycle['cancelAttempt'] = await capture('BulkOperationCancelCapture', bulkOperationCancelMutation, { id });
  }

  lifecycle['statusPolls'] = await pollBulkOperationToTerminal(id);
  lifecycle['catalogById'] = await capture('BulkOperationsCatalogCapture', bulkOperationsCatalogQuery, {
    first: 5,
    query: `id:${numericIdFromGid(id)}`,
    sortKey: 'CREATED_AT',
    reverse: true,
  });
  lifecycle['currentQueryOperation'] = await capture('CurrentBulkOperationCapture', currentBulkOperationQuery, {
    type: 'QUERY',
  });

  if (options.cancelImmediately !== true) {
    lifecycle['terminalCancelAttempt'] = await capture('BulkOperationCancelCapture', bulkOperationCancelMutation, {
      id,
    });
  }

  const polls = lifecycle['statusPolls'];
  if (Array.isArray(polls)) {
    const statuses = polls
      .map((poll) => readBulkOperationStatus(readBulkOperationFromField(poll as CapturedInteraction, 'bulkOperation')))
      .filter((status): status is string => status !== null);
    lifecycle['observedStatuses'] = statuses;
    lifecycle['observedStatusTransition'] = new Set(statuses).size > 1;
  }

  lifecycle['label'] = label;
  return lifecycle;
}

await mkdir(outputDir, { recursive: true });

fixture['reads'] = {
  unknownId: await capture('BulkOperationByIdCapture', bulkOperationByIdQuery, { id: unknownBulkOperationId }),
  malformedId: await capture('BulkOperationByIdCapture', bulkOperationByIdQuery, { id: malformedBulkOperationId }),
  catalogDefault: await capture('BulkOperationsCatalogCapture', bulkOperationsCatalogQuery, {
    first: 5,
    sortKey: 'CREATED_AT',
    reverse: true,
  }),
  catalogEmptyRunningQuery: await capture('BulkOperationsCatalogCapture', bulkOperationsCatalogQuery, {
    first: 5,
    query: 'status:running operation_type:query',
    sortKey: 'CREATED_AT',
    reverse: true,
  }),
  catalogEmptyRunningMutation: await capture('BulkOperationsCatalogCapture', bulkOperationsCatalogQuery, {
    first: 5,
    query: 'status:running operation_type:mutation',
    sortKey: 'CREATED_AT',
    reverse: true,
  }),
  currentQuery: await capture('CurrentBulkOperationCapture', currentBulkOperationQuery, { type: 'QUERY' }),
  currentMutation: await capture('CurrentBulkOperationCapture', currentBulkOperationQuery, { type: 'MUTATION' }),
};

fixture['validations'] = {
  bulkOperationMissingId: await capture('BulkOperationMissingIdValidation', bulkOperationMissingIdQuery),
  bulkOperationsMissingWindow: await capture('BulkOperationsMissingWindowValidation', bulkOperationsMissingWindowQuery),
  bulkOperationsFirstAndLast: await capture('BulkOperationsFirstAndLastValidation', bulkOperationsFirstAndLastQuery),
  bulkOperationsInvalidSearch: await capture('BulkOperationsInvalidSearchValidation', bulkOperationsInvalidSearchQuery),
  bulkOperationCancelMissingId: await capture(
    'BulkOperationCancelMissingIdValidation',
    bulkOperationCancelMissingIdMutation,
  ),
  bulkOperationCancelUnknownId: await capture('BulkOperationCancelCapture', bulkOperationCancelMutation, {
    id: unknownBulkOperationId,
  }),
  bulkOperationRunQueryMissingQuery: await capture(
    'BulkOperationRunQueryMissingQueryValidation',
    bulkOperationRunQueryMissingQueryMutation,
  ),
  bulkOperationRunQueryWithoutConnection: await capture('BulkOperationRunQueryCapture', bulkOperationRunQueryMutation, {
    query: invalidBulkQueryWithoutConnection,
  }),
};

fixture['lifecycle'] = {
  queryExportToTerminal: await captureRunQueryLifecycle('query export to terminal', safeBulkQuery),
  queryExportImmediateCancel: await captureRunQueryLifecycle(
    'query export with immediate cancel attempt',
    safeBulkQuery,
    {
      cancelImmediately: true,
    },
  ),
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
