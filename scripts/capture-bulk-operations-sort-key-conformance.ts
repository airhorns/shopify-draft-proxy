/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

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

const scenarioId = 'bulk-operations-sort-key';
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
`;

const sortKeyDocumentPath = path.join(requestDir, 'bulk-operations-sort-key.graphql');
const idRejectedDocumentPath = path.join(requestDir, 'bulk-operations-sort-key-id-rejected.graphql');

const sortKeyQuery = `#graphql
query BulkOperationsSortKeyCapture($sortKey: BulkOperationsSortKeys!, $reverse: Boolean!) {
  bulkOperations(first: 50, sortKey: $sortKey, reverse: $reverse) {
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

const idRejectedQuery = `#graphql
query BulkOperationsSortKeyIdRejected {
  bulkOperations(first: 5, sortKey: ID) {
    nodes {
      id
    }
  }
}
`;

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

const cases = {
  completedAtDesc: await capture('BulkOperationsSortKeyCapture', sortKeyQuery, {
    sortKey: 'COMPLETED_AT',
    reverse: false,
  }),
  completedAtAsc: await capture('BulkOperationsSortKeyCapture', sortKeyQuery, {
    sortKey: 'COMPLETED_AT',
    reverse: true,
  }),
  createdAtDesc: await capture('BulkOperationsSortKeyCapture', sortKeyQuery, {
    sortKey: 'CREATED_AT',
    reverse: false,
  }),
  createdAtAsc: await capture('BulkOperationsSortKeyCapture', sortKeyQuery, {
    sortKey: 'CREATED_AT',
    reverse: true,
  }),
  idRejected: await capture('BulkOperationsSortKeyIdRejected', idRejectedQuery, {}),
};

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
      cases,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
await writeFile(sortKeyDocumentPath, sortKeyQuery);
await writeFile(idRejectedDocumentPath, idRejectedQuery);
await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`);

console.log(`Wrote ${outputPath}`);
console.log(`Wrote ${sortKeyDocumentPath}`);
console.log(`Wrote ${idRejectedDocumentPath}`);
console.log(`Wrote ${specPath}`);

function buildSpec(): Record<string, unknown> {
  return {
    scenarioId,
    operationNames: ['bulkOperations'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'sort-order-parity', 'graphql-validation-parity'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/bulk_operations_test.gleam'],
    proxyRequest: {
      documentPath: sortKeyDocumentPath,
      apiVersion,
      variablesCapturePath: '$.cases.completedAtDesc.variables',
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Captured Shopify Admin GraphQL bulkOperations ordering for public CREATED_AT and COMPLETED_AT sort keys in both directions plus public-schema rejection for hidden ID sort key. Sort comparisons are strict over the full selected connection payload.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        sortTarget('completed-at-desc', 'completedAtDesc'),
        sortTarget('completed-at-asc', 'completedAtAsc'),
        sortTarget('created-at-desc', 'createdAtDesc'),
        sortTarget('created-at-asc', 'createdAtAsc'),
        {
          name: 'id-sort-key-rejected',
          capturePath: '$.cases.idRejected.response.errors',
          proxyPath: '$.errors',
          proxyRequest: {
            documentPath: idRejectedDocumentPath,
            apiVersion,
            variablesCapturePath: '$.cases.idRejected.variables',
          },
        },
      ],
    },
  };
}

function sortTarget(name: string, caseName: string): Record<string, unknown> {
  return {
    name,
    capturePath: `$.cases.${caseName}.response.data.bulkOperations`,
    proxyPath: '$.data.bulkOperations',
    proxyRequest: {
      documentPath: sortKeyDocumentPath,
      apiVersion,
      variablesCapturePath: `$.cases.${caseName}.variables`,
    },
  };
}
