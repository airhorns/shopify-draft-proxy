import 'dotenv/config';

/* oxlint-disable no-console -- CLI capture script writes status to stdout/stderr. */

import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { runAdminGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type StagedTarget = {
  url: string;
  resourceUrl: string;
  parameters: Array<{ name: string; value: string }>;
};

type GraphqlCapture = {
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  status: number;
  response: unknown;
};

type GraphqlContext = {
  adminOrigin: string;
  apiVersion: string;
  headers: Record<string, string>;
};

type BulkOperationNode = {
  id?: string | null;
  status?: string | null;
  type?: string | null;
};

type UploadCapture = {
  status: number;
  body: string;
  sizeBytes: number;
};

const scenarioId = 'bulk-operation-run-mutation-file-size';
const maxInputFileSizeBytes = 100 * 1024 * 1024;
const oversizedInputFileSizeBytes = maxInputFileSizeBytes + 1;
const runningInputFileSizeBytes = 8 * 1024 * 1024;
const stagedUploadPolicyHeadroomBytes = 5 * 1024 * 1024;

const bulkOperationFields = `id
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
query`;

const stagedUploadOperationName = 'BulkOperationRunMutationFileSizeStagedUpload';
const stagedUploadQuery = `mutation ${stagedUploadOperationName}($input: [StagedUploadInput!]!) {
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

const runMutationOperationName = 'BulkOperationRunMutationFileSize';
const runMutationQuery = `mutation ${runMutationOperationName}($mutation: String!, $path: String!) {
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

const currentMutationOperationName = 'BulkOperationRunMutationFileSizeCurrent';
const currentMutationQuery = `query ${currentMutationOperationName} {
  currentBulkOperation(type: MUTATION) {
    ${bulkOperationFields}
  }
}
`;

const cancelOperationName = 'BulkOperationRunMutationFileSizeCancel';
const cancelMutation = `mutation ${cancelOperationName}($id: ID!) {
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

const innerProductCreateMutation =
  'mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }';

function capture(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
  result: { status: number; payload: unknown },
): GraphqlCapture {
  return {
    operationName,
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

function readStagedTarget(response: unknown): StagedTarget {
  const payload = response as {
    data?: {
      stagedUploadsCreate?: {
        stagedTargets?: StagedTarget[] | null;
        userErrors?: unknown[] | null;
      } | null;
    };
  };
  const result = payload.data?.stagedUploadsCreate;
  if (!result || (result.userErrors?.length ?? 0) > 0) {
    throw new Error(`stagedUploadsCreate returned userErrors: ${JSON.stringify(result?.userErrors ?? response)}`);
  }
  const [target] = result.stagedTargets ?? [];
  if (!target) {
    throw new Error(`stagedUploadsCreate did not return a staged target: ${JSON.stringify(response)}`);
  }
  return target;
}

function stagedUploadPath(target: StagedTarget): string {
  const key = target.parameters.find((parameter) => parameter.name === 'key')?.value;
  if (key) {
    return key;
  }
  const resourceUrl = target.resourceUrl || target.url;
  if (resourceUrl) {
    const parsed = new URL(resourceUrl);
    return parsed.pathname.replace(/^\/+/, '');
  }
  throw new Error(`staged target did not include a path-bearing URL: ${JSON.stringify(target)}`);
}

function readCurrentMutation(response: unknown): BulkOperationNode | null {
  const payload = response as {
    data?: { currentBulkOperation?: BulkOperationNode | null };
  };
  return payload.data?.currentBulkOperation ?? null;
}

function readRunMutationOperation(response: unknown): BulkOperationNode | null {
  const payload = response as {
    data?: { bulkOperationRunMutation?: { bulkOperation?: BulkOperationNode | null } | null };
  };
  return payload.data?.bulkOperationRunMutation?.bulkOperation ?? null;
}

function terminal(operation: BulkOperationNode | null): boolean {
  return ['CANCELED', 'COMPLETED', 'EXPIRED', 'FAILED'].includes(operation?.status ?? '');
}

function assertFileSizeError(response: unknown): void {
  const payload = response as {
    data?: {
      bulkOperationRunMutation?: {
        bulkOperation?: unknown;
        userErrors?: Array<{ field?: string[] | null; message?: string; code?: string | null }> | null;
      } | null;
    };
  };
  const result = payload.data?.bulkOperationRunMutation;
  const [error] = result?.userErrors ?? [];
  if (
    !result ||
    result.bulkOperation !== null ||
    (result.userErrors?.length ?? 0) !== 1 ||
    error?.field !== null ||
    error?.message !== 'The input file size exceeds the maximum allowed size of 100 MB.' ||
    error?.code !== 'INVALID_STAGED_UPLOAD_FILE'
  ) {
    throw new Error(`Unexpected file-size validation payload: ${JSON.stringify(response)}`);
  }
}

async function runGraphql(context: GraphqlContext, query: string, variables: Record<string, unknown>) {
  const result = await runAdminGraphqlRequest(context, query, variables);
  if (result.status !== 200) {
    throw new Error(`GraphQL request returned HTTP ${result.status}: ${JSON.stringify(result.payload)}`);
  }
  return result;
}

async function cancelCurrentMutationIfNeeded(context: GraphqlContext): Promise<GraphqlCapture[]> {
  const captures: GraphqlCapture[] = [];
  const current = await runGraphql(context, currentMutationQuery, {});
  captures.push(capture(currentMutationOperationName, currentMutationQuery, {}, current));
  const operation = readCurrentMutation(current.payload);
  if (!operation?.id || terminal(operation)) {
    return captures;
  }

  const variables = { id: operation.id };
  const cancel = await runGraphql(context, cancelMutation, variables);
  captures.push(capture(cancelOperationName, cancelMutation, variables, cancel));
  return captures;
}

function jsonlBlobAtLeastBytes(row: unknown, minBytes: number): Blob {
  const line = `${JSON.stringify(row)}\n`;
  const chunk = line.repeat(Math.ceil((1024 * 1024) / line.length));
  const chunks = Array.from({ length: Math.ceil(minBytes / chunk.length) }, () => chunk);
  return new Blob(chunks, { type: 'text/jsonl' });
}

async function uploadJsonl(target: StagedTarget, filename: string, body: Blob): Promise<UploadCapture> {
  if (target.parameters.some((parameter) => parameter.name === 'key')) {
    const form = new FormData();
    for (const parameter of target.parameters) {
      form.append(parameter.name, parameter.value);
    }
    form.append('file', body, filename);

    const response = await fetch(target.url, { method: 'POST', body: form });
    return { status: response.status, body: await response.text(), sizeBytes: body.size };
  }

  const response = await fetch(target.url, {
    method: 'PUT',
    headers: { 'content-type': 'text/jsonl' },
    body,
  });
  return { status: response.status, body: await response.text(), sizeBytes: body.size };
}

async function createStagedUpload(
  context: GraphqlContext,
  filename: string,
  fileSizeBytes?: number,
): Promise<{ stagedUpload: GraphqlCapture; target: StagedTarget; path: string }> {
  const input: Record<string, unknown> = {
    filename,
    mimeType: 'text/jsonl',
    resource: 'BULK_MUTATION_VARIABLES',
    httpMethod: 'PUT',
  };
  if (fileSizeBytes !== undefined) {
    input['fileSize'] = String(fileSizeBytes);
  }
  const variables = {
    input: [input],
  };
  const result = await runGraphql(context, stagedUploadQuery, variables);
  const target = readStagedTarget(result.payload);
  return {
    stagedUpload: capture(stagedUploadOperationName, stagedUploadQuery, variables, result),
    target,
    path: stagedUploadPath(target),
  };
}

async function main(): Promise<void> {
  const config = readConformanceScriptConfig({
    defaultApiVersion: '2026-04',
    exitOnMissing: true,
  });
  const accessToken = await getValidConformanceAccessToken({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
  });
  const context = {
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
    headers: buildAdminAuthHeaders(accessToken),
  };

  const cleanupBefore = await cancelCurrentMutationIfNeeded(context);

  const runId = `${scenarioId}-${Date.now()}`;
  const runningFilename = `${runId}-running.jsonl`;
  const runningBody = jsonlBlobAtLeastBytes({ product: { title: '' } }, runningInputFileSizeBytes);
  const runningUpload = await createStagedUpload(
    context,
    runningFilename,
    runningBody.size + stagedUploadPolicyHeadroomBytes,
  );
  const runningUploadResult = await uploadJsonl(runningUpload.target, runningFilename, runningBody);
  if (runningUploadResult.status < 200 || runningUploadResult.status > 299) {
    throw new Error(`running staged upload returned HTTP ${runningUploadResult.status}: ${runningUploadResult.body}`);
  }

  const firstRunVariables = { mutation: innerProductCreateMutation, path: runningUpload.path };
  const firstRun = await runGraphql(context, runMutationQuery, firstRunVariables);
  const firstOperation = readRunMutationOperation(firstRun.payload);
  if (firstOperation?.status !== 'CREATED' || !firstOperation.id) {
    throw new Error(
      `initial bulkOperationRunMutation did not start a mutation operation: ${JSON.stringify(firstRun.payload)}`,
    );
  }

  const oversizedFilename = `${runId}-oversized.jsonl`;
  const oversizedBody = jsonlBlobAtLeastBytes(
    { product: { title: `Oversized file-size guard ${runId}` } },
    oversizedInputFileSizeBytes,
  );
  const oversizedUpload = await createStagedUpload(context, oversizedFilename, oversizedBody.size);
  const oversizedUploadResult = await uploadJsonl(oversizedUpload.target, oversizedFilename, oversizedBody);
  if (oversizedUploadResult.status < 200 || oversizedUploadResult.status > 299) {
    throw new Error(
      `oversized staged upload returned HTTP ${oversizedUploadResult.status}: ${oversizedUploadResult.body}`,
    );
  }

  const oversizedRunVariables = { mutation: innerProductCreateMutation, path: oversizedUpload.path };
  const oversizedRun = await runGraphql(context, runMutationQuery, oversizedRunVariables);
  assertFileSizeError(oversizedRun.payload);

  const currentAfterOversized = await runGraphql(context, currentMutationQuery, {});
  const currentAfterOversizedOperation = readCurrentMutation(currentAfterOversized.payload);
  if (currentAfterOversizedOperation?.id !== firstOperation.id || terminal(currentAfterOversizedOperation)) {
    throw new Error(
      `initial mutation operation was not still non-terminal after oversized run: ${JSON.stringify(
        currentAfterOversized.payload,
      )}`,
    );
  }

  const cancelAfterVariables = { id: firstOperation.id };
  const cancelAfter = await runGraphql(context, cancelMutation, cancelAfterVariables);

  const outputDir = path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, 'bulk-operations');
  mkdirSync(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, `${scenarioId}.json`);
  writeFileSync(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId,
        capturedAt: new Date().toISOString(),
        storeDomain: config.storeDomain,
        apiVersion: config.apiVersion,
        request: { operationName: runMutationOperationName, query: runMutationQuery },
        cleanupBefore,
        running: {
          stagedUpload: runningUpload.stagedUpload,
          upload: runningUploadResult,
          firstRun: capture(runMutationOperationName, runMutationQuery, firstRunVariables, firstRun),
          oversizedStagedUpload: oversizedUpload.stagedUpload,
          oversizedUpload: oversizedUploadResult,
          oversizedRun: capture(runMutationOperationName, runMutationQuery, oversizedRunVariables, oversizedRun),
          currentAfterOversized: capture(currentMutationOperationName, currentMutationQuery, {}, currentAfterOversized),
          cancelAfter: capture(cancelOperationName, cancelMutation, cancelAfterVariables, cancelAfter),
        },
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
