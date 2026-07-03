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

type MarketRegionNode = {
  __typename?: string;
  id?: string;
  name?: string;
  code?: string;
};

type MarketRecord = {
  id?: string;
  conditions?: {
    regionsCondition?: {
      regions?: {
        nodes?: MarketRegionNode[];
      };
    };
  };
};

type MarketRegionNodeShapeData = {
  marketCreate?: {
    market?: MarketRecord | null;
    userErrors?: UserError[];
  };
  market?: MarketRecord | null;
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
const outputPath = path.join(outputDir, 'market-create-region-node-shape.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketCreateMutation = await readFile(
  'config/parity-requests/markets/market-create-region-node-shape.graphql',
  'utf8',
);
const marketReadQuery = await readFile(
  'config/parity-requests/markets/market-create-region-node-shape-read.graphql',
  'utf8',
);

const marketDeleteMutation = `#graphql
mutation MarketCreateRegionNodeShapeCleanup($id: ID!) {
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

function createdMarketId(result: ConformanceGraphqlResult<MarketRegionNodeShapeData>): string | null {
  const market = result.payload.data?.marketCreate?.market;
  return typeof market?.id === 'string' ? market.id : null;
}

function regionNodes(market: MarketRecord | null | undefined): MarketRegionNode[] {
  return market?.conditions?.regionsCondition?.regions?.nodes ?? [];
}

function assertCreatedMarket(result: ConformanceGraphqlResult<MarketRegionNodeShapeData>, label: string): string {
  const market = result.payload.data?.marketCreate?.market;
  const userErrors = result.payload.data?.marketCreate?.userErrors ?? [];
  const id = createdMarketId(result);
  if (result.status !== 200 || result.payload.errors || !market || !id || userErrors.length > 0) {
    throw new Error(
      `${label} did not create a market: status=${result.status} market=${JSON.stringify(
        market,
      )} userErrors=${JSON.stringify(userErrors)} errors=${JSON.stringify(result.payload.errors ?? null)}`,
    );
  }
  return id;
}

function assertCanadaRegionNode(market: MarketRecord | null | undefined, label: string): void {
  const nodes = regionNodes(market);
  const node = nodes[0];
  if (
    nodes.length !== 1 ||
    !node ||
    typeof node.id !== 'string' ||
    node.id.length === 0 ||
    node.__typename !== 'MarketRegionCountry' ||
    node.name !== 'Canada' ||
    node.code !== 'CA'
  ) {
    throw new Error(`${label} returned unexpected region nodes: ${JSON.stringify(nodes)}`);
  }
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
    name: `Draft Proxy Region Node ${suffix}`,
    conditions: {
      regionsCondition: {
        regions: [{ countryCode: 'CA' }],
      },
    },
  },
};

const cases: Array<{
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<MarketRegionNodeShapeData>;
}> = [];
let createdId: string | null = null;
let cleanup: ConformanceGraphqlResult<MarketDeleteData> | null = null;

try {
  const create = await runGraphqlRequest<MarketRegionNodeShapeData>(marketCreateMutation, variables);
  createdId = assertCreatedMarket(create, 'marketCreate region node shape');
  assertCanadaRegionNode(create.payload.data?.marketCreate?.market, 'marketCreate region node shape');
  cases.push({
    name: 'marketCreate returns MarketRegionCountry identity fields',
    query: marketCreateMutation,
    variables,
    response: create,
  });

  const readVariables = { id: createdId };
  const read = await runGraphqlRequest<MarketRegionNodeShapeData>(marketReadQuery, readVariables);
  if (read.status !== 200 || read.payload.errors || !read.payload.data?.market) {
    throw new Error(
      `market read-after-create failed: status=${read.status} market=${JSON.stringify(
        read.payload.data?.market ?? null,
      )} errors=${JSON.stringify(read.payload.errors ?? null)}`,
    );
  }
  assertCanadaRegionNode(read.payload.data.market, 'market read-after-create region node shape');
  cases.push({
    name: 'market read-after-create returns MarketRegionCountry identity fields',
    query: marketReadQuery,
    variables: readVariables,
    response: read,
  });
} finally {
  if (createdId) {
    cleanup = await cleanupMarket(createdId);
  }
}

const capture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scope: 'marketCreate country region node identity fields and read-after-write shape',
  cases,
  cleanup: createdId
    ? {
        marketDelete: {
          query: marketDeleteMutation,
          variables: { id: createdId },
          response: cleanup,
        },
      }
    : null,
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
      cleanupDeletedId: cleanup?.payload.data?.marketDelete?.deletedId ?? null,
      createNode: regionNodes(cases[0]?.response.payload.data?.marketCreate?.market)[0] ?? null,
      readNode: regionNodes(cases[1]?.response.payload.data?.market)[0] ?? null,
    },
    null,
    2,
  ),
);
