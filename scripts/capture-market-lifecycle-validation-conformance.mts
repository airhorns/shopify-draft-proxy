/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'market-lifecycle-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketCreateBlankNameQuery = `#graphql
mutation MarketCreateBlankName($input: MarketCreateInput!) {
  marketCreate(input: $input) {
    market { id name handle status enabled }
    userErrors { field message code }
  }
}`;

const marketUpdateUnknownIdQuery = `#graphql
mutation MarketUpdateUnknownId($id: ID!, $input: MarketUpdateInput!) {
  marketUpdate(id: $id, input: $input) {
    market { id name handle status enabled }
    userErrors { field message code }
  }
}`;

const marketDeleteUnknownIdQuery = `#graphql
mutation MarketDeleteUnknownId($id: ID!) {
  marketDelete(id: $id) {
    deletedId
    userErrors { field message code }
  }
}`;

async function captureCase(name: string, query: string, variables: Record<string, unknown>): Promise<CapturedCase> {
  const response = await runGraphqlRequest(query, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${name} failed: ${JSON.stringify(response.payload)}`);
  }

  return {
    name,
    query,
    variables,
    response,
  };
}

function readFirstUserErrorCode(capture: CapturedCase, root: string): string | null {
  const data = capture.response.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) return null;
  const payload = data[root];
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) return null;
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors)) return null;
  const first = userErrors[0];
  if (typeof first !== 'object' || first === null || Array.isArray(first)) return null;
  const code = first['code'];
  return typeof code === 'string' ? code : null;
}

const cases = [
  await captureCase('marketCreateBlankName', marketCreateBlankNameQuery, {
    input: {
      name: '',
    },
  }),
  await captureCase('marketUpdateUnknownId', marketUpdateUnknownIdQuery, {
    id: 'gid://shopify/Market/9999999',
    input: {
      name: 'Nope',
    },
  }),
  await captureCase('marketDeleteUnknownId', marketDeleteUnknownIdQuery, {
    id: 'gid://shopify/Market/9999999',
  }),
];

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cases,
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
      cases: cases.map((capture) => ({
        name: capture.name,
        status: capture.response.status,
      })),
      marketUpdateUnknownIdCode: readFirstUserErrorCode(cases[1], 'marketUpdate'),
      marketDeleteUnknownIdCode: readFirstUserErrorCode(cases[2], 'marketDelete'),
    },
    null,
    2,
  ),
);
