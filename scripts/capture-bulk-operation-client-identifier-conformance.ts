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

const scenarioId = 'bulk-operation-run-mutation-client-identifier-validation';
const stagedUploadOperationName = 'BulkOperationClientIdentifierStagedUpload';
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

const runMutationOperationName = 'BulkOperationRunMutationClientIdentifierValidation';
const runMutationQuery = `mutation ${runMutationOperationName}($mutation: String!, $path: String!, $clientIdentifier: String) {
  bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path, clientIdentifier: $clientIdentifier) {
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

const currentMutationOperationName = 'BulkOperationClientIdentifierCurrentMutation';
const currentMutationQuery = `query ${currentMutationOperationName} {
  currentBulkOperation(type: MUTATION) {
    id
    status
    type
  }
}
`;

const cancelOperationName = 'BulkOperationClientIdentifierCancel';
const cancelMutation = `mutation ${cancelOperationName}($id: ID!) {
  bulkOperationCancel(id: $id) {
    bulkOperation {
      id
      status
      type
    }
    userErrors {
      field
      message
    }
  }
}
`;

const runQueryProbeOperationName = 'BulkOperationRunQueryClientIdentifierProbe';
const runQueryProbe = `mutation ${runQueryProbeOperationName}($query: String!, $clientIdentifier: String) {
  bulkOperationRunQuery(query: $query, clientIdentifier: $clientIdentifier) {
    bulkOperation {
      id
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

async function runGraphql(context: GraphqlContext, query: string, variables: Record<string, unknown>) {
  const result = await runAdminGraphqlRequest(context, query, variables);
  if (result.status !== 200) {
    throw new Error(`GraphQL request returned HTTP ${result.status}: ${JSON.stringify(result.payload)}`);
  }
  return result;
}

function readCurrentMutationId(response: unknown): string | null {
  const payload = response as {
    data?: { currentBulkOperation?: { id?: string | null; status?: string | null } | null };
  };
  const operation = payload.data?.currentBulkOperation;
  if (!operation || ['CANCELED', 'COMPLETED', 'EXPIRED', 'FAILED'].includes(operation.status ?? '')) {
    return null;
  }
  return operation.id ?? null;
}

async function cancelCurrentMutationIfNeeded(context: GraphqlContext): Promise<GraphqlCapture[]> {
  const captures: GraphqlCapture[] = [];
  const current = await runGraphql(context, currentMutationQuery, {});
  captures.push(capture(currentMutationOperationName, currentMutationQuery, {}, current));
  const id = readCurrentMutationId(current.payload);
  if (!id) {
    return captures;
  }

  const variables = { id };
  const cancel = await runGraphql(context, cancelMutation, variables);
  captures.push(capture(cancelOperationName, cancelMutation, variables, cancel));
  return captures;
}

function assertClientIdentifierError(response: unknown, message: string): void {
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
    JSON.stringify(error?.field) !== JSON.stringify(['clientIdentifier']) ||
    error?.message !== message ||
    error?.code !== 'INVALID_MUTATION'
  ) {
    throw new Error(`Unexpected clientIdentifier validation payload: ${JSON.stringify(response)}`);
  }
}

async function captureValidationCase(
  context: GraphqlContext,
  key: string,
  clientIdentifier: string,
): Promise<Record<string, unknown>> {
  const filename = `bulk-client-identifier-${key}-${Date.now()}.jsonl`;
  const uploadContent = `${JSON.stringify({ product: { title: `Client identifier ${key}` } })}\n`;
  const stagedUploadVariables = {
    input: [
      {
        filename,
        mimeType: 'text/jsonl',
        resource: 'BULK_MUTATION_VARIABLES',
        httpMethod: 'POST',
      },
    ],
  };
  const stagedUploadResult = await runGraphql(context, stagedUploadQuery, stagedUploadVariables);
  const target = readStagedTarget(stagedUploadResult.payload);
  const upload = await uploadJsonl(target, filename, uploadContent);
  if (upload.status < 200 || upload.status > 299) {
    throw new Error(`${key} staged upload returned HTTP ${upload.status}: ${upload.body}`);
  }

  const variables = {
    mutation: innerProductCreateMutation,
    path: stagedUploadPath(target),
    clientIdentifier,
  };
  const run = await runGraphql(context, runMutationQuery, variables);
  return {
    stagedUpload: capture(stagedUploadOperationName, stagedUploadQuery, stagedUploadVariables, stagedUploadResult),
    upload,
    uploadContent,
    run: capture(runMutationOperationName, runMutationQuery, variables, run),
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
  const tooShort = await captureValidationCase(context, 'too-short', 'abc');
  assertClientIdentifierError((tooShort['run'] as GraphqlCapture).response, 'is too short (minimum is 10 characters)');
  const tooLong = await captureValidationCase(context, 'too-long', 'x'.repeat(256));
  assertClientIdentifierError((tooLong['run'] as GraphqlCapture).response, 'is too long (maximum is 255 characters)');

  const runQueryProbeVariables = { query: exportQuery, clientIdentifier: 'client-one' };
  const runQueryWithClientIdentifier = await runAdminGraphqlRequest(context, runQueryProbe, runQueryProbeVariables);

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
        setup: {
          tooShort,
          tooLong,
        },
        validations: {
          tooShort: tooShort['run'],
          tooLong: tooLong['run'],
        },
        observations: {
          runQueryWithClientIdentifier: capture(
            runQueryProbeOperationName,
            runQueryProbe,
            runQueryProbeVariables,
            runQueryWithClientIdentifier,
          ),
          posAllowlist: {
            status: 'blocked',
            reason:
              'The active conformance credential is not a POS-class or product-feed API client, so POS allowlist throttle scoping cannot be captured with this auth grant.',
          },
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
