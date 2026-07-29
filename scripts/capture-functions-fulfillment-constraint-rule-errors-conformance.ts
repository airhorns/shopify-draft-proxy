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
const outputPath = path.join(outputDir, 'functions-fulfillment-constraint-rule-errors.json');
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

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    current = readRecord(current)[part];
  }
  return current;
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

function requiredString(value: unknown, context: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${context} must be a non-empty string, received ${JSON.stringify(value)}`);
  }
  return value;
}

function readRule(captureResult: Capture, alias: string): Record<string, unknown> {
  const branch = assertNoUserErrors(captureResult, alias);
  const rule = readRecord(branch['fulfillmentConstraintRule']);
  requiredString(rule['id'], `${alias} fulfillmentConstraintRule.id`);
  return rule;
}

function readRules(captureResult: Capture): Record<string, unknown>[] {
  assertNoTopLevelErrors(captureResult, 'fulfillmentConstraintRules read');
  return readArray(readPath(captureResult.response.payload, ['data', 'fulfillmentConstraintRules'])).map(readRecord);
}

function assertJsonEqual(actual: unknown, expected: unknown, context: string): void {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${context} mismatch: ${JSON.stringify({ expected, actual }, null, 2)}`);
  }
}

function assertNoUserErrors(captureResult: Capture, alias: string): Record<string, unknown> {
  assertNoTopLevelErrors(captureResult, alias);
  const branch = readRecord(readPath(captureResult.response.payload, ['data', alias]));
  const userErrors = readArray(branch['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${alias} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
  return branch;
}

function assertPayloadUserError(
  captureResult: Capture,
  alias: string,
  expected: { code: string; field: string[]; message: string },
): void {
  const payload = readPath(captureResult.response.payload, ['data', alias]);
  const branch = readRecord(payload);
  const userErrors = readArray(branch['userErrors']);
  const actual = readRecord(userErrors[0]);
  const returnedRule = branch['fulfillmentConstraintRule'];
  if (
    userErrors.length !== 1 ||
    returnedRule !== null ||
    actual['code'] !== expected.code ||
    actual['message'] !== expected.message ||
    JSON.stringify(actual['field']) !== JSON.stringify(expected.field)
  ) {
    throw new Error(
      `${alias} userError mismatch: ${JSON.stringify(
        {
          expected,
          actual: branch,
        },
        null,
        2,
      )}`,
    );
  }
}

function assertDeleteUnknown(captureResult: Capture): void {
  const branch = readRecord(readPath(captureResult.response.payload, ['data', 'deleteUnknown']));
  const userErrors = readArray(branch['userErrors']);
  const actual = readRecord(userErrors[0]);
  const expectedId = 'gid://shopify/FulfillmentConstraintRule/999999999999';
  if (
    branch['success'] !== false ||
    userErrors.length !== 1 ||
    actual['code'] !== 'NOT_FOUND' ||
    JSON.stringify(actual['field']) !== JSON.stringify(['id']) ||
    actual['message'] !== `Could not find FulfillmentConstraintRule with id: ${expectedId}`
  ) {
    throw new Error(`deleteUnknown userError mismatch: ${JSON.stringify(branch, null, 2)}`);
  }
}

const schemaIntrospectionDocument = `query FulfillmentConstraintRuleSchemaSnapshot {
  __schema {
    queryType {
      fields {
        name
      }
    }
    mutationType {
      fields {
        name
        args {
          name
        }
      }
    }
  }
}
`;

const errorShapeDocument = `mutation FulfillmentConstraintRuleErrorShape {
  missing: fulfillmentConstraintRuleCreate(deliveryMethodTypes: [SHIPPING]) {
    fulfillmentConstraintRule {
      id
    }
    userErrors {
      code
      field
      message
    }
  }
  multiple: fulfillmentConstraintRuleCreate(
    functionId: "gid://shopify/ShopifyFunction/999999999999"
    functionHandle: "definitely-missing-fulfillment-constraint"
    deliveryMethodTypes: [SHIPPING]
  ) {
    fulfillmentConstraintRule {
      id
    }
    userErrors {
      code
      field
      message
    }
  }
  emptyDelivery: fulfillmentConstraintRuleCreate(
    functionId: "gid://shopify/ShopifyFunction/999999999999"
    deliveryMethodTypes: []
  ) {
    fulfillmentConstraintRule {
      id
    }
    userErrors {
      code
      field
      message
    }
  }
  deleteUnknown: fulfillmentConstraintRuleDelete(id: "gid://shopify/FulfillmentConstraintRule/999999999999") {
    success
    userErrors {
      code
      field
      message
    }
  }
  updateUnknown: fulfillmentConstraintRuleUpdate(
    id: "gid://shopify/FulfillmentConstraintRule/999999999999"
    deliveryMethodTypes: [PICK_UP]
  ) {
    fulfillmentConstraintRule {
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

const readEmptyDocument = `query FulfillmentConstraintRulesEmptyRead {
  fulfillmentConstraintRules {
    id
    deliveryMethodTypes
    function {
      id
      handle
      apiType
    }
  }
}
`;

const unknownFunctionDocument = `mutation FulfillmentConstraintRuleUnknownFunction {
  unknownId: fulfillmentConstraintRuleCreate(
    functionId: "gid://shopify/ShopifyFunction/999999999999"
    deliveryMethodTypes: [SHIPPING]
  ) {
    fulfillmentConstraintRule {
      id
    }
    userErrors {
      code
      field
      message
    }
  }
  unknownHandle: fulfillmentConstraintRuleCreate(
    functionHandle: "definitely-missing-fulfillment-constraint"
    deliveryMethodTypes: [SHIPPING]
  ) {
    fulfillmentConstraintRule {
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

const functionCatalogDocument = `query FulfillmentConstraintRuleFunctionCatalog {
  fulfillmentConstraintFunctions: shopifyFunctions(first: 100) {
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

const fulfillmentConstraintRulesHydrateDocument = `query FunctionFulfillmentConstraintRulesHydrate {
  fulfillmentConstraintRules {
    id
    deliveryMethodTypes
    function {
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
        compareDigest
        ownerType
        createdAt
        updatedAt
      }
    }
  }
}
`;

const createExistingRuleDocument = `mutation FulfillmentConstraintRuleCreateForUpdate(
  $functionHandle: String!
  $deliveryMethodTypes: [DeliveryMethodType!]!
) {
  setupCreate: fulfillmentConstraintRuleCreate(
    functionHandle: $functionHandle
    deliveryMethodTypes: $deliveryMethodTypes
    metafields: [{
      namespace: "custom"
      key: "update-config"
      type: "json"
      value: "{\\"preserve\\":true}"
    }]
  ) {
    fulfillmentConstraintRule {
      id
      deliveryMethodTypes
      function {
        id
        title
        handle
        apiType
        description
        appKey
        app { __typename id title handle apiKey }
      }
      metafields(first: 100) {
        nodes { id namespace key type value compareDigest ownerType createdAt updatedAt }
      }
    }
    userErrors { code field message }
  }
}
`;

const updateExistingRuleDocument = `mutation FulfillmentConstraintRuleUpdateExisting(
  $id: ID!
  $deliveryMethodTypes: [DeliveryMethodType!]!
) {
  successfulUpdate: fulfillmentConstraintRuleUpdate(
    id: $id
    deliveryMethodTypes: $deliveryMethodTypes
  ) {
    fulfillmentConstraintRule {
      id
      deliveryMethodTypes
      function {
        id
        title
        handle
        apiType
        description
        appKey
        app { __typename id title handle apiKey }
      }
      metafields(first: 100) {
        nodes { id namespace key type value compareDigest ownerType createdAt updatedAt }
      }
    }
    userErrors { code field message }
  }
}
`;

const downstreamReadDocument = `query FulfillmentConstraintRuleUpdatedRead {
  fulfillmentConstraintRules {
    id
    deliveryMethodTypes
    function {
      id
      title
      handle
      apiType
      description
      appKey
      app { __typename id title handle apiKey }
    }
    metafields(first: 100) {
      nodes { id namespace key type value compareDigest ownerType createdAt updatedAt }
    }
  }
}
`;

const cleanupDocument = `mutation FulfillmentConstraintRuleCaptureCleanup($id: ID!) {
  cleanup: fulfillmentConstraintRuleDelete(id: $id) {
    success
    userErrors { code field message }
  }
}
`;

const schemaSnapshot = await capture(schemaIntrospectionDocument);
const functionCatalog = await capture(functionCatalogDocument);
assertNoTopLevelErrors(schemaSnapshot, 'fulfillmentConstraintRule schema snapshot');
assertNoTopLevelErrors(functionCatalog, 'fulfillment constraint Function catalog');
const fulfillmentConstraintFunction = readArray(
  readPath(functionCatalog.response.payload, ['data', 'fulfillmentConstraintFunctions', 'nodes']),
)
  .map(readRecord)
  .find((node) => node['handle'] === 'conformance-fulfillment-constraint');
if (!fulfillmentConstraintFunction) {
  throw new Error('Missing released Function handle conformance-fulfillment-constraint');
}
assertJsonEqual(
  fulfillmentConstraintFunction['apiType'],
  'fulfillment_constraints',
  'released fulfillment constraint Function apiType',
);

const preCaptureCatalog = await capture(fulfillmentConstraintRulesHydrateDocument);
const preCaptureCleanup: Capture[] = [];
for (const rule of readRules(preCaptureCatalog)) {
  if (readRecord(rule['function'])['handle'] !== 'conformance-fulfillment-constraint') {
    continue;
  }
  const staleId = requiredString(rule['id'], 'stale fulfillmentConstraintRule.id');
  const staleCleanup = await capture(cleanupDocument, { id: staleId });
  assertNoUserErrors(staleCleanup, 'cleanup');
  preCaptureCleanup.push(staleCleanup);
}

const errorShape = await capture(errorShapeDocument);
const readEmpty = await capture(readEmptyDocument);
const unknownFunction = await capture(unknownFunctionDocument);
assertNoTopLevelErrors(errorShape, 'fulfillmentConstraintRule error shape');
assertNoTopLevelErrors(readEmpty, 'fulfillmentConstraintRules empty read');
assertNoTopLevelErrors(unknownFunction, 'fulfillmentConstraintRule unknown Function errors');
assertPayloadUserError(errorShape, 'missing', {
  code: 'MISSING_FUNCTION_IDENTIFIER',
  field: ['functionHandle'],
  message: 'Either function_id or function_handle must be provided.',
});
assertPayloadUserError(errorShape, 'multiple', {
  code: 'MULTIPLE_FUNCTION_IDENTIFIERS',
  field: ['functionHandle'],
  message: 'Only one of function_id or function_handle can be provided, not both.',
});
assertPayloadUserError(errorShape, 'emptyDelivery', {
  code: 'INPUT_INVALID',
  field: ['deliveryMethodTypes'],
  message: 'Delivery method types cannot be empty.',
});
assertPayloadUserError(errorShape, 'updateUnknown', {
  code: 'NOT_FOUND',
  field: ['id'],
  message: 'Could not find FulfillmentConstraintRule with id: gid://shopify/FulfillmentConstraintRule/999999999999',
});
assertDeleteUnknown(errorShape);
assertJsonEqual(readPath(readEmpty.response.payload, ['data', 'fulfillmentConstraintRules']), [], 'empty rule catalog');

let createdRuleId: string | null = null;
let cleanup: Capture | null = null;
try {
  const setupCreate = await capture(createExistingRuleDocument, {
    functionHandle: 'conformance-fulfillment-constraint',
    deliveryMethodTypes: ['SHIPPING'],
  });
  const createdRule = readRule(setupCreate, 'setupCreate');
  createdRuleId = requiredString(createdRule['id'], 'setupCreate fulfillmentConstraintRule.id');
  assertJsonEqual(createdRule['deliveryMethodTypes'], ['SHIPPING'], 'created deliveryMethodTypes');
  assertJsonEqual(createdRule['function'], fulfillmentConstraintFunction, 'created Function identity');
  if (readArray(readPath(createdRule, ['metafields', 'nodes'])).length !== 1) {
    throw new Error(`Expected one setup metafield: ${JSON.stringify(createdRule, null, 2)}`);
  }

  const hydrationBaseline = await capture(fulfillmentConstraintRulesHydrateDocument);
  const hydratedRule = readRules(hydrationBaseline).find((rule) => rule['id'] === createdRuleId);
  if (!hydratedRule) {
    throw new Error(`Created rule ${createdRuleId} was absent from the hydration baseline`);
  }
  assertJsonEqual(hydratedRule, createdRule, 'hydrated setup rule');

  const updateExistingRule = await capture(updateExistingRuleDocument, {
    id: createdRuleId,
    deliveryMethodTypes: ['PICK_UP'],
  });
  const updatedRule = readRule(updateExistingRule, 'successfulUpdate');
  assertJsonEqual(updatedRule['id'], createdRuleId, 'updated rule id');
  assertJsonEqual(updatedRule['deliveryMethodTypes'], ['PICK_UP'], 'updated deliveryMethodTypes');
  assertJsonEqual(updatedRule['function'], createdRule['function'], 'preserved Function identity');
  assertJsonEqual(updatedRule['metafields'], createdRule['metafields'], 'preserved metafields');

  const downstreamRead = await capture(downstreamReadDocument);
  const downstreamRule = readRules(downstreamRead).find((rule) => rule['id'] === createdRuleId);
  if (!downstreamRule) {
    throw new Error(`Updated rule ${createdRuleId} was absent from the downstream read`);
  }
  assertJsonEqual(downstreamRule, updatedRule, 'updated downstream rule');

  cleanup = await capture(cleanupDocument, { id: createdRuleId });
  assertNoUserErrors(cleanup, 'cleanup');
  createdRuleId = null;

  const capturePayload = {
    scenarioId: 'functions-fulfillment-constraint-rule-errors',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify',
    storeDomain,
    apiVersion,
    summary:
      'Live fulfillment constraint rule validation errors plus an existing-rule update that preserves Function identity and metafields and is visible in the downstream rule catalog.',
    conformanceApp: {
      functionHandle: 'conformance-fulfillment-constraint',
      function: fulfillmentConstraintFunction,
      requiredScopes: ['read_fulfillment_constraint_rules', 'write_fulfillment_constraint_rules'],
    },
    schemaFindings: {
      singularReadRoot:
        'Admin GraphQL 2026-04 exposes fulfillmentConstraintRules only; no fulfillmentConstraintRule(id:) query root was present in live introspection.',
    },
    schemaSnapshot,
    functionCatalog,
    preCaptureCatalog,
    preCaptureCleanup,
    errorShape,
    readEmpty,
    unknownFunction,
    setupCreate,
    hydrationBaseline,
    updateExistingRule,
    downstreamRead,
    cleanup,
    upstreamCalls: [
      {
        operationName: 'FunctionFulfillmentConstraintRulesHydrate',
        variables: {},
        query: hydrationBaseline.query,
        response: {
          status: hydrationBaseline.response.status,
          body: hydrationBaseline.response.payload,
        },
      },
    ],
    notes: {
      lifecycle:
        'The registered capture removes stale rules for the released conformance Function, creates one disposable rule with a metafield, records the exact pre-update catalog used for proxy hydration, updates only deliveryMethodTypes, verifies a downstream read, and deletes the rule.',
      ownership:
        'The captured rule retains the released Function id, handle, API type, app key, and app identity through the update; unknown rule ids return NOT_FOUND without fabricating a rule.',
    },
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(capturePayload, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  if (createdRuleId) {
    const cleanupAfterFailure = await capture(cleanupDocument, { id: createdRuleId });
    if (
      cleanupAfterFailure.response.status < 200 ||
      cleanupAfterFailure.response.status >= 300 ||
      readRecord(cleanupAfterFailure.response.payload)['errors']
    ) {
      console.error(`Cleanup failed for ${createdRuleId}: ${JSON.stringify(cleanupAfterFailure, null, 2)}`);
    }
  }
}
