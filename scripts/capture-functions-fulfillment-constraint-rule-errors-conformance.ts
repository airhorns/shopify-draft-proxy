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
const missingRuleId = 'gid://shopify/FulfillmentConstraintRule/999999999999';
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
  if (
    branch['success'] !== false ||
    userErrors.length !== 1 ||
    actual['code'] !== 'NOT_FOUND' ||
    JSON.stringify(actual['field']) !== JSON.stringify(['id']) ||
    actual['message'] !== `Could not find FulfillmentConstraintRule with id: ${missingRuleId}`
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

const ruleHydrateDocument = `query FunctionFulfillmentConstraintRuleHydrateById($id: ID!) {
  node(id: $id) {
    ... on FulfillmentConstraintRule {
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
  fulfillmentConstraintFunctions: shopifyFunctions(apiType: "FULFILLMENT_CONSTRAINT_RULE", first: 10) {
    nodes {
      id
      title
      handle
      apiType
    }
  }
}
`;

const schemaSnapshot = await capture(schemaIntrospectionDocument);
const functionCatalog = await capture(functionCatalogDocument);
const missingHydrate = await capture(ruleHydrateDocument, { id: missingRuleId });
const errorShape = await capture(errorShapeDocument);
const readEmpty = await capture(readEmptyDocument);
const unknownFunction = await capture(unknownFunctionDocument);

assertNoTopLevelErrors(errorShape, 'fulfillmentConstraintRule error shape');
assertNoTopLevelErrors(readEmpty, 'fulfillmentConstraintRules empty read');
assertNoTopLevelErrors(missingHydrate, 'unknown fulfillmentConstraintRule Node hydrate');
if (readPath(missingHydrate.response.payload, ['data', 'node']) !== null) {
  throw new Error(`Expected unknown rule hydrate to return null: ${JSON.stringify(missingHydrate.response, null, 2)}`);
}
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
assertDeleteUnknown(errorShape);

const capturePayload = {
  scenarioId: 'functions-fulfillment-constraint-rule-errors',
  capturedAt: new Date().toISOString(),
  source: 'live-shopify',
  storeDomain,
  apiVersion,
  summary:
    'fulfillmentConstraintRuleCreate deterministic userErrors, fulfillmentConstraintRuleDelete unknown-id shape, and empty fulfillmentConstraintRules read.',
  setupBlocker:
    'The conformance app currently has no released FULFILLMENT_CONSTRAINT_RULE Function, so live success-path, wrong-API-type, and read-after-created-rule capture require releasing a purchase.fulfillment-constraint-rule.run or cart.fulfillment-constraints.generate.run Function in the installed conformance app with read/write fulfillment constraint rule scopes.',
  schemaFindings: {
    singularReadRoot:
      'Admin GraphQL 2026-04 exposes fulfillmentConstraintRules only; no fulfillmentConstraintRule(id:) query root was present in live introspection.',
  },
  schemaSnapshot,
  functionCatalog,
  missingHydrate,
  errorShape,
  readEmpty,
  unknownFunction,
  upstreamCalls: [
    {
      operationName: 'FunctionFulfillmentConstraintRuleHydrateById',
      variables: missingHydrate.variables,
      query: missingHydrate.query,
      response: { status: missingHydrate.response.status, body: missingHydrate.response.payload },
    },
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(capturePayload, null, 2)}\n`, 'utf8');

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
