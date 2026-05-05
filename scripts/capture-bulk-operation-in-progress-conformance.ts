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

type BulkOperation = {
  id: string;
  status: string;
  type: string;
  errorCode?: string | null;
  createdAt: string;
  completedAt?: string | null;
  objectCount: string;
  rootObjectCount: string;
  fileSize?: string | null;
  url?: string | null;
  partialDataUrl?: string | null;
  query?: string | null;
};

type StagedTarget = {
  url: string | null;
  resourceUrl: string | null;
  parameters: Array<{ name: string; value: string }>;
};

const queryScenarioId = 'bulk-operation-run-query-operation-in-progress';
const mutationScenarioId = 'bulk-operation-run-mutation-operation-in-progress';
const configEnv = {
  ...process.env,
  SHOPIFY_CONFORMANCE_API_VERSION: process.env['SHOPIFY_CONFORMANCE_BULK_API_VERSION'] ?? '2025-01',
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  env: configEnv,
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'bulk-operations');

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

const currentBulkOperationQuery = `#graphql
  query CurrentBulkOperationForThrottleCapture($type: BulkOperationType!) {
    currentBulkOperation(type: $type) {
      ${bulkOperationFields}
    }
  }
`;

const bulkOperationCancelMutation = `#graphql
  mutation BulkOperationCancelParity($id: ID!) {
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
  mutation BulkOperationRunQueryOperationInProgress($query: String!) {
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
  }
`;

const stagedUploadsCreateMutation = `#graphql
  mutation BulkOperationMutationUploadTarget($input: [StagedUploadInput!]!) {
    stagedUploadsCreate(input: $input) {
      stagedTargets {
        url
        resourceUrl
        parameters {
          name
          value
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const bulkOperationRunMutationMutation = `#graphql
  mutation BulkOperationRunMutationOperationInProgress($mutation: String!, $path: String!) {
    bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
      bulkOperation {
        ${bulkOperationFields}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const cleanupProductsQuery = `#graphql
  query BulkOperationThrottleCleanupProducts($query: String!) {
    products(first: 20, query: $query) {
      nodes {
        id
        title
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation BulkOperationThrottleCleanupProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const runId = `har-752-${Date.now()}`;
const exportQuery = `#graphql
{
  products {
    edges {
      node {
        id
      }
    }
  }
}`;
const productTitle = `HAR-752 bulk throttle ${runId}`;
const innerBulkMutation = `#graphql
mutation ProductCreate($product: ProductCreateInput!) {
  productCreate(product: $product) {
    product {
      id
      title
    }
    userErrors {
      field
      message
    }
  }
}`;

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

function readPayloadRecord(result: ConformanceGraphqlResult): Record<string, unknown> {
  if (typeof result.payload !== 'object' || result.payload === null) {
    throw new Error('GraphQL response payload was not an object.');
  }
  return result.payload as Record<string, unknown>;
}

function readDataRecord(result: ConformanceGraphqlResult): Record<string, unknown> {
  const payload = readPayloadRecord(result);
  const data = payload['data'];
  if (typeof data !== 'object' || data === null) {
    throw new Error(`GraphQL response did not include object data: ${JSON.stringify(payload)}`);
  }
  return data as Record<string, unknown>;
}

function readFieldRecord(data: Record<string, unknown>, fieldName: string): Record<string, unknown> {
  const value = data[fieldName];
  if (typeof value !== 'object' || value === null) {
    throw new Error(`Expected ${fieldName} to be an object: ${JSON.stringify(data)}`);
  }
  return value as Record<string, unknown>;
}

function readBulkOperationFromPayload(result: ConformanceGraphqlResult, fieldName: string): BulkOperation {
  const field = readFieldRecord(readDataRecord(result), fieldName);
  const operation = field['bulkOperation'];
  if (typeof operation !== 'object' || operation === null) {
    throw new Error(`${fieldName} did not return a BulkOperation: ${JSON.stringify(field)}`);
  }
  const record = operation as Record<string, unknown>;
  if (
    typeof record['id'] !== 'string' ||
    typeof record['status'] !== 'string' ||
    typeof record['type'] !== 'string' ||
    typeof record['createdAt'] !== 'string' ||
    typeof record['objectCount'] !== 'string' ||
    typeof record['rootObjectCount'] !== 'string'
  ) {
    throw new Error(`${fieldName} returned an incomplete BulkOperation: ${JSON.stringify(record)}`);
  }
  return record as BulkOperation;
}

function readUserErrorCode(result: ConformanceGraphqlResult, fieldName: string): string | null {
  const field = readFieldRecord(readDataRecord(result), fieldName);
  const errors = field['userErrors'];
  if (!Array.isArray(errors) || errors.length === 0) {
    return null;
  }
  const first = errors[0];
  if (typeof first !== 'object' || first === null) {
    return null;
  }
  const code = (first as Record<string, unknown>)['code'];
  return typeof code === 'string' ? code : null;
}

function assertOperationInProgress(result: ConformanceGraphqlResult, fieldName: string): void {
  const code = readUserErrorCode(result, fieldName);
  if (code !== 'OPERATION_IN_PROGRESS') {
    throw new Error(`${fieldName} did not return OPERATION_IN_PROGRESS: ${JSON.stringify(result.payload)}`);
  }
}

async function runRaw(query: string, variables: Record<string, unknown>): Promise<ConformanceGraphqlResult> {
  return await runGraphqlRequest(query, variables);
}

function isNonTerminal(operation: BulkOperation | null): boolean {
  return operation !== null && !['CANCELED', 'COMPLETED', 'EXPIRED', 'FAILED'].includes(operation.status);
}

async function currentBulkOperation(type: 'QUERY' | 'MUTATION'): Promise<BulkOperation | null> {
  const result = await runRaw(currentBulkOperationQuery, { type });
  const operation = readDataRecord(result)['currentBulkOperation'];
  if (operation === null || operation === undefined) {
    return null;
  }
  if (typeof operation !== 'object') {
    throw new Error(`currentBulkOperation(${type}) returned non-object value: ${JSON.stringify(operation)}`);
  }
  return operation as BulkOperation;
}

async function cancelIfNonTerminal(type: 'QUERY' | 'MUTATION'): Promise<void> {
  const operation = await currentBulkOperation(type);
  if (operation === null || !isNonTerminal(operation)) {
    return;
  }
  console.log(`Canceling pre-existing ${type} bulk operation ${operation.id}`);
  await runRaw(bulkOperationCancelMutation, { id: operation.id });
}

function hydrateCallFromOperation(operation: BulkOperation): Record<string, unknown> {
  return {
    operationName: 'BulkOperationHydrate',
    variables: { id: operation.id },
    query: 'hand-synthesized from captured first run operation',
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

async function captureQueryScenario(): Promise<Record<string, unknown>> {
  await cancelIfNonTerminal('QUERY');

  const variables = { query: exportQuery };
  const firstRunResult = await runRaw(bulkOperationRunQueryMutation, variables);
  const firstOperation = readBulkOperationFromPayload(firstRunResult, 'bulkOperationRunQuery');
  const firstRun = captureResult(
    'BulkOperationRunQueryOperationInProgress',
    bulkOperationRunQueryMutation,
    variables,
    firstRunResult,
  );

  const secondRunResult = await runRaw(bulkOperationRunQueryMutation, variables);
  assertOperationInProgress(secondRunResult, 'bulkOperationRunQuery');
  const secondRun = captureResult(
    'BulkOperationRunQueryOperationInProgress',
    bulkOperationRunQueryMutation,
    variables,
    secondRunResult,
  );

  const cancelVariables = { id: firstOperation.id };
  const cancelResult = await runRaw(bulkOperationCancelMutation, cancelVariables);
  const cancelAttempt = captureResult(
    'BulkOperationCancelParity',
    bulkOperationCancelMutation,
    cancelVariables,
    cancelResult,
  );

  return {
    scenarioId: queryScenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    firstRun,
    secondRun,
    cancelAttempt,
    upstreamCalls: [hydrateCallFromOperation(firstOperation)],
    notes:
      'Captured on an API version that still enforces one in-progress query bulk operation per app/shop. The parity spec records the two consecutive runQuery behavior, then uses cassette-backed cancel hydration to create the local non-terminal operation without pre-seeding.',
  };
}

function readFirstStagedTarget(result: ConformanceGraphqlResult): StagedTarget {
  const field = readFieldRecord(readDataRecord(result), 'stagedUploadsCreate');
  const errors = field['userErrors'];
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`stagedUploadsCreate returned userErrors: ${JSON.stringify(errors)}`);
  }
  const targets = field['stagedTargets'];
  if (!Array.isArray(targets) || targets.length === 0) {
    throw new Error(`stagedUploadsCreate did not return a target: ${JSON.stringify(field)}`);
  }
  const target = targets[0];
  if (typeof target !== 'object' || target === null) {
    throw new Error(`stagedUploadsCreate target was not an object: ${JSON.stringify(target)}`);
  }
  return target as StagedTarget;
}

function readStagedUploadPath(target: StagedTarget): string {
  const key = target.parameters.find((parameter) => parameter.name === 'key')?.value;
  if (!key) {
    throw new Error(`staged upload target did not include a key parameter: ${JSON.stringify(target)}`);
  }
  return key;
}

async function createAndUploadBulkMutationVariables(
  filename: string,
  content: string,
): Promise<{
  stagedUploadPath: string;
}> {
  const input = [
    {
      resource: 'BULK_MUTATION_VARIABLES',
      filename,
      mimeType: 'text/jsonl',
      httpMethod: 'POST',
    },
  ];
  const createResult = await runRaw(stagedUploadsCreateMutation, { input });
  const target = readFirstStagedTarget(createResult);
  if (!target.url) {
    throw new Error(`staged upload target did not include a URL: ${JSON.stringify(target)}`);
  }

  const form = new FormData();
  for (const parameter of target.parameters) {
    form.append(parameter.name, parameter.value);
  }
  form.append('file', new Blob([content], { type: 'text/jsonl' }), filename);

  const upload = await fetch(target.url, {
    method: 'POST',
    body: form,
  });
  if (upload.status < 200 || upload.status >= 300) {
    throw new Error(`staged upload failed with HTTP ${upload.status}: ${await upload.text()}`);
  }

  return {
    stagedUploadPath: readStagedUploadPath(target),
  };
}

async function cleanupCreatedProducts(): Promise<Array<Record<string, unknown>>> {
  const cleanupLog: Array<Record<string, unknown>> = [];
  const search = `title:'${productTitle}'`;
  const result = await runRaw(cleanupProductsQuery, { query: search });
  const products = readFieldRecord(readDataRecord(result), 'products')['nodes'];
  if (!Array.isArray(products)) {
    return cleanupLog;
  }

  for (const product of products) {
    if (typeof product !== 'object' || product === null) {
      continue;
    }
    const record = product as Record<string, unknown>;
    if (typeof record['id'] !== 'string' || record['title'] !== productTitle) {
      continue;
    }
    const deleteResult = await runRaw(productDeleteMutation, { input: { id: record['id'] } });
    cleanupLog.push({
      productId: record['id'],
      response: deleteResult.payload,
    });
  }

  return cleanupLog;
}

async function captureMutationScenario(): Promise<Record<string, unknown>> {
  await cancelIfNonTerminal('MUTATION');

  const jsonl = `${JSON.stringify({ product: { title: productTitle } })}\n`;
  const firstUpload = await createAndUploadBulkMutationVariables(`${runId}-first.jsonl`, jsonl);
  const secondUpload = await createAndUploadBulkMutationVariables(`${runId}-second.jsonl`, jsonl);

  const firstVariables = {
    mutation: innerBulkMutation,
    path: firstUpload.stagedUploadPath,
  };
  const firstRunResult = await runRaw(bulkOperationRunMutationMutation, firstVariables);
  const firstOperation = readBulkOperationFromPayload(firstRunResult, 'bulkOperationRunMutation');
  const firstRun = captureResult(
    'BulkOperationRunMutationOperationInProgress',
    bulkOperationRunMutationMutation,
    firstVariables,
    firstRunResult,
  );

  const secondVariables = {
    mutation: innerBulkMutation,
    path: secondUpload.stagedUploadPath,
  };
  const secondRunResult = await runRaw(bulkOperationRunMutationMutation, secondVariables);
  assertOperationInProgress(secondRunResult, 'bulkOperationRunMutation');
  const secondRun = captureResult(
    'BulkOperationRunMutationOperationInProgress',
    bulkOperationRunMutationMutation,
    secondVariables,
    secondRunResult,
  );

  const cancelVariables = { id: firstOperation.id };
  const cancelResult = await runRaw(bulkOperationCancelMutation, cancelVariables);
  const cancelAttempt = captureResult(
    'BulkOperationCancelParity',
    bulkOperationCancelMutation,
    cancelVariables,
    cancelResult,
  );
  const cleanup = await cleanupCreatedProducts();

  return {
    scenarioId: mutationScenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    productTitle,
    firstRun,
    secondRun,
    cancelAttempt,
    cleanup,
    upstreamCalls: [hydrateCallFromOperation(firstOperation)],
    notes:
      'Captured with two uploaded JSONL variable files. The parity spec records consecutive runMutation OPERATION_IN_PROGRESS behavior, then uses cassette-backed cancel hydration to create the local non-terminal operation without pre-seeding; local throttle validation happens before staged upload lookup.',
  };
}

const queryFixture = await captureQueryScenario();
const mutationFixture = await captureMutationScenario();

await mkdir(outputDir, { recursive: true });
await writeFile(path.join(outputDir, `${queryScenarioId}.json`), `${JSON.stringify(queryFixture, null, 2)}\n`, 'utf8');
await writeFile(
  path.join(outputDir, `${mutationScenarioId}.json`),
  `${JSON.stringify(mutationFixture, null, 2)}\n`,
  'utf8',
);

console.log(`Wrote ${path.join(outputDir, `${queryScenarioId}.json`)}`);
console.log(`Wrote ${path.join(outputDir, `${mutationScenarioId}.json`)}`);
