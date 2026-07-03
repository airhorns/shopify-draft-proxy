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

type CapturedCase = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<CatalogCreateData>;
};

const expectedField = ['input', 'context'];
const expectedMessage = 'Must provide exactly one context type.';
const expectedCode = 'MUST_PROVIDE_EXACTLY_ONE_CONTEXT_TYPE';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'catalog-create-empty-context.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const query = await readFile('config/parity-requests/markets/catalog-create-empty-context.graphql', 'utf8');

async function captureCase(name: string, variables: Record<string, unknown>): Promise<CapturedCase> {
  const response = await runGraphqlRequest<CatalogCreateData>(query, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${name} failed: ${JSON.stringify(response, null, 2)}`);
  }

  const payload = response.payload.data?.catalogCreate;
  const firstUserError = payload?.userErrors?.[0];
  if (payload?.catalog !== null) {
    throw new Error(`${name}: expected catalogCreate.catalog null: ${JSON.stringify(response.payload, null, 2)}`);
  }
  if (firstUserError?.code !== expectedCode) {
    throw new Error(`${name}: expected ${expectedCode} userError: ${JSON.stringify(response.payload, null, 2)}`);
  }
  if (firstUserError.message !== expectedMessage) {
    throw new Error(`${name}: expected exact message: ${JSON.stringify(response.payload, null, 2)}`);
  }
  if (JSON.stringify(firstUserError.field) !== JSON.stringify(expectedField)) {
    throw new Error(`${name}: expected input.context field path: ${JSON.stringify(response.payload, null, 2)}`);
  }

  return {
    name,
    query,
    variables,
    response,
  };
}

const cases = [
  await captureCase('catalogCreateEmptyContext', {
    input: {
      title: 'Empty Context Catalog',
      status: 'ACTIVE',
      context: {},
    },
  }),
  await captureCase('catalogCreateMultipleContextTypes', {
    input: {
      title: 'Multiple Context Types Catalog',
      status: 'ACTIVE',
      context: {
        marketIds: ['gid://shopify/Market/999999999999'],
        companyLocationIds: ['gid://shopify/CompanyLocation/999999999999'],
      },
    },
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
      userErrors: cases.map((capturedCase) => capturedCase.response.payload.data?.catalogCreate?.userErrors?.[0]),
    },
    null,
    2,
  ),
);
