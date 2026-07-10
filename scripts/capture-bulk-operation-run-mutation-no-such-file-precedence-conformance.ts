/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { randomUUID } from 'node:crypto';
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

const scenarioId = 'bulk-operation-run-mutation-no-such-file-precedence';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
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
  query CurrentBulkOperationForNoSuchFilePrecedence($type: BulkOperationType!) {
    currentBulkOperation(type: $type) {
      ${bulkOperationFields}
    }
  }
`;

const bulkOperationHydrateQuery =
  'query BulkOperationHydrate($id: ID!) { bulkOperation(id: $id) { id status type errorCode createdAt completedAt objectCount rootObjectCount fileSize url partialDataUrl query } }';

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
  query BulkOperationNoSuchFileCleanupProducts($query: String!) {
    products(first: 20, query: $query) {
      nodes {
        id
        title
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation BulkOperationNoSuchFileCleanupProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const runId = `bulk-no-such-file-precedence-${Date.now()}`;
const productTitle = `Bulk no-such-file precedence ${runId}`;
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

function assertNoSuchFile(result: ConformanceGraphqlResult): void {
  const field = readFieldRecord(readDataRecord(result), 'bulkOperationRunMutation');
  if (field['bulkOperation'] !== null) {
    throw new Error(`Expected missing upload to return bulkOperation: null: ${JSON.stringify(field)}`);
  }
  const errors = field['userErrors'];
  if (!Array.isArray(errors) || errors.length !== 1) {
    throw new Error(`Expected exactly one missing-upload userError: ${JSON.stringify(field)}`);
  }
  const error = errors[0];
  if (typeof error !== 'object' || error === null) {
    throw new Error(`Expected missing-upload userError object: ${JSON.stringify(field)}`);
  }
  const record = error as Record<string, unknown>;
  if (record['field'] !== null || record['code'] !== 'NO_SUCH_FILE') {
    throw new Error(`Expected NO_SUCH_FILE with field null: ${JSON.stringify(field)}`);
  }
  const expectedMessage =
    "The JSONL file could not be found. Try uploading the file again, and check that you've entered the URL correctly for the stagedUploadPath mutation argument.";
  if (record['message'] !== expectedMessage) {
    throw new Error(`Unexpected NO_SUCH_FILE message: ${JSON.stringify(field)}`);
  }
}

async function runRaw(query: string, variables: Record<string, unknown>): Promise<ConformanceGraphqlResult> {
  return await runGraphqlRequest(query, variables);
}

function isNonTerminal(operation: BulkOperation | null): operation is BulkOperation {
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

async function cancelAllNonTerminal(type: 'QUERY' | 'MUTATION'): Promise<void> {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const operation = await currentBulkOperation(type);
    if (!isNonTerminal(operation)) {
      return;
    }
    console.log(`Canceling pre-existing ${type} bulk operation ${operation.id}`);
    await runRaw(bulkOperationCancelMutation, { id: operation.id });
  }
  throw new Error(`Unable to clear pre-existing ${type} bulk operations after 10 attempts.`);
}

function hydrateCallFromOperation(operation: BulkOperation): Record<string, unknown> {
  return {
    operationName: 'BulkOperationHydrate',
    variables: { id: operation.id },
    query: bulkOperationHydrateQuery,
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

function missingPathBeside(stagedUploadPath: string): string {
  const lastSlash = stagedUploadPath.lastIndexOf('/');
  if (lastSlash < 0) {
    return `missing-${randomUUID()}.jsonl`;
  }
  return `${stagedUploadPath.slice(0, lastSlash + 1)}missing-${randomUUID()}.jsonl`;
}

async function cleanupCreatedProducts(): Promise<Array<Record<string, unknown>>> {
  const cleanupLog: Array<Record<string, unknown>> = [];
  const result = await runRaw(cleanupProductsQuery, { query: `title:'${productTitle}'` });
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

await cancelAllNonTerminal('MUTATION');

const operations: BulkOperation[] = [];
const successfulRuns: CapturedInteraction[] = [];
let lastStagedUploadPath: string | null = null;
for (let index = 0; index < 5; index += 1) {
  const jsonl = `${JSON.stringify({ product: { title: productTitle } })}\n`;
  const upload = await createAndUploadBulkMutationVariables(`${runId}-${index + 1}.jsonl`, jsonl);
  lastStagedUploadPath = upload.stagedUploadPath;
  const variables = {
    mutation: innerBulkMutation,
    path: upload.stagedUploadPath,
  };
  const result = await runRaw(bulkOperationRunMutationMutation, variables);
  const operation = readBulkOperationFromPayload(result, 'bulkOperationRunMutation');
  operations.push(operation);
  successfulRuns.push(
    captureResult(
      `BulkOperationRunMutationNoSuchFilePrecedenceSetup${index + 1}`,
      bulkOperationRunMutationMutation,
      variables,
      result,
    ),
  );
}

if (lastStagedUploadPath === null) {
  throw new Error('No staged upload path was created for the missing-file probe.');
}

const missingVariables = {
  mutation: innerBulkMutation,
  path: missingPathBeside(lastStagedUploadPath),
};
const missingRunResult = await runRaw(bulkOperationRunMutationMutation, missingVariables);
assertNoSuchFile(missingRunResult);
const missingRun = captureResult(
  'BulkOperationRunMutationNoSuchFilePrecedence',
  bulkOperationRunMutationMutation,
  missingVariables,
  missingRunResult,
);

const cancelAttempts: CapturedInteraction[] = [];
for (const operation of operations) {
  const cancelVariables = { id: operation.id };
  const cancelResult = await runRaw(bulkOperationCancelMutation, cancelVariables);
  cancelAttempts.push(
    captureResult('BulkOperationCancelParity', bulkOperationCancelMutation, cancelVariables, cancelResult),
  );
}
const cleanup = await cleanupCreatedProducts();

const fixture = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  productTitle,
  successfulRuns,
  missingRun,
  cancelAttempts,
  cleanup,
  upstreamCalls: operations.map(hydrateCallFromOperation),
  notes:
    'Captures Shopify Admin GraphQL 2026-04 returning NO_SUCH_FILE for a missing bulkOperationRunMutation stagedUploadPath even when five same-type mutation bulk operations are already non-terminal. The executable replay hydrates those non-terminal operations through public bulkOperationCancel requests using the captured upstream cassette before asserting the missing-file userError.',
};

await mkdir(outputDir, { recursive: true });
await writeFile(path.join(outputDir, `${scenarioId}.json`), `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${path.join(outputDir, `${scenarioId}.json`)}`);
