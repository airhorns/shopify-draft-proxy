/* oxlint-disable no-console -- CLI capture script writes status to stdout/stderr. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedGraphqlRequest = {
  operationName: string | null;
  query: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    payload: unknown;
  };
};

const scenarioId = 'graphql-request-execution-operation-name-defaults';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

async function readRequestDocument(name: string): Promise<string> {
  return await readFile(path.join('config', 'parity-requests', 'admin-platform', name), 'utf8');
}

async function runGraphqlJsonBody({
  operationName,
  query,
  variables,
}: {
  operationName: string | null;
  query: string;
  variables: Record<string, unknown>;
}): Promise<CapturedGraphqlRequest> {
  const body: Record<string, unknown> = { query, variables };
  if (operationName !== null) body['operationName'] = operationName;

  const response = await fetch(`${adminOrigin}/admin/api/${apiVersion}/graphql.json`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...buildAdminAuthHeaders(adminAccessToken),
    },
    body: JSON.stringify(body),
  });

  return {
    operationName,
    query,
    variables,
    response: {
      status: response.status,
      payload: await response.json(),
    },
  };
}

const multipleQueriesDocument = await readRequestDocument('graphql-operation-multiple-queries.graphql');
const selectedProductsZeroDocument = await readRequestDocument('graphql-operation-selected-products-zero.graphql');
const defaultZeroDocument = await readRequestDocument('graphql-variable-default-zero-products.graphql');

console.log(`[${scenarioId}] capturing missing operationName error`);
const missingOperationName = await runGraphqlJsonBody({
  operationName: null,
  query: multipleQueriesDocument,
  variables: {},
});

console.log(`[${scenarioId}] capturing unknown operationName error`);
const unknownOperationName = await runGraphqlJsonBody({
  operationName: 'Missing',
  query: multipleQueriesDocument,
  variables: {},
});

console.log(`[${scenarioId}] capturing selected operationName products(first: 0) branch`);
const selectedOperationName = await runGraphqlJsonBody({
  operationName: 'Selected',
  query: selectedProductsZeroDocument,
  variables: {},
});

console.log(`[${scenarioId}] capturing omitted scalar variable default`);
const scalarDefault = await runGraphqlJsonBody({
  operationName: null,
  query: defaultZeroDocument,
  variables: {},
});

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  cases: {
    missingOperationName,
    unknownOperationName,
    selectedOperationName,
    scalarDefault,
  },
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`[${scenarioId}] wrote ${outputPath}`);
