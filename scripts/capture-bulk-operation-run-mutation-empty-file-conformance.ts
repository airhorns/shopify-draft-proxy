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

const scenarioId = 'bulk-operation-run-mutation-empty-file';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'bulk-operations');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const requestDir = path.join('config', 'parity-requests', 'bulk-operations');
const stagedUploadDocumentPath = path.join(requestDir, 'bulk-operation-run-mutation-empty-file-staged-upload.graphql');
const runMutationDocumentPath = path.join(requestDir, 'bulk-operation-run-mutation-empty-file.graphql');
const specPath = path.join('config', 'parity-specs', 'bulk-operations', `${scenarioId}.json`);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const stagedUploadOperationName = 'BulkOperationRunMutationEmptyFileStagedUpload';
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

const runMutationOperationName = 'BulkOperationRunMutationEmptyFile';
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

const innerProductCreateMutation =
  'mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }';

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
  const parsed = new URL(target.resourceUrl || target.url);
  return parsed.pathname.replace(/^\/+/, '');
}

async function uploadEmptyJsonl(target: StagedTarget): Promise<{ status: number; body: string; sizeBytes: number }> {
  if (target.parameters.some((parameter) => parameter.name === 'key')) {
    const form = new FormData();
    for (const parameter of target.parameters) {
      form.append(parameter.name, parameter.value);
    }
    form.append('file', new Blob([], { type: 'text/jsonl' }), 'empty.jsonl');
    const response = await fetch(target.url, { method: 'POST', body: form });
    return { status: response.status, body: await response.text(), sizeBytes: 0 };
  }

  const response = await fetch(target.url, {
    method: 'PUT',
    headers: { 'content-type': 'text/jsonl' },
    body: new Blob([], { type: 'text/jsonl' }),
  });
  return { status: response.status, body: await response.text(), sizeBytes: 0 };
}

function assertEmptyFileError(response: unknown): void {
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
    error?.message !== 'The input file is empty.' ||
    error?.code !== 'INVALID_STAGED_UPLOAD_FILE'
  ) {
    throw new Error(`Unexpected empty-file validation payload: ${JSON.stringify(response)}`);
  }
}

const stagedUploadVariables = {
  input: [
    {
      filename: `bulk-operation-run-mutation-empty-file-${Date.now()}.jsonl`,
      mimeType: 'text/jsonl',
      resource: 'BULK_MUTATION_VARIABLES',
      httpMethod: 'PUT',
      fileSize: '0',
    },
  ],
};
const stagedUpload = await runGraphqlRequest(stagedUploadQuery, stagedUploadVariables);
if (stagedUpload.status !== 200) {
  throw new Error(`stagedUploadsCreate returned HTTP ${stagedUpload.status}: ${JSON.stringify(stagedUpload.payload)}`);
}
const target = readStagedTarget(stagedUpload.payload);
const upload = await uploadEmptyJsonl(target);
if (upload.status < 200 || upload.status > 299) {
  throw new Error(`empty staged upload returned HTTP ${upload.status}: ${upload.body}`);
}

const runVariables = {
  mutation: innerProductCreateMutation,
  path: stagedUploadPath(target),
};
const run = await runGraphqlRequest(runMutationQuery, runVariables);
if (run.status !== 200) {
  throw new Error(`bulkOperationRunMutation returned HTTP ${run.status}: ${JSON.stringify(run.payload)}`);
}
assertEmptyFileError(run.payload);

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
      request: { operationName: runMutationOperationName, query: runMutationQuery },
      setup: {
        stagedUpload: captureResult(stagedUploadOperationName, stagedUploadQuery, stagedUploadVariables, stagedUpload),
        upload,
        uploadContent: '',
      },
      validation: {
        run: captureResult(runMutationOperationName, runMutationQuery, runVariables, run),
      },
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
await writeFile(stagedUploadDocumentPath, stagedUploadQuery);
await writeFile(runMutationDocumentPath, runMutationQuery);
await writeFile(specPath, `${JSON.stringify(buildSpec(), null, 2)}\n`);

console.log(
  JSON.stringify({ ok: true, outputPath, stagedUploadDocumentPath, runMutationDocumentPath, specPath }, null, 2),
);

function buildSpec(): Record<string, unknown> {
  return {
    scenarioId,
    operationNames: ['stagedUploadsCreate', 'bulkOperationRunMutation'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'validation-guardrails', 'side-effect-boundary'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['tests/graphql_routes/admin_app_shipping.rs'],
    proxyRequest: {
      documentPath: stagedUploadDocumentPath,
      variablesCapturePath: '$.setup.stagedUpload.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'empty-file-staged-upload-user-errors',
          capturePath: '$.setup.stagedUpload.response.data.stagedUploadsCreate.userErrors',
          proxyPath: '$.data.stagedUploadsCreate.userErrors',
        },
        {
          name: 'empty-file-upload-content',
          capturePath: '$.scenarioId',
          proxyUpload: {
            method: 'PUT',
            path: {
              fromPrimaryProxyPath: '$.data.stagedUploadsCreate.stagedTargets[0].resourceUrl',
            },
            body: {
              fromCapturePath: '$.setup.uploadContent',
            },
          },
        },
        {
          name: 'empty-file-run-mutation-bulk-operation-null',
          capturePath: '$.validation.run.response.data.bulkOperationRunMutation.bulkOperation',
          proxyPath: '$.data.bulkOperationRunMutation.bulkOperation',
          proxyRequest: {
            documentPath: runMutationDocumentPath,
            variables: {
              mutation: {
                fromCapturePath: '$.validation.run.variables.mutation',
              },
              path: {
                fromPrimaryProxyPath: '$.data.stagedUploadsCreate.stagedTargets[0].resourceUrl',
              },
            },
            apiVersion,
          },
        },
        {
          name: 'empty-file-run-mutation-user-errors',
          capturePath: '$.validation.run.response.data.bulkOperationRunMutation.userErrors',
          proxyPath: '$.data.bulkOperationRunMutation.userErrors',
          proxyRequest: {
            documentPath: runMutationDocumentPath,
            variables: {
              mutation: {
                fromCapturePath: '$.validation.run.variables.mutation',
              },
              path: {
                fromPrimaryProxyPath: '$.data.stagedUploadsCreate.stagedTargets[0].resourceUrl',
              },
            },
            apiVersion,
          },
        },
      ],
    },
    notes:
      'Captures Shopify Admin GraphQL rejecting a zero-byte BULK_MUTATION_VARIABLES staged upload for bulkOperationRunMutation with INVALID_STAGED_UPLOAD_FILE, bulkOperation: null, and the empty-file message. Replay earns the staged upload through stagedUploadsCreate and the public staged-upload HTTP route before invoking bulkOperationRunMutation.',
  };
}
