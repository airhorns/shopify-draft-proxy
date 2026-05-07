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

type Context = {
  adminOrigin: string;
  apiVersion: string;
  headers: Record<string, string>;
};

type CaseConfig = {
  key: string;
  filename: string;
  innerMutation: string;
  variables: Record<string, unknown>;
  cleanupIdPath: string[];
  cleanup?: (id: string) => Promise<GraphqlCapture>;
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

const stagedUploadOperationName = 'BulkOperationRunMutationAllowedRootsStagedUpload';
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

const runMutationOperationName = 'BulkOperationRunMutationAllowedRoots';
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

const bulkOperationByIdOperationName = 'BulkOperationRunMutationAllowedRootsById';
const bulkOperationByIdQuery = `query ${bulkOperationByIdOperationName}($id: ID!) {
  bulkOperation(id: $id) {
    ${bulkOperationFields}
  }
}
`;

const currentMutationOperationName = 'BulkOperationRunMutationAllowedRootsCurrent';
const currentMutationQuery = `query ${currentMutationOperationName} {
  currentBulkOperation(type: MUTATION) {
    ${bulkOperationFields}
  }
}
`;

const shopIdOperationName = 'BulkOperationRunMutationAllowedRootsShopId';
const shopIdQuery = `query ${shopIdOperationName} {
  shop {
    id
  }
}
`;

const customerDeleteOperationName = 'BulkOperationRunMutationAllowedRootsCustomerDelete';
const customerDeleteQuery = `mutation ${customerDeleteOperationName}($input: CustomerDeleteInput!) {
  customerDelete(input: $input) {
    deletedCustomerId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const metaobjectDefinitionDeleteOperationName = 'BulkOperationRunMutationAllowedRootsDefinitionDelete';
const metaobjectDefinitionDeleteQuery = `mutation ${metaobjectDefinitionDeleteOperationName}($id: ID!) {
  metaobjectDefinitionDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
      elementKey
      elementIndex
    }
  }
}
`;

const metafieldsDeleteOperationName = 'BulkOperationRunMutationAllowedRootsMetafieldsDelete';
const metafieldsDeleteQuery = `mutation ${metafieldsDeleteOperationName}($metafields: [MetafieldIdentifierInput!]!) {
  metafieldsDelete(metafields: $metafields) {
    deletedMetafields {
      ownerId
      namespace
      key
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const innerCustomerCreateMutation =
  'mutation CustomerCreate($input: CustomerInput!) { customerCreate(input: $input) { customer { id email displayName } userErrors { field message } } }';

const innerMetaobjectDefinitionCreateMutation =
  'mutation MetaobjectDefinitionCreate($definition: MetaobjectDefinitionCreateInput!) { metaobjectDefinitionCreate(definition: $definition) { metaobjectDefinition { id type name displayNameKey fieldDefinitions { key name required type { name } } } userErrors { field message code elementKey elementIndex } } }';

const innerMetafieldsSetMutation =
  'mutation MetafieldsSet($metafields: [MetafieldsSetInput!]!) { metafieldsSet(metafields: $metafields) { metafields { id namespace key type value ownerType } userErrors { field message code } } }';

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

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (typeof current !== 'object' || current === null) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

function readRequiredString(value: unknown, pathSegments: string[], label: string): string {
  const found = readPath(value, pathSegments);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`Missing ${label} at ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
  }
  return found;
}

function readIdFromResultJsonl(jsonl: string | null | undefined, pathSegments: string[]): string | null {
  if (!jsonl) {
    return null;
  }
  for (const line of jsonl.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }
    const id = readPath(JSON.parse(trimmed) as unknown, pathSegments);
    if (typeof id === 'string' && id.length > 0) {
      return id;
    }
  }
  return null;
}

function assertRunCreated(caseKey: string, response: unknown): void {
  const payload = response as {
    data?: {
      bulkOperationRunMutation?: {
        bulkOperation?: BulkOperationNode | null;
        userErrors?: unknown[] | null;
      } | null;
    };
  };
  const result = payload.data?.bulkOperationRunMutation;
  if (result?.bulkOperation?.status !== 'CREATED' || (result.userErrors?.length ?? 0) > 0) {
    throw new Error(`${caseKey} did not return CREATED with no userErrors: ${JSON.stringify(response)}`);
  }
}

function assertNoSuchFile(caseKey: string, response: unknown): void {
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
    result?.bulkOperation !== null ||
    (result?.userErrors?.length ?? 0) !== 1 ||
    error?.field !== null ||
    error.code !== 'NO_SUCH_FILE'
  ) {
    throw new Error(
      `${caseKey} missing-upload branch was not accepted through NO_SUCH_FILE: ${JSON.stringify(response)}`,
    );
  }
}

async function createStagedUpload(
  context: Context,
  filename: string,
): Promise<{ capture: GraphqlCapture; target: StagedTarget; pathKey: string }> {
  const variables = {
    input: [
      {
        filename,
        mimeType: 'text/jsonl',
        resource: 'BULK_MUTATION_VARIABLES',
        httpMethod: 'POST',
      },
    ],
  };
  const result = await runAdminGraphqlRequest(context, stagedUploadQuery, variables);
  if (result.status !== 200) {
    throw new Error(`stagedUploadsCreate returned HTTP ${result.status}: ${JSON.stringify(result.payload)}`);
  }
  const target = readStagedTarget(result.payload);
  return {
    capture: capture(stagedUploadOperationName, stagedUploadQuery, variables, result),
    target,
    pathKey: stagedUploadPath(target),
  };
}

async function runBulkCase(context: Context, config: CaseConfig): Promise<Record<string, unknown>> {
  const stagedUpload = await createStagedUpload(context, config.filename);
  const upload = await uploadJsonl(stagedUpload.target, config.filename, `${JSON.stringify(config.variables)}\n`);
  if (upload.status < 200 || upload.status > 299) {
    throw new Error(`${config.key} staged upload returned HTTP ${upload.status}: ${upload.body}`);
  }

  const runVariables = {
    mutation: config.innerMutation,
    path: stagedUpload.pathKey,
  };
  const run = await runAdminGraphqlRequest(context, runMutationQuery, runVariables);
  if (run.status !== 200) {
    throw new Error(
      `${config.key} bulkOperationRunMutation returned HTTP ${run.status}: ${JSON.stringify(run.payload)}`,
    );
  }
  assertRunCreated(config.key, run.payload);

  const currentAfterRun = await runAdminGraphqlRequest(context, currentMutationQuery, {});
  const runOperation = readRunBulkOperation(run.payload);
  const statusPolls: GraphqlCapture[] = [];
  let terminalOperation: BulkOperationNode | null = null;
  if (runOperation?.id) {
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
  if (terminalOperation?.status !== 'COMPLETED') {
    throw new Error(`${config.key} did not complete successfully: ${JSON.stringify(terminalOperation)}`);
  }

  const result = await fetchResultJsonl(terminalOperation.url);
  const cleanupId = readIdFromResultJsonl(result?.body, config.cleanupIdPath);
  const cleanup: Record<string, unknown> = {};
  if (cleanupId && config.cleanup) {
    cleanup['primary'] = await config.cleanup(cleanupId);
  }

  const missingUpload = await createStagedUpload(context, `missing-${config.filename}`);
  const missingRunVariables = {
    mutation: config.innerMutation,
    path: missingUpload.pathKey,
  };
  const missingRun = await runAdminGraphqlRequest(context, runMutationQuery, missingRunVariables);
  if (missingRun.status !== 200) {
    throw new Error(`${config.key} missing-upload run returned HTTP ${missingRun.status}`);
  }
  assertNoSuchFile(config.key, missingRun.payload);

  return {
    success: {
      stagedUpload: stagedUpload.capture,
      upload,
      run: capture(runMutationOperationName, runMutationQuery, runVariables, run),
      currentAfterRun: capture(currentMutationOperationName, currentMutationQuery, {}, currentAfterRun),
      statusPolls,
      terminalOperation,
      result,
    },
    missingUpload: {
      stagedUpload: missingUpload.capture,
      run: capture(runMutationOperationName, runMutationQuery, missingRunVariables, missingRun),
    },
    cleanup,
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
  const headers = buildAdminAuthHeaders(accessToken);
  const context = { adminOrigin: config.adminOrigin, apiVersion: config.apiVersion, headers };

  const timestamp = Date.now();
  const shopIdResult = await runAdminGraphqlRequest(context, shopIdQuery, {});
  if (shopIdResult.status !== 200) {
    throw new Error(`shop id query returned HTTP ${shopIdResult.status}: ${JSON.stringify(shopIdResult.payload)}`);
  }
  const shopId = readRequiredString(shopIdResult.payload, ['data', 'shop', 'id'], 'shop id');
  const metafieldNamespace = 'bulk_allowed_roots';
  const metafieldKey = `case_${timestamp}`;

  const cleanupCustomer = async (id: string): Promise<GraphqlCapture> => {
    const variables = { input: { id } };
    const result = await runAdminGraphqlRequest(context, customerDeleteQuery, variables);
    return capture(customerDeleteOperationName, customerDeleteQuery, variables, result);
  };
  const cleanupDefinition = async (id: string): Promise<GraphqlCapture> => {
    const variables = { id };
    const result = await runAdminGraphqlRequest(context, metaobjectDefinitionDeleteQuery, variables);
    return capture(metaobjectDefinitionDeleteOperationName, metaobjectDefinitionDeleteQuery, variables, result);
  };
  const cleanupMetafield = async (): Promise<GraphqlCapture> => {
    const variables = {
      metafields: [{ ownerId: shopId, namespace: metafieldNamespace, key: metafieldKey }],
    };
    const result = await runAdminGraphqlRequest(context, metafieldsDeleteQuery, variables);
    return capture(metafieldsDeleteOperationName, metafieldsDeleteQuery, variables, result);
  };

  const cases: Record<string, unknown> = {};
  cases['customerCreate'] = await runBulkCase(context, {
    key: 'customerCreate',
    filename: `bulk-allowed-roots-customer-${timestamp}.jsonl`,
    innerMutation: innerCustomerCreateMutation,
    variables: {
      input: {
        email: `bulk-allowed-roots-${timestamp}@example.com`,
        firstName: 'Bulk',
        lastName: 'Allowed Root',
      },
    },
    cleanupIdPath: ['data', 'customerCreate', 'customer', 'id'],
    cleanup: cleanupCustomer,
  });
  cases['metaobjectDefinitionCreate'] = await runBulkCase(context, {
    key: 'metaobjectDefinitionCreate',
    filename: `bulk-allowed-roots-metaobject-definition-${timestamp}.jsonl`,
    innerMutation: innerMetaobjectDefinitionCreateMutation,
    variables: {
      definition: {
        name: `Bulk Allowed Root ${timestamp}`,
        type: `bulk_allowed_root_${timestamp}`,
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            type: 'single_line_text_field',
            required: true,
          },
        ],
      },
    },
    cleanupIdPath: ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    cleanup: cleanupDefinition,
  });
  cases['metafieldsSet'] = await runBulkCase(context, {
    key: 'metafieldsSet',
    filename: `bulk-allowed-roots-metafields-${timestamp}.jsonl`,
    innerMutation: innerMetafieldsSetMutation,
    variables: {
      metafields: [
        {
          ownerId: shopId,
          namespace: metafieldNamespace,
          key: metafieldKey,
          type: 'single_line_text_field',
          value: `bulk allowed root ${timestamp}`,
        },
      ],
    },
    cleanupIdPath: ['data', 'metafieldsSet', 'metafields', '0', 'id'],
    cleanup: async () => cleanupMetafield(),
  });

  const outputDir = path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, 'bulk-operations');
  mkdirSync(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'bulk-operation-run-mutation-allowed-roots.json');
  writeFileSync(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain: config.storeDomain,
        apiVersion: config.apiVersion,
        request: { operationName: runMutationOperationName, query: runMutationQuery },
        setup: {
          shopId: capture(shopIdOperationName, shopIdQuery, {}, shopIdResult),
        },
        cases,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(JSON.stringify({ ok: true, outputPath, roots: Object.keys(cases) }, null, 2));
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
