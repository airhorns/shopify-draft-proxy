/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildStorefrontRequestHeaders, getStoredStorefrontAccessToken } from './shopify-conformance-auth.mjs';

type ScenarioConfig = {
  scenarioId: string;
  operationName: string;
  documentPath: string;
  hydrateDocumentPath: string;
  hydrateOperationName: string;
  variables: Record<string, string>;
};

const scenarios: ScenarioConfig[] = [
  {
    scenarioId: 'storefront-first-slice-default',
    operationName: 'StorefrontFirstSliceDefault',
    documentPath: 'config/parity-requests/storefront/storefront-first-slice-default.graphql',
    hydrateDocumentPath: 'config/parity-requests/storefront/storefront-first-slice-hydrate.graphql',
    hydrateOperationName: 'StorefrontFirstSliceHydrate',
    variables: {},
  },
  {
    scenarioId: 'storefront-first-slice-context',
    operationName: 'StorefrontFirstSliceContext',
    documentPath: 'config/parity-requests/storefront/storefront-first-slice-context.graphql',
    hydrateDocumentPath: 'config/parity-requests/storefront/storefront-first-slice-hydrate-context.graphql',
    hydrateOperationName: 'StorefrontFirstSliceHydrateWithContext',
    variables: { country: 'US', language: 'EN' },
  },
];

const { storeDomain, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
  requireAdminOrigin: false,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'storefront');
const endpoint = `https://${storeDomain}/api/${apiVersion}/graphql.json`;
const pathName = `/api/${apiVersion}/graphql.json`;
const storedAuth = await getStoredStorefrontAccessToken();
if (storedAuth.shop && storedAuth.shop !== storeDomain) {
  throw new Error(
    `Stored Storefront token is for ${storedAuth.shop}, but SHOPIFY_CONFORMANCE_STORE_DOMAIN is ${storeDomain}. ` +
      'Run `corepack pnpm conformance:grant-storefront-token` for the target store.',
  );
}

const authHeaders = buildStorefrontRequestHeaders(storedAuth.storefront_access_token);
const redactedHeaders = Object.fromEntries(
  Object.keys(authHeaders).map((name) => [name, '<redacted:storefront-access-token>']),
);

async function recordStorefrontRequest(operationName: string, query: string, variables: Record<string, string>) {
  const result = await runStorefrontGraphqlRequest(
    {
      storeOrigin: `https://${storeDomain}`,
      apiVersion,
      storefrontAccessToken: storedAuth.storefront_access_token,
    },
    query,
    variables,
  );

  return {
    method: 'POST',
    apiSurface: 'storefront',
    apiVersion,
    path: pathName,
    endpoint,
    authMode: 'storefront-access-token',
    headers: redactedHeaders,
    operationName,
    query,
    variables,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

await mkdir(outputDir, { recursive: true });

for (const scenario of scenarios) {
  const document = await readFile(scenario.documentPath, 'utf8');
  const hydrateDocument = await readFile(scenario.hydrateDocumentPath, 'utf8');
  const primary = await recordStorefrontRequest(scenario.operationName, document, scenario.variables);
  const hydrate = await recordStorefrontRequest(scenario.hydrateOperationName, hydrateDocument, scenario.variables);
  const outputPath = path.join(outputDir, `${scenario.scenarioId}.json`);

  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId: scenario.scenarioId,
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        apiSurface: 'storefront',
        endpoint,
        authMode: 'storefront-access-token',
        storefrontToken: {
          id: storedAuth.storefront_token_id || '<unknown>',
          title: storedAuth.storefront_token_title || '<unknown>',
          accessScopes: storedAuth.storefront_access_scopes,
          obtainedAt: storedAuth.obtained_at || '<unknown>',
        },
        primary,
        hydrate,
        upstreamCalls: [hydrate],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(`Wrote ${outputPath}`);
  console.log(`Captured authenticated Storefront status ${primary.response.status} for ${scenario.operationName}`);
}
