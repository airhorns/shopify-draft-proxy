import 'dotenv/config';

/* oxlint-disable no-console -- CLI capture script writes status to stdout/stderr. */

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
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

const operationName = 'BulkOperationRunMutationValidators';
const query = readFileSync(
  'config/parity-requests/bulk-operations/bulk-operation-run-mutation-validators.graphql',
  'utf8',
);

const tooManyConnectionsMessage = 'Bulk mutations cannot contain more than 1 connection.';

function payloadFrom(name: string, response: unknown): BulkOperationRunMutationPayload {
  const payload = response as {
    data?: { bulkOperationRunMutation?: BulkOperationRunMutationPayload };
    errors?: unknown[] | null;
  };
  if ((payload.errors?.length ?? 0) > 0) {
    throw new Error(`${name} returned top-level errors: ${JSON.stringify(payload.errors)}`);
  }
  const result = payload.data?.bulkOperationRunMutation;
  if (!result) {
    throw new Error(`${name} missing bulkOperationRunMutation payload: ${JSON.stringify(response)}`);
  }
  return result;
}

function assertValidationError(name: string, response: unknown, expectedMessage: string): void {
  const payload = payloadFrom(name, response);
  const [error] = payload.userErrors;
  if (
    payload.bulkOperation !== null ||
    payload.userErrors.length !== 1 ||
    !Array.isArray(error?.field) ||
    error.field.length !== 1 ||
    error.field[0] !== 'mutation' ||
    error.message !== expectedMessage ||
    error.code !== null
  ) {
    throw new Error(`${name} returned unexpected payload: ${JSON.stringify(payload)}`);
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

  const cases: Record<string, { mutation: string; path: string; expectedMessage: string }> = {
    tooManyConnections: {
      mutation:
        'mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { variants(first: 1) { edges { node { id } } } media(first: 1) { edges { node { id } } } } } }',
      path: 'valid',
      expectedMessage: tooManyConnectionsMessage,
    },
    nestingTooDeep: {
      mutation:
        'mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { variants(first: 1) { edges { node { id media(first: 1) { edges { node { id } } } } } } } } }',
      path: 'valid',
      expectedMessage: tooManyConnectionsMessage,
    },
  };

  const validations: Record<string, ValidationCapture> = {};
  for (const [key, captureCase] of Object.entries(cases)) {
    const variables = {
      mutation: captureCase.mutation,
      path: captureCase.path,
    };
    const result = await runAdminGraphqlRequest(
      {
        adminOrigin: config.adminOrigin,
        apiVersion: config.apiVersion,
        headers,
      },
      query,
      variables,
    );
    if (result.status !== 200) {
      throw new Error(`${key} returned HTTP ${result.status}: ${JSON.stringify(result.payload)}`);
    }
    assertValidationError(key, result.payload, captureCase.expectedMessage);
    validations[key] = {
      operationName,
      query,
      variables,
      status: result.status,
      response: result.payload,
    };
  }

  const outputDir = path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, 'bulk-operations');
  mkdirSync(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'bulk-operation-run-mutation-connection-validators.json');
  writeFileSync(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain: config.storeDomain,
        apiVersion: config.apiVersion,
        request: { operationName, query },
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
