/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlCapture = {
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: ConformanceGraphqlResult['payload'];
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-validation-option-names.json');
const paritySpecPath = path.join(
  'config',
  'parity-specs',
  'metafields',
  'metafield-definition-validation-option-names.json',
);
const requestPath = path.join(
  'config',
  'parity-requests',
  'metafields',
  'metafield-definition-invalid-validation-options.graphql',
);

const requestDocument = await readFile(requestPath, 'utf8');
const runId = Date.now().toString(36);
const variables = {
  namespace: `validation_options_${runId}`,
};

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function capture(query: string, requestVariables: Record<string, unknown>): Promise<GraphqlCapture> {
  const result = await runGraphqlRaw(query, requestVariables);
  return {
    request: { query, variables: requestVariables },
    status: result.status,
    response: result.payload,
  };
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function collectCreatedDefinitionIds(value: unknown, ids = new Set<string>()): Set<string> {
  const object = readObject(value);
  if (object) {
    const createdDefinition = readObject(object['createdDefinition']);
    const id = createdDefinition?.['id'];
    if (typeof id === 'string') ids.add(id);
    for (const entry of Object.values(object)) collectCreatedDefinitionIds(entry, ids);
  } else if (Array.isArray(value)) {
    for (const entry of value) collectCreatedDefinitionIds(entry, ids);
  }
  return ids;
}

async function cleanupDefinition(id: string): Promise<GraphqlCapture> {
  return capture(
    `#graphql
      mutation CleanupMetafieldDefinitionValidationOptionNames($id: ID!) {
        metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
          deletedDefinitionId
          userErrors {
            field
            message
            code
          }
        }
      }
    `,
    { id },
  );
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

const primary = await capture(requestDocument, variables);
const cleanup: GraphqlCapture[] = [];
for (const id of collectCreatedDefinitionIds(primary.response)) {
  cleanup.push(await cleanupDefinition(id));
}

await writeJson(outputPath, {
  scenarioId: 'metafield-definition-validation-option-names',
  storeDomain,
  apiVersion,
  capturedAt: new Date().toISOString(),
  variables,
  primary,
  cleanup,
  upstreamCalls: [],
  notes:
    'Live Shopify Admin API metafieldDefinitionCreate/metafieldDefinitionUpdate validation-option capture. It records unsupported option names on single_line_text_field, invalid number_decimal min coercion, and a valid number_decimal min/max control.',
});

await writeJson(paritySpecPath, {
  scenarioId: 'metafield-definition-validation-option-names',
  operationNames: ['metafieldDefinitionCreate', 'metafieldDefinitionUpdate'],
  scenarioStatus: 'captured',
  assertionKinds: ['user-errors-parity', 'input-validation', 'payload-shape', 'no-local-staging-on-validation-error'],
  liveCaptureFiles: [outputPath],
  runtimeTestFiles: ['tests/graphql_routes/metafield_definitions.rs'],
  proxyRequest: {
    documentPath: requestPath,
    variablesCapturePath: '$.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'create-unsupported-option-not-real',
        capturePath: '$.primary.response.data.unsupportedNotRealOption',
        proxyPath: '$.data.unsupportedNotRealOption',
      },
      {
        name: 'create-unsupported-option-pattern',
        capturePath: '$.primary.response.data.unsupportedPattern',
        proxyPath: '$.data.unsupportedPattern',
      },
      {
        name: 'create-decimal-invalid-min',
        capturePath: '$.primary.response.data.decimalBadMin',
        proxyPath: '$.data.decimalBadMin',
      },
      {
        name: 'create-decimal-valid-range',
        capturePath: '$.primary.response.data.decimalValidRange',
        proxyPath: '$.data.decimalValidRange',
        expectedDifferences: [
          {
            path: '$.createdDefinition.id',
            matcher: 'shopify-gid:MetafieldDefinition',
            reason:
              'The proxy stages a deterministic synthetic definition ID while Shopify returned the live-store definition ID.',
          },
        ],
      },
      {
        name: 'update-unsupported-option-name',
        capturePath: '$.primary.response.data.updateUnsupportedOption',
        proxyPath: '$.data.updateUnsupportedOption',
      },
      {
        name: 'update-decimal-invalid-min',
        capturePath: '$.primary.response.data.updateDecimalBadMin',
        proxyPath: '$.data.updateDecimalBadMin',
      },
    ],
  },
  notes:
    'Strict parity for metafield definition validations[] option-name support and numeric option coercion. Setup aliases create disposable definitions through the same public mutation surface; strict targets assert the invalid create/update payloads and a valid decimal range control.',
});

console.log(
  JSON.stringify(
    {
      outputPath,
      paritySpecPath,
      namespace: variables.namespace,
      apiVersion,
      status: primary.status,
      cleanupCount: cleanup.length,
    },
    null,
    2,
  ),
);
