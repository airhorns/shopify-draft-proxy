/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const validationFunctionHandle = 'conformance-validation';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-cart-transform-create-registered-wrong-api-precedence.json');
const requestDir = path.join('config', 'parity-requests', 'functions');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type Capture = {
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

type FunctionNode = {
  id: string;
  title: string | null;
  handle: string | null;
  apiType: string | null;
  description: string | null;
  appKey: string | null;
  app: {
    __typename: string | null;
    id: string | null;
    title: string | null;
    handle: string | null;
    apiKey: string | null;
  } | null;
};

async function loadRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

async function capture(query: string, variables: Record<string, unknown> = {}): Promise<Capture> {
  return {
    query,
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function readRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current: unknown = value;
  for (const segment of pathSegments) {
    current = readRecord(current)[segment];
  }
  return current;
}

function readString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function assertNoTopLevelErrors(captureResult: Capture, context: string): void {
  if (
    captureResult.response.status < 200 ||
    captureResult.response.status >= 300 ||
    readRecord(captureResult.response.payload)['errors']
  ) {
    throw new Error(`${context} failed: ${JSON.stringify(captureResult.response, null, 2)}`);
  }
}

function readUserErrors(value: unknown): unknown[] {
  return readArray(readRecord(value)['userErrors']);
}

function assertEmptyUserErrors(value: unknown, context: string): void {
  const userErrors = readUserErrors(value);
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function readFunctionNodes(captureResult: Capture): FunctionNode[] {
  return readArray(readPath(captureResult.response.payload, ['data', 'shopifyFunctions', 'nodes'])).map(
    (node) => readRecord(node) as FunctionNode,
  );
}

function requireValidationFunction(nodes: FunctionNode[]): FunctionNode {
  const node =
    nodes.find((candidate) => candidate.handle === validationFunctionHandle) ??
    nodes.find((candidate) => candidate.apiType === 'cart_checkout_validation');
  if (!node?.id || !node.handle) {
    throw new Error('Expected a released validation Function with id and handle in live shopifyFunctions response.');
  }
  return node;
}

function readValidationCreate(captureResult: Capture): Record<string, unknown> {
  return readRecord(readPath(captureResult.response.payload, ['data', 'validationCreate']));
}

function readCartTransformCreate(captureResult: Capture): Record<string, unknown> {
  return readRecord(readPath(captureResult.response.payload, ['data', 'cartTransformCreate']));
}

function assertCartTransformUserError(
  captureResult: Capture,
  context: string,
  expected: { code: string; field: string[]; message: string },
): void {
  assertNoTopLevelErrors(captureResult, context);
  const payload = readCartTransformCreate(captureResult);
  if (payload['cartTransform'] !== null) {
    throw new Error(`${context} unexpectedly returned cartTransform: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = readUserErrors(payload);
  const actual = readRecord(userErrors[0]);
  if (
    userErrors.length !== 1 ||
    actual['code'] !== expected.code ||
    actual['message'] !== expected.message ||
    JSON.stringify(actual['field']) !== JSON.stringify(expected.field)
  ) {
    throw new Error(`${context} userError mismatch: ${JSON.stringify({ expected, actual: userErrors }, null, 2)}`);
  }
}

const functionReadDocument = `query ReadCartTransformRegisteredWrongApiFunctions {
  shopifyFunctions(first: 50) {
    nodes {
      id
      title
      handle
      apiType
      description
      appKey
      app {
        __typename
        id
        title
        handle
        apiKey
      }
    }
  }
}
`;

const validationDeleteDocument = `mutation DeleteRegisteredWrongApiValidation($id: ID!) {
  validationDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const functionRead = await capture(functionReadDocument);
assertNoTopLevelErrors(functionRead, 'Function inventory read');
const validationFunction = requireValidationFunction(readFunctionNodes(functionRead));
const validationFunctionId = validationFunction.id;
const validationFunctionHandleValue = validationFunction.handle;
const apiMismatchMessage =
  'Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].';

const validationCreateDocument = await loadRequest(
  'functions-cart-transform-create-registered-wrong-api-validation-setup.graphql',
);
const byIdDocument = await loadRequest('functions-cart-transform-create-registered-wrong-api-by-id.graphql');
const byHandleDocument = await loadRequest('functions-cart-transform-create-registered-wrong-api-by-handle.graphql');

let createdValidationId: string | null = null;
let cleanup: Capture | null = null;

try {
  const validationCreateSetup = await capture(validationCreateDocument, {
    validation: {
      functionId: validationFunctionId,
      title: `Registered wrong API precedence ${Date.now()}`,
      enable: false,
      blockOnFailure: false,
    },
  });
  assertNoTopLevelErrors(validationCreateSetup, 'validationCreate setup');
  assertEmptyUserErrors(readValidationCreate(validationCreateSetup), 'validationCreate setup');
  createdValidationId = readString(
    readPath(validationCreateSetup.response.payload, ['data', 'validationCreate', 'validation', 'id']),
  );
  if (!createdValidationId) {
    throw new Error(`validationCreate setup did not return an id: ${JSON.stringify(validationCreateSetup, null, 2)}`);
  }

  const cartTransformCreateRegisteredWrongApiById = await capture(byIdDocument, {
    functionId: validationFunctionId,
    blockOnFailure: false,
  });
  assertCartTransformUserError(
    cartTransformCreateRegisteredWrongApiById,
    'cartTransformCreate registered wrong-API functionId',
    {
      code: 'FUNCTION_ALREADY_REGISTERED',
      field: ['functionId'],
      message: 'Could not enable cart transform because it is already registered',
    },
  );

  const cartTransformCreateRegisteredWrongApiByHandle = await capture(byHandleDocument, {
    functionHandle: validationFunctionHandleValue,
    blockOnFailure: false,
  });
  assertCartTransformUserError(
    cartTransformCreateRegisteredWrongApiByHandle,
    'cartTransformCreate registered wrong-API functionHandle',
    {
      code: 'FUNCTION_DOES_NOT_IMPLEMENT',
      field: ['functionHandle'],
      message: apiMismatchMessage,
    },
  );

  cleanup = await capture(validationDeleteDocument, { id: createdValidationId });
  assertNoTopLevelErrors(cleanup, 'validationDelete cleanup');
  assertEmptyUserErrors(readPath(cleanup.response.payload, ['data', 'validationDelete']), 'validationDelete cleanup');
  createdValidationId = null;

  const fixture = {
    scenarioId: 'functions-cart-transform-create-registered-wrong-api-precedence',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify',
    storeDomain,
    apiVersion,
    summary: 'cartTransformCreate precedence when a validation Function is already registered on the shop.',
    functionRead,
    validationCreateSetup,
    cartTransformCreateRegisteredWrongApiById,
    cartTransformCreateRegisteredWrongApiByHandle,
    cleanup,
    notes: {
      functionIdPrecedence:
        'After validationCreate registers the validation Function on the shop, Shopify returns FUNCTION_ALREADY_REGISTERED for cartTransformCreate(functionId) before the cart-transform API mismatch.',
      functionHandlePrecedence:
        'For the same registered validation Function, Shopify still returns FUNCTION_DOES_NOT_IMPLEMENT for cartTransformCreate(functionHandle) from the handle/API resolution path.',
      cleanup: 'The disposable Validation created for setup was deleted before the fixture was written.',
    },
    upstreamCalls: [
      {
        operationName: 'FunctionHydrateById',
        variables: { id: validationFunctionId },
        query: functionReadDocument,
        response: {
          status: 200,
          body: {
            data: {
              shopifyFunction: validationFunction,
            },
          },
        },
      },
      {
        operationName: 'FunctionHydrateByHandle',
        variables: { handle: validationFunctionHandleValue, apiType: 'VALIDATION' },
        query: functionReadDocument,
        response: {
          status: 200,
          body: {
            data: {
              shopifyFunctions: {
                nodes: [validationFunction],
              },
            },
          },
        },
      },
      {
        operationName: 'FunctionHydrateByHandle',
        variables: { handle: validationFunctionHandleValue, apiType: 'CART_TRANSFORM' },
        query: functionReadDocument,
        response: {
          status: 200,
          body: {
            data: {
              shopifyFunctions: {
                nodes: [validationFunction],
              },
            },
          },
        },
      },
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (createdValidationId) {
    const cleanupAfterFailure = await capture(validationDeleteDocument, { id: createdValidationId });
    console.log(`Cleaned up validation after failure: ${JSON.stringify(cleanupAfterFailure.response.payload)}`);
  }
}
