/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type MarketCreateData = {
  marketCreate?: {
    market?: {
      id?: string;
      name?: string;
      handle?: string;
      status?: string;
      enabled?: boolean;
    } | null;
    userErrors?: Array<{
      field?: string[] | null;
      message?: string;
      code?: string | null;
    }>;
  };
};

type MarketDeleteData = {
  marketDelete?: {
    deletedId?: string | null;
    userErrors?: Array<{
      field?: string[] | null;
      message?: string;
      code?: string | null;
    }>;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'market-create-handle-dedupe.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketCreateHandleDedupeMutation = `#graphql
mutation MarketCreateHandleDedupe($input: MarketCreateInput!) {
  marketCreate(input: $input) {
    market {
      id
      name
      handle
      status
      enabled
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const marketDeleteMutation = `#graphql
mutation MarketHandleDedupeCleanup($id: ID!) {
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

function createdMarketHandle(result: ConformanceGraphqlResult<MarketCreateData>): string | null {
  const market = result.payload.data?.marketCreate?.market;
  return typeof market?.handle === 'string' ? market.handle : null;
}

function assertExpectedHandle(
  result: ConformanceGraphqlResult<MarketCreateData>,
  expected: string,
  label: string,
): void {
  const handle = createdMarketHandle(result);
  const errors = result.payload.data?.marketCreate?.userErrors ?? [];
  if (result.status !== 200 || handle !== expected || errors.length > 0) {
    throw new Error(
      `${label} did not return expected handle ${expected}: status=${result.status} handle=${String(
        handle,
      )} userErrors=${JSON.stringify(errors)} errors=${JSON.stringify(result.payload.errors ?? null)}`,
    );
  }
}

function assertExpectedUserError(
  result: ConformanceGraphqlResult<MarketCreateData>,
  expectedCode: string,
  expectedField: string[],
  label: string,
): void {
  const market = result.payload.data?.marketCreate?.market;
  const errors = result.payload.data?.marketCreate?.userErrors ?? [];
  const hasExpectedError = errors.some(
    (error) => error.code === expectedCode && JSON.stringify(error.field ?? null) === JSON.stringify(expectedField),
  );
  if (result.status !== 200 || market !== null || !hasExpectedError) {
    throw new Error(
      `${label} did not return expected userError ${expectedCode}: status=${result.status} market=${JSON.stringify(
        market,
      )} userErrors=${JSON.stringify(errors)} errors=${JSON.stringify(result.payload.errors ?? null)}`,
    );
  }
}

async function cleanupMarket(id: string): Promise<ConformanceGraphqlResult<MarketDeleteData>> {
  return runGraphqlRequest<MarketDeleteData>(marketDeleteMutation, { id });
}

await mkdir(outputDir, { recursive: true });

const createdIds: string[] = [];
const cleanup: Array<{
  id: string;
  response: ConformanceGraphqlResult<MarketDeleteData>;
}> = [];
const cases: Array<{
  name: string;
  query: string;
  variables: { input: { name: string } };
  response: ConformanceGraphqlResult<MarketCreateData>;
}> = [];

try {
  const firstVariables = { input: { name: 'Europe' } };
  const first = await runGraphqlRequest<MarketCreateData>(marketCreateHandleDedupeMutation, firstVariables);
  const firstId = createdMarketId(first);
  if (firstId) createdIds.push(firstId);
  assertExpectedHandle(first, 'europe', 'first marketCreate');
  cases.push({
    name: 'marketCreate Europe generated handle',
    query: marketCreateHandleDedupeMutation,
    variables: firstVariables,
    response: first,
  });

  const duplicateNameVariables = { input: { name: 'Europe' } };
  const duplicateName = await runGraphqlRequest<MarketCreateData>(
    marketCreateHandleDedupeMutation,
    duplicateNameVariables,
  );
  assertExpectedUserError(duplicateName, 'TAKEN', ['input', 'name'], 'duplicate-name marketCreate');
  cases.push({
    name: 'marketCreate duplicate Europe name rejected',
    query: marketCreateHandleDedupeMutation,
    variables: duplicateNameVariables,
    response: duplicateName,
  });

  const secondVariables = { input: { name: 'Europe!' } };
  const second = await runGraphqlRequest<MarketCreateData>(marketCreateHandleDedupeMutation, secondVariables);
  const secondId = createdMarketId(second);
  if (secondId) createdIds.push(secondId);
  assertExpectedHandle(second, 'europe-1', 'second marketCreate');
  cases.push({
    name: 'marketCreate Europe punctuation generated handle deduped',
    query: marketCreateHandleDedupeMutation,
    variables: secondVariables,
    response: second,
  });
} finally {
  for (const id of createdIds.toReversed()) {
    cleanup.push({ id, response: await cleanupMarket(id) });
  }
}

const capture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scope: 'HAR-622 marketCreate generated handle dedupe',
  cases,
  cleanup,
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      createdIds,
      cleanupDeletedIds: cleanup.map((entry) => entry.response.payload.data?.marketDelete?.deletedId ?? null),
      handles: cases.map((entry) => entry.response.payload.data?.marketCreate?.market?.handle ?? null),
    },
    null,
    2,
  ),
);
