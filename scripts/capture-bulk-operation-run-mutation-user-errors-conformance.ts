import 'dotenv/config';

/* oxlint-disable no-console -- CLI capture script writes status to stdout/stderr. */

import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { runAdminGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type BulkMutationUserError = {
  field: string[] | null;
  message: string;
  code: string | null;
};

type BulkOperationRunMutationPayload = {
  bulkOperation: unknown;
  userErrors: BulkMutationUserError[];
};

type ValidationCapture = {
  operationName: string;
  query: string;
  variables: Record<string, string>;
  status: number;
  response: unknown;
};

const operationName = 'BulkOperationRunMutationUserErrors';
const stagedUploadOperationName = 'BulkOperationRunMutationUserErrorsStagedUpload';
const query = `mutation ${operationName}($mutation: String!, $path: String!) {
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
const stagedUploadQuery = `mutation ${stagedUploadOperationName}($input: [StagedUploadInput!]!) {
  stagedUploadsCreate(input: $input) {
    stagedTargets {
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

const noSuchFileMessage =
  "The JSONL file could not be found. Try uploading the file again, and check that you've entered the URL correctly for the stagedUploadPath mutation argument.";

function payloadFrom(response: unknown): BulkOperationRunMutationPayload {
  const payload = response as {
    data?: { bulkOperationRunMutation?: BulkOperationRunMutationPayload };
  };
  const result = payload.data?.bulkOperationRunMutation;
  if (!result) {
    throw new Error(`Missing bulkOperationRunMutation payload: ${JSON.stringify(response)}`);
  }
  return result;
}

function assertUserErrors(
  name: string,
  response: unknown,
  predicate: (payload: BulkOperationRunMutationPayload) => boolean,
): void {
  const payload = payloadFrom(response);
  if (!predicate(payload)) {
    throw new Error(`${name} returned unexpected payload: ${JSON.stringify(payload)}`);
  }
}

function validationResponse(validations: Record<string, ValidationCapture>, key: string): unknown {
  const validation = validations[key];
  if (!validation) {
    throw new Error(`Missing validation capture: ${key}`);
  }
  return validation.response;
}

function stagedUploadKeyFrom(response: unknown): string {
  const payload = response as {
    data?: {
      stagedUploadsCreate?: {
        stagedTargets?: Array<{
          parameters?: Array<{ name?: string | null; value?: string | null } | null> | null;
        } | null> | null;
        userErrors?: unknown[] | null;
      };
    };
  };
  const result = payload.data?.stagedUploadsCreate;
  if (!result || (result.userErrors?.length ?? 0) > 0) {
    throw new Error(`stagedUploadsCreate failed: ${JSON.stringify(response)}`);
  }
  const key = result.stagedTargets?.[0]?.parameters?.find((parameter) => parameter?.name === 'key')?.value;
  if (!key) {
    throw new Error(`stagedUploadsCreate response did not include a key parameter: ${JSON.stringify(response)}`);
  }
  return key;
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

  const stagedUploadVariables = {
    input: [
      {
        filename: 'har-725-missing.jsonl',
        mimeType: 'text/jsonl',
        resource: 'BULK_MUTATION_VARIABLES',
        httpMethod: 'POST',
      },
    ],
  };
  const stagedUploadResult = await runAdminGraphqlRequest(
    {
      adminOrigin: config.adminOrigin,
      apiVersion: config.apiVersion,
      headers,
    },
    stagedUploadQuery,
    stagedUploadVariables,
  );
  if (stagedUploadResult.status !== 200) {
    throw new Error(
      `stagedUploadsCreate returned HTTP ${stagedUploadResult.status}: ${JSON.stringify(stagedUploadResult.payload)}`,
    );
  }
  const missingUploadPath = stagedUploadKeyFrom(stagedUploadResult.payload);

  const validations: Record<string, ValidationCapture> = {};
  const cases: Array<{ key: string; variables: Record<string, string> }> = [
    {
      key: 'noSuchFile',
      variables: {
        mutation:
          'mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
        path: missingUploadPath,
      },
    },
    {
      key: 'invalidMutationSyntax',
      variables: {
        mutation: 'mutation { not real syntax',
        path: 'valid',
      },
    },
    {
      key: 'disallowedRoot',
      variables: {
        mutation:
          'mutation Probe($mutation: String!, $stagedUploadPath: String!, $clientIdentifier: String) { bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $stagedUploadPath, clientIdentifier: $clientIdentifier) { bulkOperation { id } userErrors { field message } } }',
        path: 'valid',
      },
    },
  ];

  for (const captureCase of cases) {
    const result = await runAdminGraphqlRequest(
      {
        adminOrigin: config.adminOrigin,
        apiVersion: config.apiVersion,
        headers,
      },
      query,
      captureCase.variables,
    );

    if (result.status !== 200) {
      throw new Error(`${captureCase.key} returned HTTP ${result.status}: ${JSON.stringify(result.payload)}`);
    }

    validations[captureCase.key] = {
      operationName,
      query,
      variables: captureCase.variables,
      status: result.status,
      response: result.payload,
    };
  }

  assertUserErrors('noSuchFile', validationResponse(validations, 'noSuchFile'), (payload) => {
    const [error] = payload.userErrors;
    return (
      payload.bulkOperation === null &&
      payload.userErrors.length === 1 &&
      error?.field === null &&
      error.message === noSuchFileMessage &&
      error.code === 'NO_SUCH_FILE'
    );
  });
  assertUserErrors('invalidMutationSyntax', validationResponse(validations, 'invalidMutationSyntax'), (payload) => {
    const [error] = payload.userErrors;
    return (
      payload.bulkOperation === null &&
      payload.userErrors.length === 1 &&
      error?.field === null &&
      error.message.startsWith('Failed to parse the mutation - ') &&
      error.code === 'INVALID_MUTATION'
    );
  });
  assertUserErrors('disallowedRoot', validationResponse(validations, 'disallowedRoot'), (payload) => {
    const [error] = payload.userErrors;
    return (
      payload.bulkOperation === null &&
      payload.userErrors.length === 1 &&
      Array.isArray(error?.field) &&
      error.field.length === 1 &&
      error.field[0] === 'mutation' &&
      error.message === 'You must use an allowed mutation name.' &&
      error.code === null
    );
  });

  const outputDir = path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, 'bulk-operations');
  mkdirSync(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'bulk-operation-run-mutation-user-errors.json');
  writeFileSync(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain: config.storeDomain,
        apiVersion: config.apiVersion,
        request: { operationName, query },
        setup: {
          stagedUploadTarget: {
            operationName: stagedUploadOperationName,
            query: stagedUploadQuery,
            variables: stagedUploadVariables,
            status: stagedUploadResult.status,
            response: stagedUploadResult.payload,
          },
        },
        validations,
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
