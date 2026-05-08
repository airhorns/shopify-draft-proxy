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

const maxInputFileSizeBytes = 100 * 1024 * 1024;
const oversizedInputFileSizeBytes = maxInputFileSizeBytes + 1024;

const stagedUploadOperationName = 'BulkOperationRunMutationOversizedUploadStagedUpload';
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

const runMutationOperationName = 'BulkOperationRunMutationOversizedUpload';
const runMutationQuery = `mutation ${runMutationOperationName}($mutation: String!, $path: String!) {
  bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
    bulkOperation {
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
    }
    userErrors {
      field
      message
      code
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

  const url = target.resourceUrl || target.url;
  if (!url) {
    throw new Error(`staged target did not include key parameter or resource URL: ${JSON.stringify(target)}`);
  }
  const parsed = new URL(url);
  return parsed.pathname.replace(/^\//u, '');
}

function buildOversizedJsonl(): string {
  const prefix = '{"product":{"title":"';
  const suffix = '"}}\n';
  const fillBytes = oversizedInputFileSizeBytes - Buffer.byteLength(prefix, 'utf8') - Buffer.byteLength(suffix, 'utf8');
  if (fillBytes <= 0) {
    throw new Error('oversized JSONL sizing bug');
  }
  return prefix + 'x'.repeat(fillBytes) + suffix;
}

async function uploadJsonl(
  target: StagedTarget,
  filename: string,
  jsonl: string,
): Promise<{ status: number; body: string; byteSize: number }> {
  const key = target.parameters.find((parameter) => parameter.name === 'key')?.value;
  if (key) {
    const form = new FormData();
    for (const parameter of target.parameters) {
      form.append(parameter.name, parameter.value);
    }
    form.append('file', new Blob([jsonl], { type: 'text/jsonl' }), filename);

    const response = await fetch(target.url, { method: 'POST', body: form });
    return {
      status: response.status,
      body: await response.text(),
      byteSize: Buffer.byteLength(jsonl, 'utf8'),
    };
  }

  const response = await fetch(target.url, {
    method: 'PUT',
    headers: { 'content-type': 'text/jsonl' },
    body: jsonl,
  });
  return {
    status: response.status,
    body: await response.text(),
    byteSize: Buffer.byteLength(jsonl, 'utf8'),
  };
}

function readRunPayload(response: unknown): {
  bulkOperation?: unknown;
  userErrors?: Array<{ field?: unknown; message?: unknown; code?: unknown }> | null;
} | null {
  const payload = response as {
    data?: {
      bulkOperationRunMutation?: {
        bulkOperation?: unknown;
        userErrors?: Array<{ field?: unknown; message?: unknown; code?: unknown }> | null;
      } | null;
    };
  };
  return payload.data?.bulkOperationRunMutation ?? null;
}

function assertOversizedResponse(response: unknown): void {
  const result = readRunPayload(response);
  if (!result) {
    throw new Error(`bulkOperationRunMutation response missing payload: ${JSON.stringify(response)}`);
  }
  if (result.bulkOperation !== null) {
    throw new Error(`expected null bulkOperation for oversized upload: ${JSON.stringify(response)}`);
  }
  const [error, extra] = result.userErrors ?? [];
  if (extra !== undefined || !error) {
    throw new Error(`expected exactly one userError for oversized upload: ${JSON.stringify(response)}`);
  }
  if (
    error.field !== null ||
    error.message !== 'The input file size exceeds the maximum allowed size of 100 MB.' ||
    error.code !== 'INVALID_STAGED_UPLOAD_FILE'
  ) {
    throw new Error(`unexpected oversized upload userError: ${JSON.stringify(error)}`);
  }
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
  const headers = buildAdminAuthHeaders(accessToken);
  const context = { adminOrigin: config.adminOrigin, apiVersion: config.apiVersion, headers };

  const filename = `oversized-bulk-mutation-upload-${Date.now()}.jsonl`;
  const stagedUploadVariables = {
    input: [
      {
        filename,
        mimeType: 'text/jsonl',
        resource: 'BULK_MUTATION_VARIABLES',
        httpMethod: 'PUT',
        fileSize: oversizedInputFileSizeBytes.toString(),
      },
    ],
  };
  const stagedUploadResult = await runAdminGraphqlRequest(context, stagedUploadQuery, stagedUploadVariables);
  if (stagedUploadResult.status !== 200) {
    throw new Error(`stagedUploadsCreate returned HTTP ${stagedUploadResult.status}`);
  }
  const target = readStagedTarget(stagedUploadResult.payload);
  const pathKey = stagedUploadPath(target);

  const jsonl = buildOversizedJsonl();
  const upload = await uploadJsonl(target, filename, jsonl);
  if (upload.byteSize !== oversizedInputFileSizeBytes) {
    throw new Error(`oversized JSONL was ${upload.byteSize} bytes, expected ${oversizedInputFileSizeBytes}`);
  }
  if (upload.status < 200 || upload.status > 299) {
    throw new Error(`staged upload returned HTTP ${upload.status}: ${upload.body}`);
  }

  const runVariables = { mutation: innerProductCreateMutation, path: pathKey };
  const run = await runAdminGraphqlRequest(context, runMutationQuery, runVariables);
  if (run.status !== 200) {
    throw new Error(`bulkOperationRunMutation returned HTTP ${run.status}: ${JSON.stringify(run.payload)}`);
  }
  assertOversizedResponse(run.payload);

  const outputDir = path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, 'bulk-operations');
  mkdirSync(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'bulk-operation-run-mutation-oversized-upload.json');
  writeFileSync(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain: config.storeDomain,
        apiVersion: config.apiVersion,
        maxInputFileSizeBytes,
        oversizedInputFileSizeBytes,
        request: { operationName: runMutationOperationName, query: runMutationQuery },
        setup: {
          stagedUpload: capture(
            stagedUploadOperationName,
            stagedUploadQuery,
            stagedUploadVariables,
            stagedUploadResult,
          ),
          upload,
        },
        run: capture(runMutationOperationName, runMutationQuery, runVariables, run),
        cleanup: {},
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(JSON.stringify({ ok: true, outputPath, byteSize: upload.byteSize }, null, 2));
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
