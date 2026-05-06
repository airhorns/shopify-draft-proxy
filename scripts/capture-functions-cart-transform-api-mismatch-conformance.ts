/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-cart-transform-create-api-mismatch-by-identifier.json');
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

async function capture(query: string, variables: Record<string, unknown>): Promise<Capture> {
  return {
    query,
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function assertNoTopLevelErrors(captureResult: Capture, context: string): void {
  if (
    captureResult.response.status < 200 ||
    captureResult.response.status >= 300 ||
    captureResult.response.payload.errors
  ) {
    throw new Error(`${context} failed: ${JSON.stringify(captureResult.response, null, 2)}`);
  }
}

function readRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readFunctionNodes(captureResult: Capture): FunctionNode[] {
  const payload = readRecord(captureResult.response.payload);
  const data = readRecord(payload['data']);
  const connection = readRecord(data['shopifyFunctions']);
  return readArray(connection['nodes']).map((node) => readRecord(node) as FunctionNode);
}

function readCartTransformNodes(captureResult: Capture): Record<string, unknown>[] {
  const payload = readRecord(captureResult.response.payload);
  const data = readRecord(payload['data']);
  const connection = readRecord(data['cartTransforms']);
  return readArray(connection['nodes']).map(readRecord);
}

function requireValidationFunction(nodes: FunctionNode[]): FunctionNode {
  const node =
    nodes.find((candidate) => candidate.handle === 'conformance-validation') ??
    nodes.find((candidate) => candidate.apiType === 'cart_checkout_validation');
  if (!node?.id || !node.handle) {
    throw new Error('Expected a released validation Function with id and handle in live shopifyFunctions response.');
  }
  return node;
}

function readCartTransformCreate(captureResult: Capture): Record<string, unknown> {
  const payload = readRecord(captureResult.response.payload);
  const data = readRecord(payload['data']);
  return readRecord(data['cartTransformCreate']);
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
  const userErrors = readArray(payload['userErrors']);
  const actual = readRecord(userErrors[0]);
  if (
    userErrors.length !== 1 ||
    actual['code'] !== expected.code ||
    actual['message'] !== expected.message ||
    JSON.stringify(actual['field']) !== JSON.stringify(expected.field)
  ) {
    throw new Error(
      `${context} userError mismatch: ${JSON.stringify(
        {
          expected,
          actual: userErrors,
        },
        null,
        2,
      )}`,
    );
  }
}

const functionReadDocument = `query ReadCartTransformApiMismatchFunctions {
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

const cartTransformDeleteDocument = `mutation DeleteExistingCartTransform($id: ID!) {
  cartTransformDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const functionRead = await capture(functionReadDocument, {});
assertNoTopLevelErrors(functionRead, 'Function inventory read');
const validationFunction = requireValidationFunction(readFunctionNodes(functionRead));
const validationFunctionId = validationFunction.id;
const validationFunctionHandle = validationFunction.handle;
const apiMismatchMessage =
  'Unexpected Function API. The provided function must implement one of the following extension targets: [purchase.cart-transform.run, cart.transform.run].';

const cartTransformsReadDocument = await loadRequest('functions-cart-transform-create-validation-read.graphql');
const existingCartTransforms = await capture(cartTransformsReadDocument, { first: 50 });
assertNoTopLevelErrors(existingCartTransforms, 'Existing cartTransforms read');
const cleanupBefore: Capture[] = [];
for (const node of readCartTransformNodes(existingCartTransforms)) {
  const id = node['id'];
  if (typeof id === 'string') {
    cleanupBefore.push(await capture(cartTransformDeleteDocument, { id }));
  }
}

const byIdDocument = await loadRequest('functions-cart-transform-create-api-mismatch-by-id.graphql');
const byHandleDocument = await loadRequest('functions-cart-transform-create-api-mismatch-by-handle.graphql');
const cartTransformCreateApiMismatchById = await capture(byIdDocument, {
  functionId: validationFunctionId,
  blockOnFailure: false,
});
assertCartTransformUserError(cartTransformCreateApiMismatchById, 'cartTransformCreate functionId API mismatch', {
  code: 'FUNCTION_NOT_FOUND',
  field: ['functionId'],
  message: apiMismatchMessage,
});

const cartTransformCreateApiMismatchByHandle = await capture(byHandleDocument, {
  functionHandle: validationFunctionHandle,
  blockOnFailure: false,
});
assertCartTransformUserError(
  cartTransformCreateApiMismatchByHandle,
  'cartTransformCreate functionHandle API mismatch',
  {
    code: 'FUNCTION_DOES_NOT_IMPLEMENT',
    field: ['functionHandle'],
    message: apiMismatchMessage,
  },
);

const cartTransformsAfterApiMismatch = await capture(cartTransformsReadDocument, { first: 5 });
assertNoTopLevelErrors(cartTransformsAfterApiMismatch, 'cartTransforms after API mismatch read');
const postMismatchNodes = readCartTransformNodes(cartTransformsAfterApiMismatch);
if (postMismatchNodes.length !== 0) {
  throw new Error(
    `Expected no cartTransforms after API mismatch probes: ${JSON.stringify(postMismatchNodes, null, 2)}`,
  );
}

const fixture = {
  scenarioId: 'functions-cart-transform-create-api-mismatch-by-identifier',
  capturedAt: new Date().toISOString(),
  source: 'live-shopify',
  storeDomain,
  apiVersion,
  summary:
    'cartTransformCreate API-mismatched validation Function evidence by functionId and functionHandle plus downstream empty cartTransforms read.',
  functionRead,
  cleanupBefore,
  cartTransformCreateApiMismatchById,
  cartTransformCreateApiMismatchByHandle,
  cartTransformsAfterApiMismatch,
  notes: {
    userErrorEvidence:
      'Shopify returns FUNCTION_NOT_FOUND for the functionId API-mismatch branch and FUNCTION_DOES_NOT_IMPLEMENT for the functionHandle branch with the same message text.',
    sideEffectBoundary:
      'The downstream cartTransforms read is captured after both failed mutations and confirms no CartTransform was created.',
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
