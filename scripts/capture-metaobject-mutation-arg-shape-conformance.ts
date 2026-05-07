/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedInteraction = {
  documentPath: string;
  operationName: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    payload: unknown;
  };
};

const scenarioId = 'metaobject-mutation-arg-shape';
const configEnv = {
  ...process.env,
  SHOPIFY_CONFORMANCE_API_VERSION: process.env['SHOPIFY_CONFORMANCE_METAOBJECTS_API_VERSION'] ?? '2026-04',
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: configEnv,
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const cases = {
  topLevelReset: {
    documentPath: path.join('config', 'parity-requests', 'metaobjects', 'metaobject-mutation-arg-shape-reset.graphql'),
    operationName: 'MetaobjectMutationArgShapeTopLevelReset',
    variables: {
      id: 'gid://shopify/MetaobjectDefinition/0',
    },
    expectedArgumentName: 'resetFieldOrder',
  },
  publicEnabledByShopify: {
    documentPath: path.join(
      'config',
      'parity-requests',
      'metaobjects',
      'metaobject-mutation-arg-shape-public-enable.graphql',
    ),
    operationName: 'MetaobjectMutationArgShapePublicEnable',
    variables: {},
    expectedArgumentName: 'enabledByShopify',
  },
};

function assertArgumentNotAccepted(
  label: string,
  result: ConformanceGraphqlResult,
  expectedArgumentName: string,
): void {
  const errors = Array.isArray(result.payload.errors) ? result.payload.errors : [];
  const first = errors[0];
  if (result.status !== 200 || errors.length !== 1 || typeof first !== 'object' || first === null) {
    throw new Error(`${label} did not return one top-level GraphQL error: ${JSON.stringify(result.payload)}`);
  }

  const extensions = (first as { extensions?: unknown }).extensions;
  const code =
    typeof extensions === 'object' && extensions !== null ? (extensions as { code?: unknown }).code : undefined;
  const argumentName =
    typeof extensions === 'object' && extensions !== null
      ? (extensions as { argumentName?: unknown }).argumentName
      : undefined;
  if (code !== 'argumentNotAccepted' || argumentName !== expectedArgumentName) {
    throw new Error(`${label} returned an unexpected validation error: ${JSON.stringify(result.payload)}`);
  }
}

async function capture(caseConfig: (typeof cases)[keyof typeof cases]): Promise<CapturedInteraction> {
  const query = await readFile(caseConfig.documentPath, 'utf8');
  const result = await runGraphqlRaw(query, caseConfig.variables);
  assertArgumentNotAccepted(caseConfig.operationName, result, caseConfig.expectedArgumentName);
  return {
    documentPath: caseConfig.documentPath,
    operationName: caseConfig.operationName,
    variables: caseConfig.variables,
    response: {
      status: result.status,
      payload: result.payload,
    },
  };
}

const captures = {
  topLevelReset: await capture(cases.topLevelReset),
  publicEnabledByShopify: await capture(cases.publicEnabledByShopify),
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId,
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      captures,
      evidence: {
        source: 'live-shopify',
        notes: [
          `Captured against ${storeDomain} using Admin GraphQL ${apiVersion}.`,
          'Both probes fail during GraphQL schema validation before resolver execution, so no Shopify records are created or mutated.',
          'The configured public conformance credential cannot reach Shopify internal Admin visibility for enabledByShopify success evidence.',
        ],
      },
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
