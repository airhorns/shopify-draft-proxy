/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  response: ConformanceGraphqlResult['payload'];
  status: number;
};

const missingValidationId = 'gid://shopify/Validation/999999999999';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: process.env.SHOPIFY_CONFORMANCE_API_VERSION ?? '2026-04' },
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-validation-update-rebind-variable.json');
const query = await readFile(
  path.join('config', 'parity-requests', 'functions', 'functions-validation-update-rebind-variable.graphql'),
  'utf8',
);
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function capture(variables: Record<string, unknown>): Promise<Capture> {
  const response = await runGraphqlRequest(query, variables);
  return {
    request: { query, variables },
    response: response.payload,
    status: response.status,
  };
}

function assertInvalidVariable(captureResult: Capture, fieldName: string): void {
  const errors = Array.isArray(captureResult.response.errors) ? captureResult.response.errors : [];
  const first = errors[0] as Record<string, unknown> | undefined;
  const extensions =
    first?.['extensions'] && typeof first['extensions'] === 'object'
      ? (first['extensions'] as Record<string, unknown>)
      : {};
  const problems = Array.isArray(extensions['problems']) ? extensions['problems'] : [];
  const firstProblem = problems[0] as Record<string, unknown> | undefined;
  if (
    captureResult.status !== 200 ||
    errors.length !== 1 ||
    typeof first?.['message'] !== 'string' ||
    !first['message'].includes(`Field is not defined on ValidationUpdateInput`) ||
    extensions['code'] !== 'INVALID_VARIABLE' ||
    JSON.stringify(firstProblem?.['path']) !== JSON.stringify([fieldName]) ||
    firstProblem?.['explanation'] !== 'Field is not defined on ValidationUpdateInput' ||
    'data' in captureResult.response
  ) {
    throw new Error(`Unexpected validationUpdate ${fieldName} variable coercion shape: ${JSON.stringify(captureResult, null, 2)}`);
  }
}

const functionId = await capture({
  id: missingValidationId,
  validation: {
    functionId: 'gid://shopify/ShopifyFunction/validation-beta',
  },
});
assertInvalidVariable(functionId, 'functionId');

const functionHandle = await capture({
  id: missingValidationId,
  validation: {
    functionHandle: 'validation-beta',
  },
});
assertInvalidVariable(functionHandle, 'functionHandle');

const fixture = {
  scenarioId: 'functions-validation-update-rebind-variable',
  capturedAt: new Date().toISOString(),
  source: 'live-shopify',
  storeDomain,
  apiVersion,
  summary:
    'Live validationUpdate variable-input evidence that functionId/functionHandle rebind keys are rejected by GraphQL variable coercion before resolver execution.',
  functionId,
  functionHandle,
  upstreamCalls: [],
  notes: {
    lifecycle:
      'The request uses an arbitrary missing validation id because Shopify rejects the unknown ValidationUpdateInput fields during variable coercion before resolving id or executing validationUpdate.',
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
