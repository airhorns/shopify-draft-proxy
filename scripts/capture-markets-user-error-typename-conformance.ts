/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { readFile, mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = {
  __typename?: string | null;
  code?: string | null;
  field?: string[] | null;
  message?: string;
};

type MutationPayload = {
  userErrors?: UserError[];
};

type CapturedCase = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const expectedTypenames: Record<string, string> = {
  priceListCreate: 'PriceListUserError',
  priceListUpdate: 'PriceListUserError',
  priceListDelete: 'PriceListUserError',
  quantityRulesDelete: 'QuantityRuleUserError',
  webPresenceCreate: 'MarketUserError',
  webPresenceUpdate: 'MarketUserError',
  webPresenceDelete: 'MarketUserError',
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

const documentPath = path.join('config', 'parity-requests', 'markets', 'markets-user-error-typename.graphql');
const query = await readFile(documentPath, 'utf8');
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'markets-user-error-typename.json');

const variables = {
  priceListCreateInput: {
    name: '',
    currency: 'USD',
    parent: {
      adjustment: {
        type: 'PERCENTAGE_DECREASE',
        value: 10,
      },
    },
  },
  priceListUpdateId: 'gid://shopify/PriceList/0',
  priceListUpdateInput: {
    name: 'Does not exist',
  },
  priceListDeleteId: 'gid://shopify/PriceList/0',
  quantityRulesDeletePriceListId: 'gid://shopify/PriceList/0',
  quantityRulesDeleteVariantIds: ['gid://shopify/ProductVariant/0'],
  webPresenceCreateInput: {
    defaultLocale: 'en',
    subfolderSuffix: 'x',
  },
  webPresenceUpdateId: 'gid://shopify/MarketWebPresence/0',
  webPresenceUpdateInput: {
    defaultLocale: 'en',
  },
  webPresenceDeleteId: 'gid://shopify/MarketWebPresence/0',
};

function objectValue(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
}

function userErrorsFor(response: ConformanceGraphqlResult, root: string): UserError[] {
  const data = objectValue(response.payload.data);
  const payload = objectValue(data[root]) as MutationPayload;
  return Array.isArray(payload.userErrors) ? payload.userErrors : [];
}

function assertTypename(response: ConformanceGraphqlResult, root: string): void {
  const errors = userErrorsFor(response, root);
  const expectedTypename = expectedTypenames[root];
  if (errors.length === 0) {
    throw new Error(`${root} returned no userErrors: ${JSON.stringify(response.payload, null, 2)}`);
  }
  const mismatched = errors.filter((error) => error.__typename !== expectedTypename);
  if (mismatched.length > 0) {
    throw new Error(`${root} expected ${expectedTypename} userErrors: ${JSON.stringify(mismatched, null, 2)}`);
  }
}

function assertGraphqlOk(response: ConformanceGraphqlResult): void {
  if (response.status !== 200 || response.payload.errors) {
    throw new Error(`capture failed: ${JSON.stringify(response, null, 2)}`);
  }
}

const response = await runGraphqlRequest(query, variables);
assertGraphqlOk(response);
for (const root of Object.keys(expectedTypenames)) {
  assertTypename(response, root);
}

const cases: CapturedCase[] = [
  {
    name: 'markets typed userErrors include __typename',
    query,
    variables,
    response,
  },
];

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scope: 'Markets typed userErrors __typename validation',
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
      roots: Object.keys(expectedTypenames),
    },
    null,
    2,
  ),
);
