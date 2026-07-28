/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const validationFunctionHandle = 'conformance-validation';
const cartTransformFunctionHandle = 'conformance-cart-transform';
const fulfillmentConstraintFunctionHandle = 'conformance-fulfillment-constraint';
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

function readFulfillmentConstraintRuleNodes(captureResult: Capture): JsonRecord[] {
  return readArray(readPath(captureResult.response.payload, ['data', 'fulfillmentConstraintRules'])).map(readRecord);
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

function fulfillmentConstraintRuleId(captureResult: Capture): string {
  const id = readString(
    readPath(captureResult.response.payload, [
      'data',
      'fulfillmentConstraintRuleCreate',
      'fulfillmentConstraintRule',
      'id',
    ]),
  );
  if (!id) {
    throw new Error(
      `fulfillmentConstraintRuleCreate did not return an id: ${JSON.stringify(captureResult.response, null, 2)}`,
    );
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
const cartTransformConnectionWindowHydrateThreeDocument = `query FunctionConnectionWindowHydrate { cartTransforms(first: 3) { edges { cursor node { id functionId blockOnFailure } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }`;
const fulfillmentConstraintListHydrateDocument = `query FunctionListWindowHydrate { fulfillmentConstraintRules { deliveryMethodTypes function { handle apiType } id } }`;

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
  fulfillmentConstraintRules {
    id
    function {
      id
      handle
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

const fulfillmentConstraintRuleDeleteDocument = `mutation FunctionsLiveHybridOverlayFulfillmentRuleCleanup($id: ID!) {
  fulfillmentConstraintRuleDelete(id: $id) {
    success
    userErrors {
      field
      message
      code
    }
  }
}
`;

const baseFulfillmentConstraintRuleCreateDocument = `mutation FunctionsLiveHybridOverlayFulfillmentRuleSetup(
  $functionId: String!
) {
  fulfillmentConstraintRuleCreate(functionId: $functionId, deliveryMethodTypes: [SHIPPING]) {
    fulfillmentConstraintRule {
      id
      deliveryMethodTypes
      function {
        id
        handle
        apiType
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
const stagedCartTransformCreateDocument = await loadRequest('functions-live-hybrid-cart-overlay-stage.graphql');
const cartTransformWindowReadDocument = await loadRequest('functions-live-hybrid-cart-overlay-window.graphql');
const stagedCartTransformDeleteDocument = await loadRequest('functions-live-hybrid-cart-overlay-delete.graphql');
const fulfillmentConstraintRuleBaseReadDocument = await loadRequest(
  'functions-live-hybrid-fulfillment-rule-base-read.graphql',
);
const stagedFulfillmentConstraintRuleDeleteDocument = await loadRequest(
  'functions-live-hybrid-fulfillment-rule-delete.graphql',
);
const fulfillmentConstraintRuleTombstoneReadDocument = await loadRequest(
  'functions-live-hybrid-fulfillment-rule-tombstone-read.graphql',
);

async function cleanupExisting(
  validationFunction: FunctionNode,
  cartFunctions: FunctionNode[],
  fulfillmentConstraintFunction: FunctionNode,
): Promise<{
  inventory: Capture;
  validationDeletes: Capture[];
  cartTransformDeletes: Capture[];
  fulfillmentConstraintRuleDeletes: Capture[];
}> {
  const inventory = await capture(inventoryDocument);
  assertNoTopLevelErrors(inventory.response, 'Function inventory cleanup read');
  const validationDeletes: Capture[] = [];
  const cartTransformDeletes: Capture[] = [];
  const fulfillmentConstraintRuleDeletes: Capture[] = [];

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
    if (id && cartFunctions.some((functionNode) => node['functionId'] === functionNode.id)) {
      cartTransformDeletes.push(await capture(cartTransformDeleteDocument, { id }));
    }
  }

  for (const node of readFulfillmentConstraintRuleNodes(inventory)) {
    const id = readString(node['id']);
    const functionNode = readRecord(node['function']);
    if (
      id &&
      (functionNode['id'] === fulfillmentConstraintFunction.id ||
        functionNode['handle'] === fulfillmentConstraintFunction.handle)
    ) {
      fulfillmentConstraintRuleDeletes.push(await capture(fulfillmentConstraintRuleDeleteDocument, { id }));
    }
  }

  return { inventory, validationDeletes, cartTransformDeletes, fulfillmentConstraintRuleDeletes };
}

const functionLookup = await capture(functionMetadataCatalogHydrateDocument);
assertNoTopLevelErrors(functionLookup.response, 'shopifyFunctions lookup');
const functionNodes = readFunctionNodes(functionLookup);
const validationFunction = requireFunction(functionNodes, validationFunctionHandle, 'cart_checkout_validation');
const cartFunction = requireFunction(functionNodes, cartTransformFunctionHandle, 'cart_transform');
const cartFunctions = functionNodes.filter((functionNode) => functionNode.apiType === 'cart_transform');
const fulfillmentConstraintFunction = requireFunction(
  functionNodes,
  fulfillmentConstraintFunctionHandle,
  'fulfillment_constraints',
);

const cleanupBefore = await cleanupExisting(validationFunction, cartFunctions, fulfillmentConstraintFunction);

let baseValidationId: string | null = null;
let refillValidationId: string | null = null;
let stagedValidationId: string | null = null;
let stagedCartTransformId: string | null = null;
let baseFulfillmentConstraintRuleId: string | null = null;
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

  const baseFulfillmentConstraintRuleCreate = await capture(baseFulfillmentConstraintRuleCreateDocument, {
    functionId: fulfillmentConstraintFunction.id,
  });
  assertNoUserErrors(
    baseFulfillmentConstraintRuleCreate,
    'fulfillmentConstraintRuleCreate',
    'base fulfillmentConstraintRuleCreate',
  );
  baseFulfillmentConstraintRuleId = fulfillmentConstraintRuleId(baseFulfillmentConstraintRuleCreate);

  const validationFunctionHydrateById = await capture(functionHydrateByIdDocument, {
    id: validationFunction.id,
  });
  assertNoTopLevelErrors(validationFunctionHydrateById.response, 'validation FunctionHydrateById cassette');

  const baseWindowFirst = await capture(windowReadDocument, { after: null });
  assertNoTopLevelErrors(baseWindowFirst.response, 'base first window cassette');
  const baseWindowRefillThree = await capture(functionConnectionWindowHydrateThreeDocument);
  assertNoTopLevelErrors(baseWindowRefillThree.response, 'base three-row refill cassette');
  const baseWindowRefillFour = await capture(functionConnectionWindowHydrateFourDocument);
  assertNoTopLevelErrors(baseWindowRefillFour.response, 'base four-row refill cassette');

  const cartFunctionHydrateById = await capture(functionHydrateByIdDocument, {
    id: cartFunction.id,
  });
  assertNoTopLevelErrors(cartFunctionHydrateById.response, 'cart FunctionHydrateById cassette');
  const cartWindowBaseFirst = await capture(cartTransformWindowReadDocument, { after: null });
  assertNoTopLevelErrors(cartWindowBaseFirst.response, 'base cart-transform first window cassette');
  const cartWindowBaseRefill = await capture(cartTransformConnectionWindowHydrateThreeDocument);
  assertNoTopLevelErrors(cartWindowBaseRefill.response, 'base cart-transform bounded refill cassette');
  const cartWindowBaseAfterDelete = await capture(cartTransformWindowReadDocument, { after: null });
  assertNoTopLevelErrors(cartWindowBaseAfterDelete.response, 'base cart-transform post-delete window cassette');

  const fulfillmentConstraintRuleBaseRead = await capture(fulfillmentConstraintRuleBaseReadDocument);
  assertNoTopLevelErrors(fulfillmentConstraintRuleBaseRead.response, 'base fulfillment-rule read');
  const fulfillmentConstraintRuleBaseWithoutIdentity = await capture(fulfillmentConstraintRuleTombstoneReadDocument);
  assertNoTopLevelErrors(
    fulfillmentConstraintRuleBaseWithoutIdentity.response,
    'base fulfillment-rule identity-omitting cassette',
  );
  const fulfillmentConstraintRuleBaseIdentityHydrate = await capture(fulfillmentConstraintListHydrateDocument);
  assertNoTopLevelErrors(
    fulfillmentConstraintRuleBaseIdentityHydrate.response,
    'base fulfillment-rule identity hydrate cassette',
  );

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

  const stagedCartTransformCreate = await capture(stagedCartTransformCreateDocument, {
    functionId: cartFunction.id,
  });
  assertNoUserErrors(stagedCartTransformCreate, 'cartTransformCreate', 'staged cartTransformCreate');
  stagedCartTransformId = cartTransformId(stagedCartTransformCreate);
  const cartWindowFirst = await capture(cartTransformWindowReadDocument, { after: null });
  assertNoTopLevelErrors(cartWindowFirst.response, 'cart-transform overlay first window');
  const stagedCartWindowCursor = readString(
    readPath(cartWindowFirst.response.payload, ['data', 'cartTransforms', 'pageInfo', 'endCursor']),
  );
  if (!stagedCartWindowCursor) {
    throw new Error(
      `Cart-transform overlay first window did not return a cursor: ${JSON.stringify(cartWindowFirst, null, 2)}`,
    );
  }
  const cartWindowAfter = await capture(cartTransformWindowReadDocument, { after: stagedCartWindowCursor });
  assertNoTopLevelErrors(cartWindowAfter.response, 'cart-transform overlay after window');
  const stagedCartTransformDelete = await capture(stagedCartTransformDeleteDocument, {
    id: stagedCartTransformId,
  });
  assertNoUserErrors(stagedCartTransformDelete, 'cartTransformDelete', 'staged cartTransformDelete');
  stagedCartTransformId = null;
  const cartWindowAfterDelete = await capture(cartTransformWindowReadDocument, { after: null });
  assertNoTopLevelErrors(cartWindowAfterDelete.response, 'cart-transform window after staged delete');

  const stagedFulfillmentConstraintRuleDelete = await capture(stagedFulfillmentConstraintRuleDeleteDocument, {
    id: baseFulfillmentConstraintRuleId,
  });
  assertNoUserErrors(
    stagedFulfillmentConstraintRuleDelete,
    'fulfillmentConstraintRuleDelete',
    'base fulfillmentConstraintRuleDelete',
  );
  baseFulfillmentConstraintRuleId = null;
  const fulfillmentConstraintRulesAfterTombstone = await capture(fulfillmentConstraintRuleTombstoneReadDocument);
  assertNoTopLevelErrors(
    fulfillmentConstraintRulesAfterTombstone.response,
    'fulfillment-rule list after base tombstone',
  );

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
      'Live Functions overlay evidence for validation windows, a staged cart-transform local-cursor refill, and a fulfillment-rule base tombstone with identity-only refill.',
    shopifyFunctions: {
      validation: normalizeFunctionNode(validationFunction),
      cartTransform: normalizeFunctionNode(cartFunction),
      fulfillmentConstraint: normalizeFunctionNode(fulfillmentConstraintFunction),
    },
    cleanupBefore,
    baseValidationCreate,
    refillValidationCreate,
    baseFulfillmentConstraintRuleCreate,
    stagedValidationCreate,
    baseWindowFirst,
    baseWindowRefillThree,
    baseWindowRefillFour,
    windowFirst,
    windowAfter,
    refillValidationDelete,
    windowAfterTombstone,
    stagedCartTransformCreate,
    cartWindowFirst,
    cartWindowAfter,
    stagedCartTransformDelete,
    cartWindowAfterDelete,
    fulfillmentConstraintRuleBaseRead,
    stagedFulfillmentConstraintRuleDelete,
    fulfillmentConstraintRulesAfterTombstone,
    overlayRead,
    cleanupAfter,
    upstreamCalls: [
      {
        operationName: 'FunctionHydrateById',
        variables: { id: validationFunction.id },
        query: validationFunctionHydrateById.query,
        response: {
          status: validationFunctionHydrateById.response.status,
          body: validationFunctionHydrateById.response.payload,
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
      {
        operationName: 'FunctionHydrateById',
        variables: { id: cartFunction.id },
        query: cartFunctionHydrateById.query,
        response: {
          status: cartFunctionHydrateById.response.status,
          body: cartFunctionHydrateById.response.payload,
        },
      },
      {
        operationName: 'FunctionsLiveHybridCartOverlayWindow',
        variables: { after: null },
        query: cartWindowBaseFirst.query,
        response: {
          status: cartWindowBaseFirst.response.status,
          body: cartWindowBaseFirst.response.payload,
        },
      },
      {
        operationName: 'FunctionConnectionWindowHydrate',
        variables: {},
        query: cartWindowBaseRefill.query,
        response: {
          status: cartWindowBaseRefill.response.status,
          body: cartWindowBaseRefill.response.payload,
        },
      },
      {
        operationName: 'FunctionsLiveHybridCartOverlayWindow',
        variables: { after: null },
        query: cartWindowBaseAfterDelete.query,
        response: {
          status: cartWindowBaseAfterDelete.response.status,
          body: cartWindowBaseAfterDelete.response.payload,
        },
      },
      {
        operationName: 'FunctionsLiveHybridFulfillmentRuleBaseRead',
        variables: {},
        query: fulfillmentConstraintRuleBaseRead.query,
        response: {
          status: fulfillmentConstraintRuleBaseRead.response.status,
          body: fulfillmentConstraintRuleBaseRead.response.payload,
        },
      },
      {
        operationName: 'FunctionsLiveHybridFulfillmentRuleTombstoneRead',
        variables: {},
        query: fulfillmentConstraintRuleBaseWithoutIdentity.query,
        response: {
          status: fulfillmentConstraintRuleBaseWithoutIdentity.response.status,
          body: fulfillmentConstraintRuleBaseWithoutIdentity.response.payload,
        },
      },
      {
        operationName: 'FunctionListWindowHydrate',
        variables: {},
        query: fulfillmentConstraintRuleBaseIdentityHydrate.query,
        response: {
          status: fulfillmentConstraintRuleBaseIdentityHydrate.response.status,
          body: fulfillmentConstraintRuleBaseIdentityHydrate.response.payload,
        },
      },
    ],
    notes: {
      setup:
        'The script removes disposable Function resources, creates two base validations and one base fulfillment rule, records exact validation/cart/list window cassettes, then runs the validation and cart-transform lifecycles plus the fulfillment-rule tombstone through public Admin GraphQL.',
      cleanup:
        'The finally block deletes every validation, cart transform, and fulfillment rule created by this capture when an earlier assertion fails.',
    },
  };

  if (stagedValidationId) cleanupAfter.push(await capture(validationDeleteDocument, { id: stagedValidationId }));
  stagedValidationId = null;
  if (baseValidationId) cleanupAfter.push(await capture(validationDeleteDocument, { id: baseValidationId }));
  baseValidationId = null;
  if (refillValidationId) cleanupAfter.push(await capture(validationDeleteDocument, { id: refillValidationId }));
  refillValidationId = null;
  if (stagedCartTransformId) {
    cleanupAfter.push(await capture(cartTransformDeleteDocument, { id: stagedCartTransformId }));
  }
  stagedCartTransformId = null;
  if (baseFulfillmentConstraintRuleId) {
    cleanupAfter.push(await capture(fulfillmentConstraintRuleDeleteDocument, { id: baseFulfillmentConstraintRuleId }));
  }
  baseFulfillmentConstraintRuleId = null;

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
  if (stagedCartTransformId) {
    cleanupAfter.push(await capture(cartTransformDeleteDocument, { id: stagedCartTransformId }));
  }
  if (baseFulfillmentConstraintRuleId) {
    cleanupAfter.push(await capture(fulfillmentConstraintRuleDeleteDocument, { id: baseFulfillmentConstraintRuleId }));
  }
}
