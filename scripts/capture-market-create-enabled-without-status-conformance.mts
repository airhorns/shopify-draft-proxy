/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = {
  field?: string[] | null;
  message?: string;
  code?: string | null;
};

type MarketCreateData = {
  marketCreate?: {
    market?: {
      id?: string;
      name?: string;
      handle?: string;
      status?: string;
      enabled?: boolean;
      type?: string;
    } | null;
    userErrors?: UserError[];
  };
};

type MarketDeleteData = {
  marketDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'market-create-enabled-without-status.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketCreateMutation = await readFile(
  'config/parity-requests/markets/market-create-enabled-without-status.graphql',
  'utf8',
);

const marketDeleteMutation = `#graphql
mutation MarketCreateEnabledWithoutStatusCleanup($id: ID!) {
  marketDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

function createdMarketId(result: ConformanceGraphqlResult<MarketCreateData>): string | null {
  const market = result.payload.data?.marketCreate?.market;
  return typeof market?.id === 'string' ? market.id : null;
}

function assertCreatedWithoutErrors(result: ConformanceGraphqlResult<MarketCreateData>): string {
  const market = result.payload.data?.marketCreate?.market;
  const userErrors = result.payload.data?.marketCreate?.userErrors ?? [];
  const hasInvalidCombination = userErrors.some((error) => error.code === 'INVALID_STATUS_AND_ENABLED_COMBINATION');
  const id = createdMarketId(result);

  if (
    result.status !== 200 ||
    result.payload.errors ||
    !market ||
    !id ||
    userErrors.length > 0 ||
    hasInvalidCombination
  ) {
    throw new Error(
      `marketCreate enabled-without-status did not create a market: status=${result.status} market=${JSON.stringify(
        market,
      )} userErrors=${JSON.stringify(userErrors)} errors=${JSON.stringify(result.payload.errors ?? null)}`,
    );
  }

  return id;
}

async function cleanupMarket(id: string): Promise<ConformanceGraphqlResult<MarketDeleteData>> {
  return runGraphqlRequest<MarketDeleteData>(marketDeleteMutation, { id });
}

await mkdir(outputDir, { recursive: true });

const suffix = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const variables = {
  input: {
    name: `Enabled Without Status ${suffix}`,
    enabled: true,
  },
};

const response = await runGraphqlRequest<MarketCreateData>(marketCreateMutation, variables);
const createdId = assertCreatedWithoutErrors(response);
const cleanup = await cleanupMarket(createdId);

const capture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scope: 'marketCreate accepts enabled true with omitted status',
  cases: [{ name: 'marketCreate enabled true without status', query: marketCreateMutation, variables, response }],
  cleanup: {
    marketDelete: {
      query: marketDeleteMutation,
      variables: { id: createdId },
      response: cleanup,
    },
  },
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      createdId,
      cleanupDeletedId: cleanup.payload.data?.marketDelete?.deletedId ?? null,
      market: response.payload.data?.marketCreate?.market ?? null,
    },
    null,
    2,
  ),
);
