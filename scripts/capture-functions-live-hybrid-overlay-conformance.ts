/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const validationFunctionHandle = 'conformance-validation';
const cartTransformFunctionHandle = 'conformance-cart-transform';
const disposableTitlePrefix = 'Functions overlay disposable';
const requestDir = path.join('config', 'parity-requests', 'functions');

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-live-hybrid-overlay-read.json');
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

async function loadRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
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
    current = readRecord(current)[segment];
  }
  return current;
}

function assertNoTopLevelErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)['errors']) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(captureResult: Capture, root: string, context: string): void {
  assertNoTopLevelErrors(captureResult.response, context);
  const payload = readRecord(readPath(captureResult.response.payload, ['data', root]));
  const userErrors = readArray(payload['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function readFunctionNodes(captureResult: Capture): FunctionNode[] {
  return readArray(readPath(captureResult.response.payload, ['data', 'shopifyFunctions', 'nodes'])).map(
    (node) => readRecord(node) as FunctionNode,
  );
}

function readValidationNodes(captureResult: Capture): JsonRecord[] {
  return readArray(readPath(captureResult.response.payload, ['data', 'validations', 'nodes'])).map(readRecord);
}

function readCartTransformNodes(captureResult: Capture): JsonRecord[] {
  return readArray(readPath(captureResult.response.payload, ['data', 'cartTransforms', 'nodes'])).map(readRecord);
}

function requireFunction(nodes: FunctionNode[], handle: string, apiType: string): FunctionNode {
  const node =
    nodes.find((candidate) => candidate.handle === handle) ?? nodes.find((candidate) => candidate.apiType === apiType);
  if (!node?.id || !node.handle) {
    throw new Error(`Missing released Function ${handle}/${apiType}: ${JSON.stringify(nodes, null, 2)}`);
  }
  return node;
}

function validationId(captureResult: Capture, root: string): string {
  const id = readString(readPath(captureResult.response.payload, ['data', root, 'validation', 'id']));
  if (!id) {
    throw new Error(`${root} did not return a validation id: ${JSON.stringify(captureResult.response, null, 2)}`);
  }
  return id;
}

function cartTransformId(captureResult: Capture): string {
  const id = readString(
    readPath(captureResult.response.payload, ['data', 'cartTransformCreate', 'cartTransform', 'id']),
  );
  if (!id) {
    throw new Error(`cartTransformCreate did not return an id: ${JSON.stringify(captureResult.response, null, 2)}`);
  }
  return id;
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

const functionMetadataCatalogHydrateDocument = `query FunctionMetadataCatalogHydrate {
  shopifyFunctions(first: 100) {
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

const functionHydrateByHandleDocument = `query FunctionHydrateByHandle {
  shopifyFunctions(first: 100) {
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

const functionValidationHydrateByIdDocument = `query FunctionValidationHydrateById($id: ID!) {
  validation(id: $id) {
    id
    title
    enabled
    blockOnFailure
    shopifyFunction {
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
    metafields(first: 100) {
      nodes {
        id
        namespace
        key
        type
        value
        updatedAt
      }
    }
  }
}
`;

const functionValidationsHydrateDocument = `query FunctionValidationsHydrate {
  validations(first: 100) {
    nodes {
      id
      title
      enabled
      blockOnFailure
      shopifyFunction {
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
      metafields(first: 100) {
        nodes {
          id
          namespace
          key
          type
          value
          updatedAt
        }
      }
    }
  }
}
`;

const functionCartTransformsHydrateDocument = `query FunctionCartTransformsHydrate {
  cartTransforms(first: 100) {
    nodes {
      id
      functionId
      blockOnFailure
      metafields(first: 100) {
        nodes {
          id
          namespace
          key
          type
          value
          compareDigest
          ownerType
          createdAt
          updatedAt
        }
      }
    }
  }
}
`;

const inventoryDocument = `query FunctionsLiveHybridOverlayInventory {
  validations(first: 100) {
    nodes {
      id
      title
      shopifyFunction {
        id
        handle
        apiType
      }
    }
  }
  cartTransforms(first: 100) {
    nodes {
      id
      functionId
    }
  }
}
`;

const validationDeleteDocument = `mutation FunctionsLiveHybridOverlayValidationCleanup($id: ID!) {
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

const cartTransformDeleteDocument = `mutation FunctionsLiveHybridOverlayCartTransformCleanup($id: ID!) {
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

const baseCartTransformCreateDocument = `mutation FunctionsLiveHybridOverlayBaseCartTransform(
  $functionId: String!
  $blockOnFailure: Boolean
  $metafields: [MetafieldInput!]
) {
  cartTransformCreate(functionId: $functionId, blockOnFailure: $blockOnFailure, metafields: $metafields) {
    cartTransform {
      id
      functionId
      blockOnFailure
      metafield(namespace: "bundles", key: "config") {
        namespace
        key
        type
        value
        ownerType
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const baseValidationCreateDocument = await loadRequest('functions-live-hybrid-overlay-stage.graphql');
const stagedValidationCreateDocument = baseValidationCreateDocument;
const overlayReadDocument = await loadRequest('functions-live-hybrid-overlay-read.graphql');

async function cleanupExisting(
  validationFunction: FunctionNode,
  cartFunction: FunctionNode,
): Promise<{
  inventory: Capture;
  validationDeletes: Capture[];
  cartTransformDeletes: Capture[];
}> {
  const inventory = await capture(inventoryDocument);
  assertNoTopLevelErrors(inventory.response, 'Function inventory cleanup read');
  const validationDeletes: Capture[] = [];
  const cartTransformDeletes: Capture[] = [];

  for (const node of readValidationNodes(inventory)) {
    const id = readString(node['id']);
    const title = readString(node['title']);
    const functionNode = readRecord(node['shopifyFunction']);
    if (
      id &&
      (title?.startsWith(disposableTitlePrefix) ||
        functionNode['id'] === validationFunction.id ||
        functionNode['handle'] === validationFunction.handle)
    ) {
      validationDeletes.push(await capture(validationDeleteDocument, { id }));
    }
  }

  for (const node of readCartTransformNodes(inventory)) {
    const id = readString(node['id']);
    if (id && node['functionId'] === cartFunction.id) {
      cartTransformDeletes.push(await capture(cartTransformDeleteDocument, { id }));
    }
  }

  return { inventory, validationDeletes, cartTransformDeletes };
}

const functionLookup = await capture(functionMetadataCatalogHydrateDocument);
assertNoTopLevelErrors(functionLookup.response, 'shopifyFunctions lookup');
const functionNodes = readFunctionNodes(functionLookup);
const validationFunction = requireFunction(functionNodes, validationFunctionHandle, 'cart_checkout_validation');
const cartFunction = requireFunction(functionNodes, cartTransformFunctionHandle, 'cart_transform');

const cleanupBefore = await cleanupExisting(validationFunction, cartFunction);

let baseValidationId: string | null = null;
let stagedValidationId: string | null = null;
let baseCartTransformId: string | null = null;
const cleanupAfter: Capture[] = [];

try {
  const baseValidationCreate = await capture(baseValidationCreateDocument, {
    validation: {
      functionHandle: validationFunction.handle,
      title: `${disposableTitlePrefix} base validation`,
      enable: true,
      blockOnFailure: true,
    },
  });
  assertNoUserErrors(baseValidationCreate, 'validationCreate', 'base validationCreate');
  baseValidationId = validationId(baseValidationCreate, 'validationCreate');

  const baseCartTransformCreate = await capture(baseCartTransformCreateDocument, {
    functionId: cartFunction.id,
    blockOnFailure: false,
    metafields: [
      {
        namespace: 'bundles',
        key: 'config',
        type: 'json',
        value: '{"mode":"base"}',
      },
    ],
  });
  assertNoUserErrors(baseCartTransformCreate, 'cartTransformCreate', 'base cartTransformCreate');
  baseCartTransformId = cartTransformId(baseCartTransformCreate);

  const functionHydrateByHandle = await capture(functionHydrateByHandleDocument, {
    handle: validationFunction.handle,
    apiType: 'VALIDATION',
  });
  assertNoTopLevelErrors(functionHydrateByHandle.response, 'FunctionHydrateByHandle cassette');

  const functionValidationHydrateById = await capture(functionValidationHydrateByIdDocument, {
    id: baseValidationId,
  });
  assertNoTopLevelErrors(functionValidationHydrateById.response, 'FunctionValidationHydrateById cassette');

  const functionValidationsHydrate = await capture(functionValidationsHydrateDocument);
  assertNoTopLevelErrors(functionValidationsHydrate.response, 'FunctionValidationsHydrate cassette');

  const functionCartTransformsHydrate = await capture(functionCartTransformsHydrateDocument);
  assertNoTopLevelErrors(functionCartTransformsHydrate.response, 'FunctionCartTransformsHydrate cassette');

  const functionMetadataCatalogHydrate = await capture(functionMetadataCatalogHydrateDocument);
  assertNoTopLevelErrors(functionMetadataCatalogHydrate.response, 'FunctionMetadataCatalogHydrate cassette');

  const stagedValidationCreate = await capture(stagedValidationCreateDocument, {
    validation: {
      functionHandle: validationFunction.handle,
      title: `${disposableTitlePrefix} staged validation`,
      enable: true,
      blockOnFailure: false,
    },
  });
  assertNoUserErrors(stagedValidationCreate, 'validationCreate', 'staged validationCreate');
  stagedValidationId = validationId(stagedValidationCreate, 'validationCreate');

  const overlayRead = await capture(overlayReadDocument, {
    stagedValidationId,
    baseValidationId,
    cartFunctionId: cartFunction.id,
    cartFunctionApiType: cartFunction.apiType ?? 'cart_transform',
  });
  assertNoTopLevelErrors(overlayRead.response, 'Functions overlay read');

  const fixture = {
    scenarioId: 'functions-live-hybrid-overlay-read',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify',
    storeDomain,
    apiVersion,
    summary:
      'Live Functions overlay evidence with one existing validation, one existing cart transform, and one later validation lifecycle.',
    shopifyFunctions: {
      validation: normalizeFunctionNode(validationFunction),
      cartTransform: normalizeFunctionNode(cartFunction),
    },
    cleanupBefore,
    baseValidationCreate,
    baseCartTransformCreate,
    stagedValidationCreate,
    overlayRead,
    cleanupAfter,
    upstreamCalls: [
      {
        operationName: 'FunctionHydrateByHandle',
        variables: {
          handle: validationFunction.handle,
          apiType: 'VALIDATION',
        },
        query: functionHydrateByHandle.query,
        response: {
          status: functionHydrateByHandle.response.status,
          body: functionHydrateByHandle.response.payload,
        },
      },
      {
        operationName: 'FunctionValidationHydrateById',
        variables: { id: baseValidationId },
        query: functionValidationHydrateById.query,
        response: {
          status: functionValidationHydrateById.response.status,
          body: functionValidationHydrateById.response.payload,
        },
      },
      {
        operationName: 'FunctionValidationsHydrate',
        variables: {},
        query: functionValidationsHydrate.query,
        response: {
          status: functionValidationsHydrate.response.status,
          body: functionValidationsHydrate.response.payload,
        },
      },
      {
        operationName: 'FunctionCartTransformsHydrate',
        variables: {},
        query: functionCartTransformsHydrate.query,
        response: {
          status: functionCartTransformsHydrate.response.status,
          body: functionCartTransformsHydrate.response.payload,
        },
      },
      {
        operationName: 'FunctionMetadataCatalogHydrate',
        variables: {},
        query: functionMetadataCatalogHydrate.query,
        response: {
          status: functionMetadataCatalogHydrate.response.status,
          body: functionMetadataCatalogHydrate.response.payload,
        },
      },
    ],
    notes: {
      setup:
        'The script removes disposable Function resources for the released conformance functions, creates one base validation and one base cart transform, records upstream hydrate cassettes from that base state, then creates the validation lifecycle that the proxy stages locally.',
      cleanup:
        'The finally block deletes the base validation, staged validation, and base cart transform when they were created.',
    },
  };

  if (stagedValidationId) cleanupAfter.push(await capture(validationDeleteDocument, { id: stagedValidationId }));
  stagedValidationId = null;
  if (baseValidationId) cleanupAfter.push(await capture(validationDeleteDocument, { id: baseValidationId }));
  baseValidationId = null;
  if (baseCartTransformId) cleanupAfter.push(await capture(cartTransformDeleteDocument, { id: baseCartTransformId }));
  baseCartTransformId = null;

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath, storeDomain, apiVersion }, null, 2));
} finally {
  if (stagedValidationId) {
    cleanupAfter.push(await capture(validationDeleteDocument, { id: stagedValidationId }));
  }
  if (baseValidationId) {
    cleanupAfter.push(await capture(validationDeleteDocument, { id: baseValidationId }));
  }
  if (baseCartTransformId) {
    cleanupAfter.push(await capture(cartTransformDeleteDocument, { id: baseCartTransformId }));
  }
}
