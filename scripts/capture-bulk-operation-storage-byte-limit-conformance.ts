/* oxlint-disable no-console -- CLI capture script writes status to stdout/stderr. */
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

type StagedTarget = {
  url: string;
  resourceUrl: string;
  parameters: Array<{ name: string; value: string }>;
};

type BulkOperationPayload = {
  bulkOperation: unknown;
  userErrors: Array<{
    field: string[] | null;
    message: string;
    code: string | null;
  }>;
};

const scenarioId = 'bulk-operation-storage-byte-limit';
const storageByteLimit = 65_535;
const oversizedByteLength = storageByteLimit + 1;
const oversizedQueryMessage = `Query is too large (${oversizedByteLength} bytes; maximum is ${storageByteLimit} bytes)`;
const oversizedMutationMessage = `is too large (${oversizedByteLength} bytes; maximum is ${storageByteLimit} bytes)`;

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

const runQueryOperationName = 'BulkOperationStorageByteLimitQuery';
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

const stagedUploadOperationName = 'BulkOperationStorageByteLimitStagedUpload';
const stagedUploadDocument = `mutation ${stagedUploadOperationName}($input: [StagedUploadInput!]!) {
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

const runMutationOperationName = 'BulkOperationStorageByteLimitMutation';
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

function paddedDocumentForBytes(body: string, targetBytes: number, pad = 'a'): string {
  const fixedBytes = Buffer.byteLength('#\n', 'utf8') + Buffer.byteLength(body, 'utf8');
  if (targetBytes < fixedBytes) {
    throw new Error(`target byte count ${targetBytes} cannot fit body byte count ${fixedBytes}`);
  }
  const padBytes = Buffer.byteLength(pad, 'utf8');
  const paddingBytes = targetBytes - fixedBytes;
  if (paddingBytes % padBytes !== 0) {
    throw new Error(`padding byte count ${paddingBytes} does not align with pad byte count ${padBytes}`);
  }
  const document = `#${pad.repeat(paddingBytes / padBytes)}\n${body}`;
  const actualBytes = Buffer.byteLength(document, 'utf8');
  if (actualBytes !== targetBytes) {
    throw new Error(`expected ${targetBytes} bytes, got ${actualBytes}`);
  }
  return document;
}

function capture(
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

function payloadFrom(
  response: unknown,
  rootField: 'bulkOperationRunQuery' | 'bulkOperationRunMutation',
): BulkOperationPayload {
  const payload = response as {
    data?: Partial<Record<typeof rootField, BulkOperationPayload>>;
  };
  const result = payload.data?.[rootField];
  if (!result) {
    throw new Error(`Missing ${rootField} payload: ${JSON.stringify(response)}`);
  }
  return result;
}

function assertStorageLimitError(
  name: string,
  response: unknown,
  rootField: 'bulkOperationRunQuery' | 'bulkOperationRunMutation',
  code: string,
  message: string,
): void {
  const payload = payloadFrom(response, rootField);
  const [error] = payload.userErrors;
  if (
    payload.bulkOperation !== null ||
    payload.userErrors.length !== 1 ||
    !Array.isArray(error?.field) ||
    error.field.length !== 1 ||
    error.field[0] !== 'query' ||
    error.message !== message ||
    error.code !== code
  ) {
    throw new Error(`${name} returned unexpected payload: ${JSON.stringify(payload)}`);
  }
}

function readStagedTarget(response: unknown): StagedTarget {
  const payload = response as {
    data?: {
      stagedUploadsCreate?: {
        stagedTargets?: StagedTarget[] | null;
        userErrors?: unknown[] | null;
      };
    };
  };
  const result = payload.data?.stagedUploadsCreate;
  if (!result || (result.userErrors?.length ?? 0) > 0) {
    throw new Error(`stagedUploadsCreate failed: ${JSON.stringify(response)}`);
  }
  const [target] = result.stagedTargets ?? [];
  if (!target) {
    throw new Error(`stagedUploadsCreate response did not include a staged target: ${JSON.stringify(response)}`);
  }
  return target;
}

function stagedUploadPath(target: StagedTarget): string {
  const key = target.parameters.find((parameter) => parameter.name === 'key')?.value;
  if (!key) {
    throw new Error(`staged target did not include key parameter: ${JSON.stringify(target)}`);
  }
  return key;
}

async function uploadJsonl(
  target: StagedTarget,
  filename: string,
  jsonl: string,
): Promise<{ status: number; body: string }> {
  const form = new FormData();
  for (const parameter of target.parameters) {
    form.append(parameter.name, parameter.value);
  }
  form.append('file', new Blob([jsonl], { type: 'text/jsonl' }), filename);

  const response = await fetch(target.url, { method: 'POST', body: form });
  return { status: response.status, body: await response.text() };
}

const oversizedQuery = paddedDocumentForBytes('{ products { edges { node { id } } } }', oversizedByteLength);
const oversizedMutation = paddedDocumentForBytes(
  'mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id } userErrors { field message } } }',
  oversizedByteLength,
);

const stagedUploadVariables = {
  input: [
    {
      filename: 'bulk-operation-storage-byte-limit.jsonl',
      mimeType: 'text/jsonl',
      resource: 'BULK_MUTATION_VARIABLES',
      httpMethod: 'POST',
    },
  ],
};

console.log(`[${scenarioId}] creating staged upload target for oversized bulk mutation validation`);
const stagedUploadResult = await runGraphqlRequest(stagedUploadDocument, stagedUploadVariables);
if (stagedUploadResult.status !== 200) {
  throw new Error(
    `stagedUploadsCreate returned HTTP ${stagedUploadResult.status}: ${JSON.stringify(stagedUploadResult.payload)}`,
  );
}
const stagedTarget = readStagedTarget(stagedUploadResult.payload);
const stagedUploadKey = stagedUploadPath(stagedTarget);
const uploadResult = await uploadJsonl(
  stagedTarget,
  'bulk-operation-storage-byte-limit.jsonl',
  `${JSON.stringify({ product: { title: 'Storage Byte Limit Probe' } })}\n`,
);
if (uploadResult.status < 200 || uploadResult.status >= 300) {
  throw new Error(`staged upload returned HTTP ${uploadResult.status}: ${uploadResult.body}`);
}

const queryVariables = { query: oversizedQuery };
const mutationVariables = { mutation: oversizedMutation, path: stagedUploadKey };

console.log(`[${scenarioId}] capturing oversized bulkOperationRunQuery validation`);
const queryResult = await runGraphqlRequest(runQueryDocument, queryVariables);
if (queryResult.status !== 200) {
  throw new Error(`bulkOperationRunQuery returned HTTP ${queryResult.status}: ${JSON.stringify(queryResult.payload)}`);
}
assertStorageLimitError(
  'bulkOperationRunQuery',
  queryResult.payload,
  'bulkOperationRunQuery',
  'INVALID',
  oversizedQueryMessage,
);

console.log(`[${scenarioId}] capturing oversized bulkOperationRunMutation validation`);
const mutationResult = await runGraphqlRequest(runMutationDocument, mutationVariables);
if (mutationResult.status !== 200) {
  throw new Error(
    `bulkOperationRunMutation returned HTTP ${mutationResult.status}: ${JSON.stringify(mutationResult.payload)}`,
  );
}
assertStorageLimitError(
  'bulkOperationRunMutation',
  mutationResult.payload,
  'bulkOperationRunMutation',
  'INVALID_MUTATION',
  oversizedMutationMessage,
);

const fixture = {
  scenarioId,
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  byteLimit: storageByteLimit,
  setup: {
    stagedUploadTarget: capture(
      stagedUploadOperationName,
      stagedUploadDocument,
      stagedUploadVariables,
      stagedUploadResult,
    ),
    stagedUpload: uploadResult,
  },
  cases: {
    query: capture(runQueryOperationName, runQueryDocument, queryVariables, queryResult),
    mutation: capture(runMutationOperationName, runMutationDocument, mutationVariables, mutationResult),
  },
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`[${scenarioId}] wrote ${outputPath}`);
