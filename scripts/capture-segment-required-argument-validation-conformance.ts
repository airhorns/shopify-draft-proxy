/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase = {
  name: string;
  documentPath: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  response: ConformanceGraphqlResult;
};

type CaptureDefinition = {
  name: string;
  documentPath: string;
  variables: Record<string, unknown>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segment-mutations-required-argument-validation.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const captures: CaptureDefinition[] = [
  {
    name: 'segmentCreateMissingNameAndQuery',
    documentPath: 'config/parity-requests/segments/segment-create-required-args-missing-both.graphql',
    variables: {},
  },
  {
    name: 'segmentCreateMissingName',
    documentPath: 'config/parity-requests/segments/segment-create-required-args-missing-name.graphql',
    variables: {},
  },
  {
    name: 'segmentCreateNullName',
    documentPath: 'config/parity-requests/segments/segment-create-required-args-null-name.graphql',
    variables: {},
  },
  {
    name: 'segmentCreateMissingQuery',
    documentPath: 'config/parity-requests/segments/segment-create-required-args-missing-query.graphql',
    variables: {},
  },
  {
    name: 'segmentCreateNullQuery',
    documentPath: 'config/parity-requests/segments/segment-create-required-args-null-query.graphql',
    variables: {},
  },
  {
    name: 'segmentUpdateMissingId',
    documentPath: 'config/parity-requests/segments/segment-update-required-id-missing.graphql',
    variables: {},
  },
  {
    name: 'segmentUpdateNullId',
    documentPath: 'config/parity-requests/segments/segment-update-required-id-null.graphql',
    variables: {},
  },
  {
    name: 'segmentDeleteMissingId',
    documentPath: 'config/parity-requests/segments/segment-delete-required-id-missing.graphql',
    variables: {},
  },
  {
    name: 'segmentDeleteNullId',
    documentPath: 'config/parity-requests/segments/segment-delete-required-id-null.graphql',
    variables: {},
  },
];

function assertRequiredArgumentErrors(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed with HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  if (!Array.isArray(result.payload.errors) || result.payload.errors.length === 0) {
    throw new Error(`${context} did not return top-level errors: ${JSON.stringify(result.payload, null, 2)}`);
  }
  if ('data' in result.payload) {
    throw new Error(`${context} unexpectedly returned data: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

const cases: CapturedCase[] = [];

for (const capture of captures) {
  const query = await readFile(capture.documentPath, 'utf8');
  const response = await runGraphqlRequest(query, capture.variables);
  assertRequiredArgumentErrors(response, capture.name);
  cases.push({
    name: capture.name,
    documentPath: capture.documentPath,
    request: {
      query,
      variables: capture.variables,
    },
    response,
  });
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cases,
      notes: [
        'Required top-level segment mutation arguments are rejected by GraphQL coercion before resolver execution.',
        'Omitted required arguments return missingRequiredArguments errors with no data payload.',
        'Literal null required arguments return argumentLiteralsIncompatible errors with no data payload.',
        'Validation-only capture; no live segment setup or cleanup is expected.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
