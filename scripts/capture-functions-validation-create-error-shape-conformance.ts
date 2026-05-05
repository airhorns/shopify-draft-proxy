/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
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
const outputPath = path.join(outputDir, 'functions-validation-create-error-shape.json');
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

function readString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function readFunctionNodes(captureResult: Capture): FunctionNode[] {
  const payload = readRecord(captureResult.response.payload);
  const data = readRecord(payload['data']);
  const connection = readRecord(data['shopifyFunctions']);
  return readArray(connection['nodes']).map((node) => readRecord(node) as FunctionNode);
}

function requireFunctionNode(nodes: FunctionNode[], handle: string): FunctionNode {
  const node = nodes.find((candidate) => candidate.handle === handle);
  if (!node?.id) {
    throw new Error(`Expected released Function handle ${handle} in live shopifyFunctions response.`);
  }
  return node;
}

function readValidationCreate(captureResult: Capture, alias: string): Record<string, unknown> {
  const payload = readRecord(captureResult.response.payload);
  const data = readRecord(payload['data']);
  return readRecord(data[alias]);
}

function assertUserError(
  captureResult: Capture,
  alias: string,
  expected: { code: string; field: string[]; message: string },
): void {
  const payload = readValidationCreate(captureResult, alias);
  if (payload['validation'] !== null) {
    throw new Error(`${alias} unexpectedly returned validation: ${JSON.stringify(payload, null, 2)}`);
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
      `${alias} userError mismatch: ${JSON.stringify(
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

const functionReadDocument = `query ReadValidationCreateErrorShapeFunctions {
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

const validationCreateErrorShapeDocument = `mutation FunctionsValidationCreateErrorShape(
  $unknownFunctionId: String!
  $cartFunctionId: String!
  $cartFunctionHandle: String!
) {
  unknownFunction: validationCreate(
    validation: { functionId: $unknownFunctionId, title: "Unknown function id shape" }
  ) {
    validation {
      id
    }
    userErrors {
      code
      field
      message
    }
  }
  apiMismatch: validationCreate(
    validation: { functionId: $cartFunctionId, title: "Wrong API shape" }
  ) {
    validation {
      id
    }
    userErrors {
      code
      field
      message
    }
  }
  missingIdentifier: validationCreate(validation: {}) {
    validation {
      id
    }
    userErrors {
      code
      field
      message
    }
  }
  multipleIdentifiers: validationCreate(
    validation: {
      functionId: $cartFunctionId
      functionHandle: $cartFunctionHandle
      title: "Multiple identifiers shape"
    }
  ) {
    validation {
      id
    }
    userErrors {
      code
      field
      message
    }
  }
}
`;

const unknownFunctionId = '01900000-0000-7000-8000-000000000000';
const functionRead = await capture(functionReadDocument, {});
assertNoTopLevelErrors(functionRead, 'Function inventory read');
const functionNodes = readFunctionNodes(functionRead);
const cartFunction = requireFunctionNode(functionNodes, 'conformance-cart-transform');
const cartFunctionId = cartFunction.id;
const cartFunctionHandle = readString(cartFunction.handle);
if (!cartFunctionHandle) {
  throw new Error('Expected conformance-cart-transform to include a handle.');
}

const validationCreateErrorShape = await capture(validationCreateErrorShapeDocument, {
  unknownFunctionId,
  cartFunctionId,
  cartFunctionHandle,
});
assertNoTopLevelErrors(validationCreateErrorShape, 'validationCreate error-shape capture');
assertUserError(validationCreateErrorShape, 'unknownFunction', {
  code: 'NOT_FOUND',
  field: ['validation', 'functionId'],
  message: 'Extension not found.',
});
assertUserError(validationCreateErrorShape, 'apiMismatch', {
  code: 'FUNCTION_DOES_NOT_IMPLEMENT',
  field: ['validation', 'functionId'],
  message:
    'Unexpected Function API. The provided function must implement one of the following extension targets: [%{targets}].',
});
assertUserError(validationCreateErrorShape, 'missingIdentifier', {
  code: 'MISSING_FUNCTION_IDENTIFIER',
  field: ['validation', 'functionHandle'],
  message: 'Either function_id or function_handle must be provided.',
});
assertUserError(validationCreateErrorShape, 'multipleIdentifiers', {
  code: 'MULTIPLE_FUNCTION_IDENTIFIERS',
  field: ['validation'],
  message: 'Only one of function_id or function_handle can be provided, not both.',
});

const functionHydrateSelection =
  ' id title handle apiType description appKey app { __typename id title handle apiKey } ';
const fixture = {
  scenarioId: 'functions-validation-create-error-shape',
  capturedAt: new Date().toISOString(),
  source: 'live-shopify-and-cassette-backed-local-runtime',
  storeDomain,
  apiVersion,
  summary:
    'validationCreate userError shape evidence for unknown Function id, wrong Function API type, missing Function identifier, and multiple Function identifiers.',
  functionRead,
  validationCreateErrorShape,
  upstreamCalls: [
    {
      operationName: 'FunctionHydrateById',
      variables: {
        id: unknownFunctionId,
      },
      query: `query FunctionHydrateById($id: String!) { shopifyFunction(id: $id) {${functionHydrateSelection} } }`,
      response: {
        status: 200,
        body: {
          data: {
            shopifyFunction: null,
          },
        },
      },
    },
    {
      operationName: 'FunctionHydrateById',
      variables: {
        id: cartFunctionId,
      },
      query: `query FunctionHydrateById($id: String!) { shopifyFunction(id: $id) {${functionHydrateSelection} } }`,
      response: {
        status: 200,
        body: {
          data: {
            shopifyFunction: cartFunction,
          },
        },
      },
    },
  ],
  notes: {
    liveUserErrorEvidence:
      'All four validationCreate userError branches are captured live from Shopify on the conformance shop.',
    localRuntimeEvidence:
      'The upstream cassette contains only ShopifyFunction hydration reads required for the proxy to evaluate the same Function references in LiveHybrid mode.',
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
