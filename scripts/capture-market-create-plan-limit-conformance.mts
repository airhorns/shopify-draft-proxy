/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = {
  field?: string[] | null;
  message?: string;
  code?: string | null;
};

type CapturedCase<TData> = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<TData>;
};

type MarketCreateData = {
  marketCreate?: {
    market?: {
      id?: string | null;
      name?: string | null;
      handle?: string | null;
      status?: string | null;
      enabled?: boolean | null;
    } | null;
    userErrors?: UserError[];
  } | null;
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
const outputPath = path.join(outputDir, 'market-create-plan-limit-markets-home.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketCreateMutation = `#graphql
mutation MarketCreatePlanLimit($input: MarketCreateInput!) {
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
mutation MarketCreatePlanLimitCleanup($id: ID!) {
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

async function captureCase<TData>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase<TData>> {
  const response = await runGraphqlRequest<TData>(query, variables);
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

function marketId(capture: CapturedCase<MarketCreateData>): string | null {
  const id = capture.response.payload.data?.marketCreate?.market?.id;
  return typeof id === 'string' ? id : null;
}

function userErrorCodes(capture: CapturedCase<MarketCreateData>): string[] {
  return (capture.response.payload.data?.marketCreate?.userErrors ?? [])
    .map((error) => error.code)
    .filter((code): code is string => typeof code === 'string');
}

const stamp = new Date()
  .toISOString()
  .replace(/[^0-9]/g, '')
  .slice(0, 14);
const createdIds: string[] = [];
const cases: CapturedCase<MarketCreateData>[] = [];
const cleanupCases: CapturedCase<MarketDeleteData>[] = [];

try {
  for (let index = 1; index <= 4; index += 1) {
    const capture = await captureCase<MarketCreateData>(`marketCreate${index}`, marketCreateMutation, {
      input: {
        name: `Draft Proxy Plan Limit ${stamp} ${index}`,
        status: 'ACTIVE',
        enabled: true,
      },
    });
    cases.push(capture);

    const id = marketId(capture);
    if (id) createdIds.push(id);

    const codes = userErrorCodes(capture);
    if (!id || codes.length > 0 || codes.includes('SHOP_REACHED_PLAN_MARKETS_LIMIT')) {
      throw new Error(
        `marketCreate${index} did not create an enabled market without plan-limit errors: ${JSON.stringify(
          capture.response.payload,
        )}`,
      );
    }
  }
} finally {
  for (const id of createdIds.toReversed()) {
    cleanupCases.push(
      await captureCase<MarketDeleteData>('marketDeleteCleanup', marketDeleteMutation, {
        id,
      }),
    );
  }
}

const fourthCase = cases[3];
if (!fourthCase) {
  throw new Error(`Expected four successful marketCreate captures, got ${cases.length}`);
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cases,
      cleanupCases,
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
      createdMarkets: cases.length,
      cleanedUpMarkets: cleanupCases.length,
      fourthMarketUserErrorCodes: userErrorCodes(fourthCase),
    },
    null,
    2,
  ),
);
