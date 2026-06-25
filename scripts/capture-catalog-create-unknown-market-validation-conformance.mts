/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CatalogUserError = {
  field?: string[] | null;
  message?: string | null;
  code?: string | null;
};

type CatalogCreateData = {
  catalogCreate?: {
    catalog?: { id?: string } | null;
    userErrors?: CatalogUserError[] | null;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'catalog-create-unknown-market-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const query = await readFile('config/parity-requests/markets/catalog-create-relation-validation.graphql', 'utf8');
const variables = {
  input: {
    title: 'Unknown Market Catalog',
    status: 'ACTIVE',
    context: {
      marketIds: ['gid://shopify/Market/999999999999'],
    },
  },
};

const response: ConformanceGraphqlResult<CatalogCreateData> = await runGraphqlRequest(query, variables);
if (response.status < 200 || response.status >= 300 || response.payload.errors) {
  throw new Error(`catalogCreate unknown market capture failed: ${JSON.stringify(response, null, 2)}`);
}

const payload = response.payload.data?.catalogCreate;
const firstUserError = payload?.userErrors?.[0];
if (payload?.catalog !== null) {
  throw new Error(`Expected catalogCreate.catalog null: ${JSON.stringify(response.payload, null, 2)}`);
}
if (firstUserError?.code !== 'MARKET_NOT_FOUND') {
  throw new Error(`Expected MARKET_NOT_FOUND userError: ${JSON.stringify(response.payload, null, 2)}`);
}
if (JSON.stringify(firstUserError.field) !== JSON.stringify(['input', 'context', 'marketIds', '0'])) {
  throw new Error(`Expected marketIds[0] field path: ${JSON.stringify(response.payload, null, 2)}`);
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cases: [
        {
          name: 'catalogCreateUnknownMarket',
          query,
          variables,
          response,
        },
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain,
      apiVersion,
      userError: firstUserError,
    },
    null,
    2,
  ),
);
