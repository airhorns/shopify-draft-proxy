/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type Capture = { query: string; variables: JsonRecord; response: ConformanceGraphqlResult };

const functionHandle = 'conformance-fulfillment-constraint';
const missingRuleId = 'gid://shopify/FulfillmentConstraintRule/999999999999';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-fulfillment-constraint-rule-hydration.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function capture(query: string, variables: JsonRecord = {}): Promise<Capture> {
  return { query, variables, response: await runGraphqlRequest(query, variables) };
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
  for (const segment of segments) current = readRecord(current)[segment];
  return current;
}

function assertNoTopLevelErrors(result: Capture, context: string): void {
  if (result.response.status < 200 || result.response.status >= 300 || readRecord(result.response.payload)['errors']) {
    throw new Error(`${context} failed: ${JSON.stringify(result.response, null, 2)}`);
  }
}

function assertNoUserErrors(result: Capture, root: string, context: string): void {
  assertNoTopLevelErrors(result, context);
  const errors = readArray(readPath(result.response.payload, ['data', root, 'userErrors']));
  if (errors.length > 0) throw new Error(`${context} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
}

function assertNotFound(result: Capture, root: string): void {
  assertNoTopLevelErrors(result, root);
  const payload = readRecord(readPath(result.response.payload, ['data', root]));
  const errors = readArray(payload['userErrors']);
  const actual = readRecord(errors[0]);
  if (
    payload['fulfillmentConstraintRule'] !== null ||
    errors.length !== 1 ||
    actual['code'] !== 'NOT_FOUND' ||
    actual['message'] !== `Could not find FulfillmentConstraintRule with id: ${missingRuleId}` ||
    JSON.stringify(actual['field']) !== JSON.stringify(['id'])
  ) {
    throw new Error(`${root} NOT_FOUND mismatch: ${JSON.stringify(payload, null, 2)}`);
  }
}

const functionCatalogDocument = `query FulfillmentConstraintHydrationFunctionCatalog {
  shopifyFunctions(first: 100) {
    nodes { id handle apiType }
  }
}
`;

const ruleInventoryDocument = `query FulfillmentConstraintHydrationInventory {
  fulfillmentConstraintRules { id }
}
`;

const ruleCreateDocument = `mutation FulfillmentConstraintHydrationSetup($functionHandle: String!) {
  fulfillmentConstraintRuleCreate(functionHandle: $functionHandle, deliveryMethodTypes: [SHIPPING]) {
    fulfillmentConstraintRule {
      id
      deliveryMethodTypes
      function { id handle apiType }
    }
    userErrors { field message code }
  }
}
`;

const ruleUpdateDocument = `mutation FulfillmentConstraintHydratedUpdate($id: ID!) {
  fulfillmentConstraintRuleUpdate(id: $id, deliveryMethodTypes: [LOCAL]) {
    fulfillmentConstraintRule {
      id
      deliveryMethodTypes
      function { id handle apiType }
    }
    userErrors { field message code }
  }
}
`;

const ruleDeleteDocument = `mutation FulfillmentConstraintHydratedDelete($id: ID!) {
  fulfillmentConstraintRuleDelete(id: $id) {
    success
    userErrors { field message code }
  }
}
`;

const ruleUnknownUpdateDocument = `mutation FulfillmentConstraintHydratedUnknownUpdate($id: ID!) {
  fulfillmentConstraintRuleUpdate(id: $id, deliveryMethodTypes: [LOCAL]) {
    fulfillmentConstraintRule { id }
    userErrors { field message code }
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

const functionCatalog = await capture(functionCatalogDocument);
assertNoTopLevelErrors(functionCatalog, 'Fulfillment constraint Function catalog');
const functionNode = readArray(readPath(functionCatalog.response.payload, ['data', 'shopifyFunctions', 'nodes']))
  .map(readRecord)
  .find((node) => node['handle'] === functionHandle);
if (!functionNode) throw new Error(`Missing released Function ${functionHandle}`);

const cleanupBefore: Capture[] = [];
const inventory = await capture(ruleInventoryDocument);
assertNoTopLevelErrors(inventory, 'Fulfillment constraint inventory');
for (const rule of readArray(readPath(inventory.response.payload, ['data', 'fulfillmentConstraintRules']))) {
  const id = readString(readRecord(rule)['id']);
  if (id) cleanupBefore.push(await capture(ruleDeleteDocument, { id }));
}

let ruleId: string | null = null;
let fixture: JsonRecord | null = null;
const cleanupAfter: Capture[] = [];
try {
  const create = await capture(ruleCreateDocument, { functionHandle });
  assertNoUserErrors(create, 'fulfillmentConstraintRuleCreate', 'Rule setup');
  ruleId = readString(
    readPath(create.response.payload, ['data', 'fulfillmentConstraintRuleCreate', 'fulfillmentConstraintRule', 'id']),
  );
  if (!ruleId) throw new Error('Rule setup returned no id');

  const hydrate = await capture(ruleHydrateDocument, { id: ruleId });
  assertNoTopLevelErrors(hydrate, 'Rule ID hydrate');
  const hydratedRule = readRecord(readPath(hydrate.response.payload, ['data', 'node']));
  if (hydratedRule['id'] !== ruleId) {
    throw new Error(`Expected hydrated rule ${ruleId}: ${JSON.stringify(hydratedRule, null, 2)}`);
  }

  const update = await capture(ruleUpdateDocument, { id: ruleId });
  assertNoUserErrors(update, 'fulfillmentConstraintRuleUpdate', 'Hydrated rule update');

  const deleteRule = await capture(ruleDeleteDocument, { id: ruleId });
  assertNoUserErrors(deleteRule, 'fulfillmentConstraintRuleDelete', 'Hydrated rule delete');
  ruleId = null;

  const missingHydrate = await capture(ruleHydrateDocument, { id: missingRuleId });
  assertNoTopLevelErrors(missingHydrate, 'Unknown rule ID hydrate');
  if (readPath(missingHydrate.response.payload, ['data', 'node']) !== null) {
    throw new Error(
      `Expected unknown rule hydrate to return null: ${JSON.stringify(missingHydrate.response, null, 2)}`,
    );
  }

  const unknownUpdate = await capture(ruleUnknownUpdateDocument, { id: missingRuleId });
  assertNotFound(unknownUpdate, 'fulfillmentConstraintRuleUpdate');

  fixture = {
    scenarioId: 'functions-fulfillment-constraint-rule-hydration',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify',
    storeDomain,
    apiVersion,
    summary:
      'Live direct update/delete of a real fulfillment constraint rule plus the exact unknown-id update payload.',
    functionNode,
    inventory,
    cleanupBefore,
    create,
    hydrate,
    update,
    deleteRule,
    missingHydrate,
    unknownUpdate,
    cleanupAfter,
    upstreamCalls: [
      {
        operationName: 'FunctionFulfillmentConstraintRuleHydrateById',
        variables: hydrate.variables,
        query: hydrate.query,
        response: { status: hydrate.response.status, body: hydrate.response.payload },
      },
      {
        operationName: 'FunctionFulfillmentConstraintRuleHydrateById',
        variables: missingHydrate.variables,
        query: missingHydrate.query,
        response: { status: missingHydrate.response.status, body: missingHydrate.response.payload },
      },
    ],
    notes: {
      hydration:
        'Proxy replay receives only the update/delete caller mutation and resolves the pre-existing target through the exact read-only Node cassette.',
      cleanup:
        'The disposable live rule is deleted during the recorded lifecycle and the finally block retries cleanup after failures.',
    },
  };
} finally {
  if (ruleId) cleanupAfter.push(await capture(ruleDeleteDocument, { id: ruleId }));
  for (const cleanup of cleanupAfter) assertNoTopLevelErrors(cleanup, 'Rule cleanup');
}

if (!fixture) throw new Error('Fulfillment constraint capture did not produce a fixture');
await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
