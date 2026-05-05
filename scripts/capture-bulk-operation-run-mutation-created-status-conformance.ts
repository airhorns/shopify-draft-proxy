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

type BulkOperationNode = {
  id?: string | null;
  status?: string | null;
  url?: string | null;
};

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

const stagedUploadOperationName = 'BulkOperationRunMutationCreatedStatusStagedUpload';
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

const runMutationOperationName = 'BulkOperationRunMutationCreatedStatus';
const runMutationQuery = `mutation ${runMutationOperationName}($mutation: String!, $path: String!) {
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

const bulkOperationByIdOperationName = 'BulkOperationRunMutationCreatedStatusById';
const bulkOperationByIdQuery = `query ${bulkOperationByIdOperationName}($id: ID!) {
  bulkOperation(id: $id) {
    ${bulkOperationFields}
  }
}
`;

const currentMutationOperationName = 'BulkOperationRunMutationCreatedStatusCurrent';
const currentMutationQuery = `query ${currentMutationOperationName} {
  currentBulkOperation(type: MUTATION) {
    ${bulkOperationFields}
  }
}
`;

const productDeleteOperationName = 'BulkOperationRunMutationCreatedStatusCleanupProductDelete';
const productDeleteQuery = `mutation ${productDeleteOperationName}($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
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

function readRunBulkOperation(response: unknown): BulkOperationNode | null {
  const payload = response as {
    data?: { bulkOperationRunMutation?: { bulkOperation?: BulkOperationNode | null } | null };
  };
  return payload.data?.bulkOperationRunMutation?.bulkOperation ?? null;
}

function readBulkOperation(response: unknown): BulkOperationNode | null {
  const payload = response as { data?: { bulkOperation?: BulkOperationNode | null } };
  return payload.data?.bulkOperation ?? null;
}

function terminal(operation: BulkOperationNode | null): boolean {
  return ['COMPLETED', 'FAILED', 'CANCELED', 'EXPIRED'].includes(operation?.status ?? '');
}

async function fetchResultJsonl(url: string | null | undefined): Promise<{ status: number; body: string } | null> {
  if (!url) {
    return null;
  }
  const response = await fetch(url);
  return { status: response.status, body: await response.text() };
}

function productIdFromResultJsonl(jsonl: string | null | undefined): string | null {
  if (!jsonl) {
    return null;
  }
  for (const line of jsonl.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }
    const row = JSON.parse(trimmed) as {
      data?: { productCreate?: { product?: { id?: string | null } | null } | null };
    };
    const id = row.data?.productCreate?.product?.id;
    if (id) {
      return id;
    }
  }
  return null;
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

  const filename = `har-750-product-create-${Date.now()}.jsonl`;
  const productTitle = `HAR-750 Bulk Created ${new Date().toISOString()}`;
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
  const stagedUploadResult = await runAdminGraphqlRequest(context, stagedUploadQuery, stagedUploadVariables);
  if (stagedUploadResult.status !== 200) {
    throw new Error(`stagedUploadsCreate returned HTTP ${stagedUploadResult.status}`);
  }
  const target = readStagedTarget(stagedUploadResult.payload);
  const pathKey = stagedUploadPath(target);
  const upload = await uploadJsonl(
    target,
    filename,
    `${JSON.stringify({ product: { title: productTitle, status: 'DRAFT' } })}\n`,
  );
  if (upload.status < 200 || upload.status > 299) {
    throw new Error(`staged upload returned HTTP ${upload.status}: ${upload.body}`);
  }

  const runVariables = { mutation: innerProductCreateMutation, path: pathKey };
  const run = await runAdminGraphqlRequest(context, runMutationQuery, runVariables);
  if (run.status !== 200) {
    throw new Error(`bulkOperationRunMutation returned HTTP ${run.status}: ${JSON.stringify(run.payload)}`);
  }
  const runOperation = readRunBulkOperation(run.payload);
  if (runOperation?.status !== 'CREATED') {
    throw new Error(`bulkOperationRunMutation did not return CREATED: ${JSON.stringify(run.payload)}`);
  }

  const currentAfterRun = await runAdminGraphqlRequest(context, currentMutationQuery, {});
  const statusPolls: GraphqlCapture[] = [];
  let terminalOperation: BulkOperationNode | null = null;
  if (runOperation.id) {
    for (let index = 0; index < 30; index += 1) {
      await new Promise((resolve) => setTimeout(resolve, 2000));
      const pollVariables = { id: runOperation.id };
      const poll = await runAdminGraphqlRequest(context, bulkOperationByIdQuery, pollVariables);
      statusPolls.push(capture(bulkOperationByIdOperationName, bulkOperationByIdQuery, pollVariables, poll));
      const operation = readBulkOperation(poll.payload);
      if (terminal(operation)) {
        terminalOperation = operation;
        break;
      }
    }
  }

  const result = await fetchResultJsonl(terminalOperation?.url);
  const productId = productIdFromResultJsonl(result?.body);
  const cleanup: Record<string, unknown> = {};
  if (productId) {
    const cleanupVariables = { input: { id: productId } };
    const cleanupResult = await runAdminGraphqlRequest(context, productDeleteQuery, cleanupVariables);
    cleanup['productDelete'] = capture(productDeleteOperationName, productDeleteQuery, cleanupVariables, cleanupResult);
  }

  const missingUploadVariables = {
    input: [
      {
        filename: `har-750-missing-${Date.now()}.jsonl`,
        mimeType: 'text/jsonl',
        resource: 'BULK_MUTATION_VARIABLES',
        httpMethod: 'POST',
      },
    ],
  };
  const missingUploadTargetResult = await runAdminGraphqlRequest(context, stagedUploadQuery, missingUploadVariables);
  const missingTarget = readStagedTarget(missingUploadTargetResult.payload);
  const missingVariables = {
    mutation: innerProductCreateMutation,
    path: stagedUploadPath(missingTarget),
  };
  const missingRun = await runAdminGraphqlRequest(context, runMutationQuery, missingVariables);

  const outputDir = path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, 'bulk-operations');
  mkdirSync(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'bulk-operation-run-mutation-created-status.json');
  writeFileSync(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain: config.storeDomain,
        apiVersion: config.apiVersion,
        request: { operationName: runMutationOperationName, query: runMutationQuery },
        setup: {
          stagedUpload: capture(
            stagedUploadOperationName,
            stagedUploadQuery,
            stagedUploadVariables,
            stagedUploadResult,
          ),
          upload,
          productTitle,
        },
        success: {
          run: capture(runMutationOperationName, runMutationQuery, runVariables, run),
          currentAfterRun: capture(currentMutationOperationName, currentMutationQuery, {}, currentAfterRun),
          statusPolls,
          terminalOperation,
          result,
        },
        missingUpload: {
          stagedUpload: capture(
            stagedUploadOperationName,
            stagedUploadQuery,
            missingUploadVariables,
            missingUploadTargetResult,
          ),
          run: capture(runMutationOperationName, runMutationQuery, missingVariables, missingRun),
        },
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(
    JSON.stringify({ ok: true, outputPath, productId, terminalStatus: terminalOperation?.status ?? null }, null, 2),
  );
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
