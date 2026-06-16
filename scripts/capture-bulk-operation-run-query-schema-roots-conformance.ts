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

const scenarioId = 'bulk-operation-run-query-schema-roots';
const terminalStatuses = new Set(['CANCELED', 'COMPLETED', 'EXPIRED', 'FAILED']);
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

const bulkOperationRunQueryMutation = `#graphql
  mutation BulkOperationRunQueryProxyFallback($query: String!) {
    bulkOperationRunQuery(query: $query) {
      bulkOperation {
        id
        status
        type
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const currentBulkOperationQuery = `#graphql
  query CurrentBulkQueryOperationForSchemaRootCapture {
    currentBulkOperation(type: QUERY) {
      id
      status
      type
    }
  }
`;

const bulkOperationCancelMutation = `#graphql
  mutation BulkOperationSchemaRootCleanupCancel($id: ID!) {
    bulkOperationCancel(id: $id) {
      bulkOperation {
        id
        status
        type
      }
      userErrors {
        field
        message
      }
    }
  }
`;

// Canonical mutation text the proxy forwards upstream for schema-valid bulk query roots
// it does not synthesize locally. The strict parity cassette matches recorded `query`
// text exactly, so this must stay byte-identical to the proxy's
// BULK_OPERATION_RUN_QUERY_PROXY_FALLBACK_QUERY constant.
const proxyFallbackQuery =
  'mutation BulkOperationRunQueryProxyFallback($query: String!) { bulkOperationRunQuery(query: $query) { bulkOperation { id status type } userErrors { field message code } } }';

const schemaRootQueries = {
  ordersRoot: `#graphql
{
  orders {
    edges {
      node {
        id
        name
      }
    }
  }
}`,
  draftOrdersWarningsList: `#graphql
{
  draftOrders {
    edges {
      node {
        id
        warnings {
          message
        }
      }
    }
  }
}`,
};

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
  return asRecord(asRecord(interaction.response)?.['data']);
}

function readBulkOperationFromPayload(interaction: CapturedInteraction, payloadName: string): BulkOperationNode | null {
  const payload = asRecord(readData(interaction)?.[payloadName]);
  return asRecord(payload?.['bulkOperation']);
}

function readCurrentBulkOperation(interaction: CapturedInteraction): BulkOperationNode | null {
  return asRecord(readData(interaction)?.['currentBulkOperation']);
}

function readBulkOperationId(node: BulkOperationNode | null): string | null {
  return typeof node?.id === 'string' ? node.id : null;
}

function readBulkOperationStatus(node: BulkOperationNode | null): string | null {
  return typeof node?.status === 'string' ? node.status : null;
}

function isActiveOperation(node: BulkOperationNode | null): node is { id: string; status: string } {
  const id = readBulkOperationId(node);
  const status = readBulkOperationStatus(node);
  return id !== null && status !== null && !terminalStatuses.has(status);
}

async function cleanupActiveBulkQueryOperation(label: string): Promise<Record<string, unknown>> {
  const before = await capture(`${label}CurrentBulkOperationBefore`, currentBulkOperationQuery);
  const current = readCurrentBulkOperation(before);
  const cleanup: Record<string, unknown> = { before };
  if (!isActiveOperation(current)) {
    return cleanup;
  }

  cleanup['cancel'] = await capture(`${label}CancelActiveBulkOperation`, bulkOperationCancelMutation, {
    id: current.id,
  });
  const polls: CapturedInteraction[] = [];
  for (let index = 0; index < 10; index += 1) {
    await sleep(500);
    const poll = await capture(`${label}CurrentBulkOperationPoll${index + 1}`, currentBulkOperationQuery);
    polls.push(poll);
    if (!isActiveOperation(readCurrentBulkOperation(poll))) {
      break;
    }
  }
  cleanup['polls'] = polls;
  return cleanup;
}

async function captureRunQueryCase(name: keyof typeof schemaRootQueries): Promise<Record<string, unknown>> {
  const beforeCleanup = await cleanupActiveBulkQueryOperation(`${name}Before`);
  const variables = {
    query: schemaRootQueries[name],
  };
  const run = await capture('BulkOperationRunQueryProxyFallback', bulkOperationRunQueryMutation, variables);
  const bulkOperation = readBulkOperationFromPayload(run, 'bulkOperationRunQuery');
  const id = readBulkOperationId(bulkOperation);
  const afterCleanup =
    id === null
      ? { skipped: 'bulkOperationRunQuery did not return a BulkOperation id.' }
      : {
          cancel: await capture(`${name}CancelCapturedBulkOperation`, bulkOperationCancelMutation, { id }),
          settle: await cleanupActiveBulkQueryOperation(`${name}After`),
        };

  return {
    bulkQuery: schemaRootQueries[name],
    beforeCleanup,
    run,
    afterCleanup,
  };
}

const ordersRoot = await captureRunQueryCase('ordersRoot');
const draftOrdersWarningsList = await captureRunQueryCase('draftOrdersWarningsList');

const fixture: Record<string, unknown> = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  cases: {
    ordersRoot,
    draftOrdersWarningsList,
  },
  upstreamCalls: [
    {
      operationName: 'BulkOperationRunQueryProxyFallback',
      query: proxyFallbackQuery,
      variables: asRecord(asRecord(ordersRoot['run'])?.['variables']) ?? {},
      response: {
        status: asRecord(ordersRoot['run'])?.['status'] ?? 200,
        body: asRecord(ordersRoot['run'])?.['response'] ?? {},
      },
    },
    {
      operationName: 'BulkOperationRunQueryProxyFallback',
      query: proxyFallbackQuery,
      variables: asRecord(asRecord(draftOrdersWarningsList['run'])?.['variables']) ?? {},
      response: {
        status: asRecord(draftOrdersWarningsList['run'])?.['status'] ?? 200,
        body: asRecord(draftOrdersWarningsList['run'])?.['response'] ?? {},
      },
    },
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
