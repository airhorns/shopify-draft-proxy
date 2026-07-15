/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildStorefrontRequestHeaders, getStoredStorefrontAccessToken } from './shopify-conformance-auth.mjs';

const scenarioId = 'storefront-shop-name-proxy-parity';
const operationName = 'StorefrontShopNameProxyParity';
const { storeDomain, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
  requireAdminOrigin: false,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'online-store');
const outputPath = path.join(outputDir, `${scenarioId}.json`);
const documentPath = path.join('config', 'parity-requests', 'online-store', 'storefront-shop-name.graphql');
const document = await readFile(documentPath, 'utf8');
const variables: Record<string, never> = {};
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
const result = await runStorefrontGraphqlRequest(
  {
    storeOrigin: `https://${storeDomain}`,
    apiVersion,
    storefrontAccessToken: storedAuth.storefront_access_token,
  },
  document,
  variables,
);
const redactedHeaders = Object.fromEntries(
  Object.keys(authHeaders).map((name) => [name, '<redacted:storefront-access-token>']),
);

const recordedRequest = {
  method: 'POST',
  apiSurface: 'storefront',
  apiVersion,
  path: pathName,
  endpoint,
  authMode: 'storefront-access-token',
  headers: redactedHeaders,
  operationName,
  query: document,
  variables,
  response: {
    status: result.status,
    body: result.payload,
  },
};

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId,
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
      primary: recordedRequest,
      upstreamCalls: [recordedRequest],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote ${outputPath}`);
console.log(`Captured authenticated Storefront status ${result.status} for ${operationName}`);
