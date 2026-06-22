/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
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

type MarketNode = {
  id?: string | null;
  name?: string | null;
  status?: string | null;
  enabled?: boolean | null;
};

type CapturedCase<TData> = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<TData>;
};

type MarketCreateData = {
  marketCreate?: {
    market?: MarketNode | null;
    userErrors?: UserError[];
  } | null;
};

type MarketUpdateData = {
  marketUpdate?: {
    market?: MarketNode | null;
    userErrors?: UserError[];
  } | null;
};

type MarketReadData = {
  market?: MarketNode | null;
};

type MarketDeleteData = {
  marketDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'market-update-scalars.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketCreateMutation = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-update-scalars-create.graphql'),
  'utf8',
);
const marketUpdateMutation = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-update-scalars-update.graphql'),
  'utf8',
);
const marketReadQuery = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-update-scalars-read.graphql'),
  'utf8',
);

const marketDeleteMutation = `#graphql
mutation MarketUpdateScalarsCleanup($id: ID!) {
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

function userErrors<TData>(result: ConformanceGraphqlResult<TData>, root: keyof TData): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = data[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const maybeErrors = (payload as { userErrors?: UserError[] }).userErrors;
  return Array.isArray(maybeErrors) ? maybeErrors : [];
}

function assertNoUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: keyof TData, label: string): void {
  const errors = userErrors(result, root);
  if (result.status !== 200 || result.payload.errors || errors.length > 0) {
    throw new Error(
      `${label} failed: status=${result.status} userErrors=${JSON.stringify(errors)} errors=${JSON.stringify(
        result.payload.errors ?? null,
      )}`,
    );
  }
}

function assertMarketField(market: MarketNode | null | undefined, field: keyof MarketNode, expected: unknown): void {
  if (market?.[field] !== expected) {
    throw new Error(`Expected market.${field}=${JSON.stringify(expected)}, got ${JSON.stringify(market)}`);
  }
}

function createdMarketId(result: ConformanceGraphqlResult<MarketCreateData>): string {
  const id = result.payload.data?.marketCreate?.market?.id;
  if (typeof id !== 'string') {
    throw new Error(`marketCreate did not return a market id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

async function captureCase<TData>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase<TData>> {
  return {
    name,
    query,
    variables,
    response: await runGraphqlRequest<TData>(query, variables),
  };
}

const unique = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const originalName = `Draft Proxy Scalar ${unique}`;
const updatedName = `Draft Proxy Scalar ${unique} Updated`;
const cases: Array<CapturedCase<unknown>> = [];
let marketId: string | null = null;
let cleanup: CapturedCase<MarketDeleteData> | null = null;

try {
  const create = await captureCase<MarketCreateData>('marketCreate scalar update setup', marketCreateMutation, {
    input: { name: originalName, enabled: true },
  });
  assertNoUserErrors(create.response, 'marketCreate', 'marketCreate scalar update setup');
  marketId = createdMarketId(create.response);
  cases.push(create);

  const update = await captureCase<MarketUpdateData>('marketUpdate name and status', marketUpdateMutation, {
    id: marketId,
    input: { name: updatedName, status: 'DRAFT' },
  });
  assertNoUserErrors(update.response, 'marketUpdate', 'marketUpdate name and status');
  assertMarketField(update.response.payload.data?.marketUpdate?.market, 'name', updatedName);
  assertMarketField(update.response.payload.data?.marketUpdate?.market, 'status', 'DRAFT');
  assertMarketField(update.response.payload.data?.marketUpdate?.market, 'enabled', false);
  cases.push(update);

  const read = await captureCase<MarketReadData>('market read after scalar update', marketReadQuery, {
    id: marketId,
  });
  if (read.response.status !== 200 || read.response.payload.errors) {
    throw new Error(`market read after scalar update failed: ${JSON.stringify(read.response.payload)}`);
  }
  assertMarketField(read.response.payload.data?.market, 'name', updatedName);
  assertMarketField(read.response.payload.data?.market, 'status', 'DRAFT');
  assertMarketField(read.response.payload.data?.market, 'enabled', false);
  cases.push(read);
} finally {
  if (marketId) {
    cleanup = await captureCase<MarketDeleteData>('marketDelete scalar update cleanup', marketDeleteMutation, {
      id: marketId,
    });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scope: 'marketUpdate scalar name/status read-after-write',
      cases,
      cleanup,
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
      cases: cases.map((capture) => ({ name: capture.name, status: capture.response.status })),
      cleanup: cleanup ? { name: cleanup.name, status: cleanup.response.status } : null,
    },
    null,
    2,
  ),
);
