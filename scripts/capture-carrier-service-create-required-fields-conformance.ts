/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedRequest = {
  documentPath: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'shipping-fulfillments');
const outputPath = path.join(outputDir, 'carrier-service-create-required-fields.json');
const documentPath = path.join(
  'config',
  'parity-requests',
  'shipping-fulfillments',
  'carrier-service-create-required-fields.graphql',
);

async function readDocument(): Promise<string> {
  return readFile(path.join(process.cwd(), documentPath), 'utf8');
}

async function capture(document: string, variables: JsonRecord): Promise<CapturedRequest> {
  return {
    documentPath,
    variables,
    response: await runGraphqlRequest(document, variables),
  };
}

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function assertInvalidVariable(captureResult: CapturedRequest, expectedProblemPaths: string[][]): void {
  const errors = captureResult.response.payload.errors;
  if (!Array.isArray(errors) || errors.length !== 1) {
    throw new Error(
      `Expected exactly one INVALID_VARIABLE error, got ${JSON.stringify(captureResult.response.payload)}`,
    );
  }
  const error = readRecord(errors[0]);
  const extensions = readRecord(error?.['extensions']);
  if (extensions?.['code'] !== 'INVALID_VARIABLE') {
    throw new Error(`Expected INVALID_VARIABLE, got ${JSON.stringify(error)}`);
  }
  const problems = extensions['problems'];
  if (!Array.isArray(problems)) {
    throw new Error(`Expected problems array, got ${JSON.stringify(extensions)}`);
  }
  const actualPaths = problems.map((problem) => readRecord(problem)?.['path']);
  const expectedJson = JSON.stringify(expectedProblemPaths);
  const actualJson = JSON.stringify(actualPaths);
  if (actualJson !== expectedJson) {
    throw new Error(`Expected problem paths ${expectedJson}, got ${actualJson}`);
  }
}

const document = await readDocument();
const suffix = Date.now().toString(36);

const missingActive = await capture(document, {
  input: {
    name: `Hermes Missing Active ${suffix}`,
    callbackUrl: 'https://mock.shop/carrier-service-rates',
    supportsServiceDiscovery: false,
  },
});
assertInvalidVariable(missingActive, [['active']]);

const missingSupportsServiceDiscovery = await capture(document, {
  input: {
    name: `Hermes Missing Supports ${suffix}`,
    callbackUrl: 'https://mock.shop/carrier-service-rates',
    active: false,
  },
});
assertInvalidVariable(missingSupportsServiceDiscovery, [['supportsServiceDiscovery']]);

const missingBoth = await capture(document, {
  input: {
    name: `Hermes Missing Both ${suffix}`,
    callbackUrl: 'https://mock.shop/carrier-service-rates',
  },
});
assertInvalidVariable(missingBoth, [['supportsServiceDiscovery'], ['active']]);

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Captures DeliveryCarrierServiceCreateInput required-field validation against live Shopify Admin GraphQL.',
        'Omitted active and supportsServiceDiscovery inputs fail at GraphQL variable coercion before the carrierServiceCreate resolver runs.',
        'The capture is validation-only and creates no live carrier service objects.',
      ],
      captures: {
        missingActive,
        missingSupportsServiceDiscovery,
        missingBoth,
      },
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
