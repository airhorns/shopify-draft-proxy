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

const scenarioId = 'bulk-operation-run-query-user-error-codes';
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

const bulkOperationRunQueryUserErrorCodesMutation = `mutation BulkOperationRunQueryUserErrorCodes($query: String!) {
  bulkOperationRunQuery(query: $query) {
    bulkOperation {
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

function captureResult(variables: Record<string, unknown>, result: ConformanceGraphqlResult): CapturedInteraction {
  return {
    operationName: 'BulkOperationRunQueryUserErrorCodes',
    query: bulkOperationRunQueryUserErrorCodesMutation,
    variables,
    status: result.status,
    response: result.payload,
  };
}

async function captureValidation(query: string): Promise<CapturedInteraction> {
  const variables = { query };
  const result = await runGraphqlRequest(bulkOperationRunQueryUserErrorCodesMutation, variables);
  return captureResult(variables, result);
}

const fixture: Record<string, unknown> = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  validations: {
    noConnectionWithCode: await captureValidation('{ shop { id } }'),
    emptyQuery: await captureValidation(''),
  },
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
