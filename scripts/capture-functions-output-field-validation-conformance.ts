/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const validationFunctionHandle = 'conformance-validation';
const missingFulfillmentConstraintRuleId = 'gid://shopify/FulfillmentConstraintRule/999999999999';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-output-field-validation.json');
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

type JsonRecord = Record<string, unknown>;

async function capture(query: string, variables: Record<string, unknown> = {}): Promise<Capture> {
  return {
    query,
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current: unknown = value;
  for (const segment of pathSegments) {
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function assertNoTopLevelErrors(captureResult: Capture, context: string): void {
  if (
    captureResult.response.status < 200 ||
    captureResult.response.status >= 300 ||
    readRecord(captureResult.response.payload)?.['errors']
  ) {
    throw new Error(`${context} failed: ${JSON.stringify(captureResult.response, null, 2)}`);
  }
}

function assertEmptyUserErrors(value: unknown, context: string): void {
  const userErrors = readArray(readRecord(value)?.['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function typeFieldNames(fieldSet: Capture, typeKey: string): string[] {
  return readArray(readPath(fieldSet.response.payload, ['data', typeKey, 'fields']))
    .map((field) => readRecord(field)?.['name'])
    .filter((name): name is string => typeof name === 'string')
    .sort();
}

function assertTypeFields(fieldSet: Capture, typeKey: string, expectedFields: string[]): void {
  const actualFields = typeFieldNames(fieldSet, typeKey);
  const expected = [...expectedFields].sort();
  if (JSON.stringify(actualFields) !== JSON.stringify(expected)) {
    throw new Error(
      `${typeKey} fields mismatch: ${JSON.stringify(
        {
          expected,
          actual: actualFields,
        },
        null,
        2,
      )}`,
    );
  }
}

function assertUndefinedFieldErrors(captureResult: Capture, typeName: string, fieldNames: string[]): void {
  const errors = readArray(readRecord(captureResult.response.payload)?.['errors']).map(readRecord);
  for (const fieldName of fieldNames) {
    const found = errors.some(
      (error) =>
        error?.['message'] === `Field '${fieldName}' doesn't exist on type '${typeName}'` &&
        readRecord(error['extensions'])?.['typeName'] === typeName &&
        readRecord(error['extensions'])?.['fieldName'] === fieldName,
    );
    if (!found) {
      throw new Error(
        `Missing undefinedField error for ${typeName}.${fieldName}: ${JSON.stringify(
          captureResult.response.payload,
          null,
          2,
        )}`,
      );
    }
  }
}

const fieldSetDocument = `query FunctionOutputFieldSets {
  validationType: __type(name: "Validation") {
    fields {
      name
    }
  }
  fulfillmentConstraintRuleType: __type(name: "FulfillmentConstraintRule") {
    fields {
      name
    }
  }
}
`;

const validationCreateDocument = `mutation FunctionOutputFieldValidationCreate($validation: ValidationCreateInput!) {
  validationCreate(validation: $validation) {
    validation {
      id
      title
      enabled
      blockOnFailure
      shopifyFunction {
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

const validationInvalidReadDocument = `query FunctionOutputFieldValidationInvalidRead($id: ID!) {
  validation(id: $id) {
    functionId
    functionHandle
    createdAt
    updatedAt
    enable
  }
}
`;

const validationsInvalidReadDocument = `query FunctionOutputFieldValidationsInvalidRead {
  validations(first: 5) {
    nodes {
      functionId
      functionHandle
      createdAt
      updatedAt
      enable
    }
  }
}
`;

const validationValidReadDocument = `query FunctionOutputFieldValidationValidRead($id: ID!) {
  validation(id: $id) {
    id
    title
    enabled
    blockOnFailure
    shopifyFunction {
      id
      handle
      apiType
    }
  }
}
`;

const validationNodeInvalidReadDocument = `query FunctionOutputFieldValidationNodeInvalidRead($id: ID!) {
  node(id: $id) {
    ... on Validation {
      functionId
      functionHandle
      createdAt
      updatedAt
      enable
    }
  }
}
`;

const validationNodeValidReadDocument = `query FunctionOutputFieldValidationNodeValidRead($id: ID!) {
  node(id: $id) {
    ... on Validation {
      id
      title
      enabled
      blockOnFailure
      shopifyFunction {
        id
        handle
        apiType
      }
    }
  }
}
`;

const fulfillmentConstraintRuleInvalidReadDocument = `query FunctionOutputFieldFulfillmentConstraintRuleInvalidRead {
  fulfillmentConstraintRules {
    functionId
    functionHandle
    shopifyFunction {
      id
    }
  }
}
`;

const fulfillmentConstraintRuleNodeInvalidReadDocument = `query FunctionOutputFieldFulfillmentConstraintRuleNodeInvalidRead($id: ID!) {
  node(id: $id) {
    ... on FulfillmentConstraintRule {
      functionId
      functionHandle
      shopifyFunction {
        id
      }
    }
  }
}
`;

const fulfillmentConstraintRulesValidEmptyReadDocument = `query FunctionOutputFieldFulfillmentConstraintRulesValidEmptyRead {
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

const validationDeleteDocument = `mutation FunctionOutputFieldValidationCleanup($id: ID!) {
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

const fieldSet = await capture(fieldSetDocument);
assertNoTopLevelErrors(fieldSet, 'Function output field introspection');
assertTypeFields(fieldSet, 'validationType', [
  'blockOnFailure',
  'enabled',
  'errorHistory',
  'id',
  'metafield',
  'metafields',
  'shopifyFunction',
  'title',
]);
assertTypeFields(fieldSet, 'fulfillmentConstraintRuleType', [
  'deliveryMethodTypes',
  'function',
  'id',
  'metafield',
  'metafields',
]);

let createdValidationId: string | null = null;
let cleanup: Capture | null = null;

try {
  const validationCreate = await capture(validationCreateDocument, {
    validation: {
      functionHandle: validationFunctionHandle,
      title: 'Function output field validation',
      enable: true,
      blockOnFailure: true,
    },
  });
  assertNoTopLevelErrors(validationCreate, 'validationCreate setup');
  const createPayload = readPath(validationCreate.response.payload, ['data', 'validationCreate']);
  assertEmptyUserErrors(createPayload, 'validationCreate setup');
  const createdValidation = readRecord(readRecord(createPayload)?.['validation']);
  createdValidationId = typeof createdValidation?.['id'] === 'string' ? createdValidation['id'] : null;
  if (!createdValidationId) {
    throw new Error(`validationCreate did not return an id: ${JSON.stringify(validationCreate.response, null, 2)}`);
  }

  const validationInvalidRead = await capture(validationInvalidReadDocument, { id: createdValidationId });
  assertUndefinedFieldErrors(validationInvalidRead, 'Validation', [
    'functionId',
    'functionHandle',
    'createdAt',
    'updatedAt',
    'enable',
  ]);

  const validationsInvalidRead = await capture(validationsInvalidReadDocument);
  assertUndefinedFieldErrors(validationsInvalidRead, 'Validation', [
    'functionId',
    'functionHandle',
    'createdAt',
    'updatedAt',
    'enable',
  ]);

  const validationValidRead = await capture(validationValidReadDocument, { id: createdValidationId });
  assertNoTopLevelErrors(validationValidRead, 'validation valid read');

  const validationNodeInvalidRead = await capture(validationNodeInvalidReadDocument, { id: createdValidationId });
  assertUndefinedFieldErrors(validationNodeInvalidRead, 'Validation', [
    'functionId',
    'functionHandle',
    'createdAt',
    'updatedAt',
    'enable',
  ]);

  const validationNodeValidRead = await capture(validationNodeValidReadDocument, { id: createdValidationId });
  assertNoTopLevelErrors(validationNodeValidRead, 'validation node valid read');

  const fulfillmentConstraintRuleInvalidRead = await capture(fulfillmentConstraintRuleInvalidReadDocument);
  assertUndefinedFieldErrors(fulfillmentConstraintRuleInvalidRead, 'FulfillmentConstraintRule', [
    'functionId',
    'functionHandle',
    'shopifyFunction',
  ]);

  const fulfillmentConstraintRuleNodeInvalidRead = await capture(fulfillmentConstraintRuleNodeInvalidReadDocument, {
    id: missingFulfillmentConstraintRuleId,
  });
  assertUndefinedFieldErrors(fulfillmentConstraintRuleNodeInvalidRead, 'FulfillmentConstraintRule', [
    'functionId',
    'functionHandle',
    'shopifyFunction',
  ]);

  const fulfillmentConstraintRulesValidEmptyRead = await capture(fulfillmentConstraintRulesValidEmptyReadDocument);
  assertNoTopLevelErrors(fulfillmentConstraintRulesValidEmptyRead, 'fulfillmentConstraintRules valid empty read');

  cleanup = await capture(validationDeleteDocument, { id: createdValidationId });
  assertNoTopLevelErrors(cleanup, 'validation cleanup');
  assertEmptyUserErrors(readPath(cleanup.response.payload, ['data', 'validationDelete']), 'validation cleanup');
  createdValidationId = null;

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: 'functions-output-field-validation',
        capturedAt: new Date().toISOString(),
        source: 'live-shopify',
        storeDomain,
        apiVersion,
        summary:
          'Validation and FulfillmentConstraintRule output field introspection plus undefined-field reads for fabricated local-only fields.',
        validationFunctionHandle,
        schemaFields: {
          validation: typeFieldNames(fieldSet, 'validationType'),
          fulfillmentConstraintRule: typeFieldNames(fieldSet, 'fulfillmentConstraintRuleType'),
        },
        fieldSet,
        validationCreate,
        validationInvalidRead,
        validationsInvalidRead,
        validationValidRead,
        validationNodeInvalidRead,
        validationNodeValidRead,
        fulfillmentConstraintRuleInvalidRead,
        fulfillmentConstraintRuleNodeInvalidRead,
        fulfillmentConstraintRulesValidEmptyRead,
        cleanup,
        upstreamCalls: [],
        notes: {
          validation:
            'Public Admin GraphQL 2026-04 does not expose Validation.functionId, functionHandle, createdAt, updatedAt, or enable; enabled is the public boolean output.',
          fulfillmentConstraintRule:
            'Public Admin GraphQL 2026-04 exposes FulfillmentConstraintRule.function, deliveryMethodTypes, id, and HasMetafields fields. Negative selection validation does not require a released fulfillment-constraint Function.',
        },
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote Functions output field validation fixture to ${outputPath}`);
} finally {
  if (createdValidationId) {
    const cleanupAfterFailure = await capture(validationDeleteDocument, { id: createdValidationId });
    if (cleanupAfterFailure.response.status >= 200 && cleanupAfterFailure.response.status < 300) {
      console.log(`Cleaned up validation ${createdValidationId} after capture failure.`);
    }
  }
}
