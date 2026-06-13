/* oxlint-disable no-console -- CLI capture script writes status to stdout/stderr. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedInteraction = {
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  status: number;
  response: unknown;
};

const scenarioId = 'bulk-operation-name-independent-run-roots';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
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

const runQueryOperationName = 'RunBulkExport';
const runQueryDocument = `mutation ${runQueryOperationName}($query: String!) {
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

const runMutationOperationName = 'RunBulkImport';
const runMutationDocument = `mutation ${runMutationOperationName}($mutation: String!, $path: String!) {
  bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
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

function capture(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
  result: { status: number; payload: unknown },
): CapturedInteraction {
  return {
    operationName,
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

const queryVariables = { query: '{ shop { id } }' };
const mutationVariables = {
  mutation:
    'mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
  path: 'tmp/92891250994/bulk/missing/name-independent.jsonl',
};

console.log(`[${scenarioId}] capturing ordinary-name bulkOperationRunQuery validation`);
const queryResult = await runGraphqlRequest(runQueryDocument, queryVariables);

console.log(`[${scenarioId}] capturing ordinary-name bulkOperationRunMutation validation`);
const mutationResult = await runGraphqlRequest(runMutationDocument, mutationVariables);

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  cases: {
    query: capture(runQueryOperationName, runQueryDocument, queryVariables, queryResult),
    mutation: capture(runMutationOperationName, runMutationDocument, mutationVariables, mutationResult),
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`[${scenarioId}] wrote ${outputPath}`);
