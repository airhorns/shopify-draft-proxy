/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

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
  type?: string | null;
};

type MarketCreateData = {
  marketCreate?: {
    market?: MarketNode | null;
    userErrors?: UserError[] | null;
  } | null;
};

type MarketDeleteData = {
  marketDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[] | null;
  } | null;
};

type ConnectionNode = {
  id?: string | null;
  name?: string | null;
};

type MarketConnection = {
  nodes?: ConnectionNode[] | null;
  edges?: Array<{ cursor?: string | null; node?: ConnectionNode | null }> | null;
  pageInfo?: {
    hasNextPage?: boolean | null;
    hasPreviousPage?: boolean | null;
    startCursor?: string | null;
    endCursor?: string | null;
  } | null;
};

type MarketsReadData = {
  filtered?: MarketConnection | null;
};

type CapturedCase<TData> = {
  name: string;
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<TData>;
};

const scenarioId = 'markets-connection-arguments';
const createRequestPath = path.join(
  'config',
  'parity-requests',
  'markets',
  'markets-connection-arguments-create.graphql',
);
const readRequestPath = path.join('config', 'parity-requests', 'markets', 'markets-connection-arguments-read.graphql');

const marketDeleteMutation = `#graphql
mutation MarketsConnectionArgumentsCleanup($id: ID!) {
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

function userErrors<TData extends JsonRecord>(result: ConformanceGraphqlResult<TData>, root: keyof TData): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = data[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const maybeErrors = (payload as { userErrors?: UserError[] | null }).userErrors;
  return Array.isArray(maybeErrors) ? maybeErrors : [];
}

function assertNoUserErrors<TData extends JsonRecord>(
  result: ConformanceGraphqlResult<TData>,
  root: keyof TData,
  label: string,
): void {
  const errors = userErrors(result, root);
  if (result.status !== 200 || result.payload.errors || errors.length > 0) {
    throw new Error(
      `${label} failed: status=${result.status} userErrors=${JSON.stringify(errors)} errors=${JSON.stringify(
        result.payload.errors ?? null,
      )}`,
    );
  }
}

function createdMarketId(result: ConformanceGraphqlResult<MarketCreateData>): string {
  const id = result.payload.data?.marketCreate?.market?.id;
  if (typeof id !== 'string') {
    throw new Error(`marketCreate did not return a market id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function assertConnectionOrder(read: ConformanceGraphqlResult<MarketsReadData>, expectedNames: string[]): void {
  const connection = read.payload.data?.filtered;
  if (read.status !== 200 || read.payload.errors || !connection) {
    throw new Error(`markets read failed: ${JSON.stringify(read.payload)}`);
  }

  const nodeNames = (connection.nodes ?? []).map((node) => node.name);
  const edgeNames = (connection.edges ?? []).map((edge) => edge.node?.name);
  if (JSON.stringify(nodeNames) !== JSON.stringify(expectedNames)) {
    throw new Error(`Expected filtered.nodes names ${JSON.stringify(expectedNames)}, got ${JSON.stringify(nodeNames)}`);
  }
  if (JSON.stringify(edgeNames) !== JSON.stringify(expectedNames)) {
    throw new Error(`Expected filtered.edges names ${JSON.stringify(expectedNames)}, got ${JSON.stringify(edgeNames)}`);
  }

  const cursors = (connection.edges ?? []).map((edge) => edge.cursor);
  if (cursors.some((cursor) => typeof cursor !== 'string' || cursor.length === 0)) {
    throw new Error(`Expected non-empty edge cursors, got ${JSON.stringify(connection.edges)}`);
  }

  const pageInfo = connection.pageInfo;
  if (
    !pageInfo ||
    pageInfo.hasNextPage !== true ||
    pageInfo.hasPreviousPage !== false ||
    typeof pageInfo.startCursor !== 'string' ||
    typeof pageInfo.endCursor !== 'string'
  ) {
    throw new Error(`Expected first window pageInfo with a following page, got ${JSON.stringify(pageInfo)}`);
  }
}

async function main(): Promise<void> {
  const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
  if (apiVersion !== '2026-04') {
    throw new Error(`Expected SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`);
  }
  const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
  const outputPath = path.join(outputDir, `${scenarioId}.json`);
  const marketCreateMutation = await readFile(createRequestPath, 'utf8');
  const marketsReadQuery = await readFile(readRequestPath, 'utf8');

  const { runGraphqlRequest } = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });

  async function captureCase<TData>(name: string, query: string, variables: JsonRecord): Promise<CapturedCase<TData>> {
    return {
      name,
      query,
      variables,
      response: await runGraphqlRequest<TData>(query, variables),
    };
  }

  const unique = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
  const queryToken = `ZZZDPConn${unique}`;
  const names = [`${queryToken} Alpha`, `${queryToken} Beta`, `${queryToken} Gamma`];
  const cases: Array<CapturedCase<unknown>> = [];
  const createdMarketIds: string[] = [];
  const cleanup: Array<CapturedCase<MarketDeleteData>> = [];

  try {
    for (const [index, name] of names.entries()) {
      const create = await captureCase<MarketCreateData>(`marketCreate ${index + 1}`, marketCreateMutation, {
        input: { name, enabled: true },
      });
      assertNoUserErrors(create.response, 'marketCreate', `marketCreate ${index + 1}`);
      createdMarketIds.push(createdMarketId(create.response));
      cases.push(create);
    }

    const read = await captureCase<MarketsReadData>('markets query sort reverse', marketsReadQuery, {
      first: 2,
      query: 'status:ACTIVE',
      reverse: true,
    });
    assertConnectionOrder(read.response, [names[2], names[1]]);
    cases.push(read);
  } finally {
    for (const id of [...createdMarketIds].reverse()) {
      cleanup.push(await captureCase<MarketDeleteData>('marketDelete cleanup', marketDeleteMutation, { id }));
    }
  }

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId,
        storeDomain,
        apiVersion,
        capturedAt: new Date().toISOString(),
        scope: 'markets query/sortKey/reverse connection arguments',
        queryToken,
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
        queryToken,
        cases: cases.map((capture) => ({ name: capture.name, status: capture.response.status })),
        cleanup: cleanup.map((capture) => ({ name: capture.name, status: capture.response.status })),
      },
      null,
      2,
    ),
  );
}

await main();
