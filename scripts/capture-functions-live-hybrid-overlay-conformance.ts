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

const functionHydrateByIdDocument = `query FunctionHydrateById($id: String!) {
  shopifyFunction(id: $id) {
    id
    title
    apiType
    description
    appKey
    app {
      __typename
      id
      title
      apiKey
    }
  }
}
`;

const functionConnectionWindowHydrateThreeDocument = `query FunctionConnectionWindowHydrate { validations(first: 3, reverse: true) { edges { cursor node { id title enabled blockOnFailure shopifyFunction { id apiType } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }`;
const functionConnectionWindowHydrateFourDocument = `query FunctionConnectionWindowHydrate { validations(first: 4, reverse: true) { edges { cursor node { id title enabled blockOnFailure shopifyFunction { id apiType } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }`;

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
const windowReadDocument = await loadRequest('functions-live-hybrid-overlay-window.graphql');
const stagedValidationDeleteDocument = await loadRequest('functions-live-hybrid-overlay-delete.graphql');

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
let refillValidationId: string | null = null;
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

  const refillValidationCreate = await capture(baseValidationCreateDocument, {
    validation: {
      functionHandle: validationFunction.handle,
      title: `${disposableTitlePrefix} refill validation`,
      enable: false,
      blockOnFailure: false,
    },
  });
  assertNoUserErrors(refillValidationCreate, 'validationCreate', 'refill validationCreate');
  refillValidationId = validationId(refillValidationCreate, 'validationCreate');

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

  const functionHydrateById = await capture(functionHydrateByIdDocument, {
    id: validationFunction.id,
  });
  assertNoTopLevelErrors(functionHydrateById.response, 'FunctionHydrateById cassette');

  const baseWindowFirst = await capture(windowReadDocument, { after: null });
  assertNoTopLevelErrors(baseWindowFirst.response, 'base first window cassette');
  const baseWindowRefillThree = await capture(functionConnectionWindowHydrateThreeDocument);
  assertNoTopLevelErrors(baseWindowRefillThree.response, 'base three-row refill cassette');
  const baseWindowRefillFour = await capture(functionConnectionWindowHydrateFourDocument);
  assertNoTopLevelErrors(baseWindowRefillFour.response, 'base four-row refill cassette');

  const stagedValidationCreate = await capture(stagedValidationCreateDocument, {
    validation: {
      functionId: validationFunction.id,
      title: `${disposableTitlePrefix} staged validation`,
      enable: true,
      blockOnFailure: false,
    },
  });
  assertNoUserErrors(stagedValidationCreate, 'validationCreate', 'staged validationCreate');
  stagedValidationId = validationId(stagedValidationCreate, 'validationCreate');

  const windowFirst = await capture(windowReadDocument, { after: null });
  assertNoTopLevelErrors(windowFirst.response, 'Functions overlay first window');
  const stagedWindowCursor = readString(
    readPath(windowFirst.response.payload, ['data', 'validations', 'pageInfo', 'endCursor']),
  );
  if (!stagedWindowCursor) {
    throw new Error(`Functions overlay first window did not return a cursor: ${JSON.stringify(windowFirst, null, 2)}`);
  }
  const windowAfter = await capture(windowReadDocument, { after: stagedWindowCursor });
  assertNoTopLevelErrors(windowAfter.response, 'Functions overlay after window');

  const refillValidationDelete = await capture(stagedValidationDeleteDocument, { id: refillValidationId });
  assertNoUserErrors(refillValidationDelete, 'validationDelete', 'refill validationDelete');
  refillValidationId = null;
  const windowAfterTombstone = await capture(windowReadDocument, { after: stagedWindowCursor });
  assertNoTopLevelErrors(windowAfterTombstone.response, 'Functions overlay tombstone refill window');

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
      'Live Functions overlay evidence with two existing validations, one existing cart transform, and one later validation lifecycle.',
    shopifyFunctions: {
      validation: normalizeFunctionNode(validationFunction),
      cartTransform: normalizeFunctionNode(cartFunction),
    },
    cleanupBefore,
    baseValidationCreate,
    refillValidationCreate,
    baseCartTransformCreate,
    stagedValidationCreate,
    baseWindowFirst,
    baseWindowRefillThree,
    baseWindowRefillFour,
    windowFirst,
    windowAfter,
    refillValidationDelete,
    windowAfterTombstone,
    overlayRead,
    cleanupAfter,
    upstreamCalls: [
      {
        operationName: 'FunctionHydrateById',
        variables: { id: validationFunction.id },
        query: functionHydrateById.query,
        response: {
          status: functionHydrateById.response.status,
          body: functionHydrateById.response.payload,
        },
      },
      {
        operationName: 'FunctionsLiveHybridOverlayWindow',
        variables: { after: null },
        query: baseWindowFirst.query,
        response: {
          status: baseWindowFirst.response.status,
          body: baseWindowFirst.response.payload,
        },
      },
      {
        operationName: 'FunctionConnectionWindowHydrate',
        variables: {},
        query: baseWindowRefillThree.query,
        response: {
          status: baseWindowRefillThree.response.status,
          body: baseWindowRefillThree.response.payload,
        },
      },
      {
        operationName: 'FunctionConnectionWindowHydrate',
        variables: {},
        query: baseWindowRefillFour.query,
        response: {
          status: baseWindowRefillFour.response.status,
          body: baseWindowRefillFour.response.payload,
        },
      },
    ],
    notes: {
      setup:
        'The script removes disposable Function resources for the released conformance functions, creates two base validations and one base cart transform, records exact first-page and bounded-refill cassettes from that base state, then creates the validation lifecycle that the proxy stages locally.',
      cleanup:
        'The finally block deletes the base validation, staged validation, and base cart transform when they were created.',
    },
  };

  if (stagedValidationId) cleanupAfter.push(await capture(validationDeleteDocument, { id: stagedValidationId }));
  stagedValidationId = null;
  if (baseValidationId) cleanupAfter.push(await capture(validationDeleteDocument, { id: baseValidationId }));
  baseValidationId = null;
  if (refillValidationId) cleanupAfter.push(await capture(validationDeleteDocument, { id: refillValidationId }));
  refillValidationId = null;
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
  if (refillValidationId) {
    cleanupAfter.push(await capture(validationDeleteDocument, { id: refillValidationId }));
  }
  if (baseCartTransformId) {
    cleanupAfter.push(await capture(cartTransformDeleteDocument, { id: baseCartTransformId }));
  }
}
