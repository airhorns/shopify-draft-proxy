/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
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
const outputPath = path.join(outputDir, 'functions-cart-transform-create-metafields.json');
const requestDir = path.join('config', 'parity-requests', 'functions');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type JsonRecord = Record<string, unknown>;

type Capture = {
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<JsonRecord>;
};

type FunctionNode = {
  id: string;
  title: string | null;
  handle: string | null;
  apiType: string | null;
  description: string | null;
  appKey: string | null;
  app: JsonRecord | null;
};

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

async function loadRequest(name: string): Promise<string> {
  return readText(path.join(requestDir, name));
}

async function capture(query: string, variables: JsonRecord = {}): Promise<Capture> {
  return {
    query,
    variables,
    response: await runGraphqlRequest<JsonRecord>(query, variables),
  };
}

function readRecord(value: unknown): JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : {};
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readPath(value: unknown, segments: string[]): unknown {
  let current = value;
  for (const segment of segments) {
    const record = readRecord(current);
    if (!(segment in record)) return null;
    current = record[segment];
  }
  return current;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  assertNoTopLevelErrors(result, context);
  const payload = readRecord(readPath(result.payload, ['data', root]));
  const errors = readArray(payload['userErrors']);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertInvalidMetafields(captureResult: Capture, context: string): void {
  assertNoTopLevelErrors(captureResult.response, context);
  const payload = readRecord(readPath(captureResult.response.payload, ['data', 'cartTransformCreate']));
  if (payload['cartTransform'] !== null) {
    throw new Error(`${context} unexpectedly returned cartTransform: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = readArray(payload['userErrors']);
  const invalidMetafieldErrors = userErrors.map(readRecord).filter((error) => error['code'] === 'INVALID_METAFIELDS');
  if (invalidMetafieldErrors.length === 0) {
    throw new Error(`${context} did not return INVALID_METAFIELDS: ${JSON.stringify(payload, null, 2)}`);
  }
}

function readFunctionNodes(captureResult: Capture): FunctionNode[] {
  return readArray(readPath(captureResult.response.payload, ['data', 'shopifyFunctions', 'nodes'])).map(
    (node) => readRecord(node) as FunctionNode,
  );
}

function readCartTransformNodes(captureResult: Capture): JsonRecord[] {
  return readArray(readPath(captureResult.response.payload, ['data', 'cartTransforms', 'nodes'])).map(readRecord);
}

function normalizeFunctionNode(node: FunctionNode): JsonRecord {
  return {
    id: node.id,
    title: node.title,
    handle: node.handle,
    apiType: node.apiType,
    description: node.description,
    appKey: node.appKey,
    app: node.app,
  };
}

function requireCartTransformFunction(nodes: FunctionNode[]): FunctionNode {
  const node =
    nodes.find((candidate) => candidate.handle === 'conformance-cart-transform') ??
    nodes.find((candidate) => candidate.apiType === 'cart_transform');
  if (!node?.id || !node.handle) {
    throw new Error(
      'Expected a released cart-transform Function with id and handle in live shopifyFunctions response.',
    );
  }
  return node;
}

const functionReadDocument = `query ReadCartTransformMetafieldFunctions {
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

const functionHydrateDocument = `query FunctionHydrateById($id: String!) {
  shopifyFunction(id: $id) {
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
`;

const cartTransformDeleteDocument = `mutation DeleteCapturedCartTransform($id: ID!) {
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

const invalidDocument = await loadRequest('functions-cart-transform-create-metafields-invalid.graphql');
const successDocument = await loadRequest('functions-cart-transform-create-metafields-success.graphql');
const readDocument = await loadRequest('functions-cart-transform-create-metafields-read.graphql');
const cleanupReadDocument = await loadRequest('functions-cart-transform-create-validation-read.graphql');

const functionRead = await capture(functionReadDocument);
assertNoTopLevelErrors(functionRead.response, 'Function inventory read');
const cartTransformFunction = requireCartTransformFunction(readFunctionNodes(functionRead));

const cleanupBeforeRead = await capture(cleanupReadDocument, { first: 50 });
assertNoTopLevelErrors(cleanupBeforeRead.response, 'Existing cartTransforms read');
const cleanupBefore: Capture[] = [];
for (const node of readCartTransformNodes(cleanupBeforeRead)) {
  const id = readString(node['id']);
  if (id) cleanupBefore.push(await capture(cartTransformDeleteDocument, { id }));
}

const invalidJsonVariables = {
  functionId: cartTransformFunction.id,
  metafields: [
    {
      namespace: 'bundles',
      key: 'bad_json',
      type: 'json',
      value: 'not-json',
    },
  ],
};
const missingValueVariables = {
  functionId: cartTransformFunction.id,
  metafields: [
    {
      namespace: 'bundles',
      key: 'missing_value',
      type: 'single_line_text_field',
    },
  ],
};
const successVariables = {
  functionId: cartTransformFunction.id,
  blockOnFailure: false,
  metafields: [
    {
      namespace: 'bundles',
      key: 'config',
      type: 'json',
      value: '{"enabled":true}',
    },
    {
      namespace: 'bundles',
      key: 'mode',
      type: 'single_line_text_field',
      value: 'strict',
    },
  ],
};

let createdCartTransformId: string | null = null;
const cleanupAfter: JsonRecord = {};

try {
  const cartTransformCreateMissingValue = await capture(invalidDocument, missingValueVariables);
  assertInvalidMetafields(cartTransformCreateMissingValue, 'cartTransformCreate missing value metafield');

  const cartTransformCreateInvalidJson = await capture(invalidDocument, invalidJsonVariables);
  assertInvalidMetafields(cartTransformCreateInvalidJson, 'cartTransformCreate invalid JSON metafield');

  const cartTransformCreateMetafields = await capture(successDocument, successVariables);
  assertNoUserErrors(cartTransformCreateMetafields.response, 'cartTransformCreate', 'cartTransformCreate metafields');
  createdCartTransformId = readString(
    readPath(cartTransformCreateMetafields.response.payload, ['data', 'cartTransformCreate', 'cartTransform', 'id']),
  );
  if (!createdCartTransformId) {
    throw new Error(
      `cartTransformCreate metafields did not return an id: ${JSON.stringify(
        cartTransformCreateMetafields.response,
        null,
        2,
      )}`,
    );
  }

  const cartTransformsAfterMetafields = await capture(readDocument, { first: 5 });
  assertNoTopLevelErrors(cartTransformsAfterMetafields.response, 'cartTransforms after metafields read');

  const functionHydrate = await runGraphqlRequest<JsonRecord>(functionHydrateDocument, {
    id: cartTransformFunction.id,
  });
  assertNoTopLevelErrors(functionHydrate, 'cart-transform Function hydrate');

  cleanupAfter['cartTransformDelete'] = (
    await runGraphqlRequest(cartTransformDeleteDocument, {
      id: createdCartTransformId,
    })
  ).payload;
  createdCartTransformId = null;

  const fixture = {
    scenarioId: 'functions-cart-transform-create-metafields',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify',
    storeDomain,
    apiVersion,
    summary:
      'cartTransformCreate metafield validation, valid metafield persistence, singular lookup, and downstream cartTransforms read evidence.',
    shopifyFunctions: {
      cartTransform: normalizeFunctionNode(cartTransformFunction),
    },
    functionRead,
    cleanupBefore,
    cartTransformCreateMissingValue,
    cartTransformCreateInvalidJson,
    cartTransformCreateMetafields,
    cartTransformsAfterMetafields,
    cleanupAfter,
    upstreamCalls: [
      {
        operationName: 'FunctionHydrateById',
        variables: { id: cartTransformFunction.id },
        query: functionHydrateDocument,
        response: {
          status: functionHydrate.status,
          body: functionHydrate.payload,
        },
      },
    ],
    notes: {
      validation:
        'Invalid branches are captured before the success path so they prove INVALID_METAFIELDS without colliding with the one-cart-transform-per-Function constraint. Public Shopify Admin 2026-04 accepted an empty namespace during exploratory capture, so empty namespace is not modeled as a cartTransformCreate rejection here.',
      sideEffectBoundary:
        'The invalid branches return cartTransform null; the success branch creates one disposable cart transform that is cleaned up after the fixture is written.',
    },
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (createdCartTransformId) {
    cleanupAfter['cartTransformDelete'] = (
      await runGraphqlRequest(cartTransformDeleteDocument, {
        id: createdCartTransformId,
      })
    ).payload;
  }
}
