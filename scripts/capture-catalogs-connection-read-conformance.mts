/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedCase = {
  name: string;
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

type CleanupEntry = {
  kind: 'catalog' | 'market';
  id: string;
  response: ConformanceGraphqlResult;
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

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'catalogs-connection-read.json');

const marketCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-market-create.graphql'),
  'utf8',
);
const catalogCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-catalog-create.graphql'),
  'utf8',
);
const firstPageDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalogs-connection-read.graphql'),
  'utf8',
);
const nextPageDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalogs-connection-next-page.graphql'),
  'utf8',
);

const catalogDeleteDocument = `#graphql
mutation CatalogsConnectionCatalogCleanup($id: ID!) {
  catalogDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const marketDeleteDocument = `#graphql
mutation CatalogsConnectionMarketCleanup($id: ID!) {
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

const cases: CapturedCase[] = [];
const cleanup: CleanupEntry[] = [];
const createdCatalogIds: string[] = [];
const createdMarketIds: string[] = [];
const unique = `catalogs-connection-${new Date().toISOString().replace(/\D/gu, '').slice(0, 14)}`;
const searchQuery = `title:${unique}`;

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result)}`);
  }
}

function rootPayload(result: ConformanceGraphqlResult, root: string): JsonRecord {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw new Error(`Missing data for ${root}: ${JSON.stringify(result.payload)}`);
  }
  const payload = (data as JsonRecord)[root];
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) {
    throw new Error(`Missing root payload ${root}: ${JSON.stringify(result.payload)}`);
  }
  return payload as JsonRecord;
}

function userErrors(result: ConformanceGraphqlResult, root: string): Array<JsonRecord> {
  const errors = rootPayload(result, root)['userErrors'];
  return Array.isArray(errors)
    ? errors.filter(
        (error): error is JsonRecord => typeof error === 'object' && error !== null && !Array.isArray(error),
      )
    : [];
}

function assertNoUserErrors(label: string, result: ConformanceGraphqlResult, root: string): void {
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function nestedId(result: ConformanceGraphqlResult, root: string, field: string): string {
  const payload = rootPayload(result, root);
  const node = payload[field];
  if (typeof node !== 'object' || node === null || Array.isArray(node)) {
    throw new Error(`Missing ${root}.${field}: ${JSON.stringify(result.payload)}`);
  }
  const id = (node as JsonRecord)['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`Missing ${root}.${field}.id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function connectionNodes(result: ConformanceGraphqlResult, root: string): JsonRecord[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw new Error(`Missing response data: ${JSON.stringify(result.payload)}`);
  }
  const connection = (data as JsonRecord)[root];
  if (typeof connection !== 'object' || connection === null || Array.isArray(connection)) {
    throw new Error(`Missing ${root} connection: ${JSON.stringify(result.payload)}`);
  }
  const nodes = (connection as JsonRecord)['nodes'];
  if (!Array.isArray(nodes)) {
    throw new Error(`Missing ${root}.nodes: ${JSON.stringify(result.payload)}`);
  }
  return nodes.filter((node): node is JsonRecord => typeof node === 'object' && node !== null && !Array.isArray(node));
}

function connectionEndCursor(result: ConformanceGraphqlResult, root: string): string {
  const data = result.payload.data as JsonRecord | undefined;
  const connection = data?.[root] as JsonRecord | undefined;
  const pageInfo = connection?.['pageInfo'] as JsonRecord | undefined;
  const endCursor = pageInfo?.['endCursor'];
  if (typeof endCursor !== 'string' || endCursor.length === 0) {
    throw new Error(`Missing ${root}.pageInfo.endCursor: ${JSON.stringify(result.payload)}`);
  }
  return endCursor;
}

async function captureCase(name: string, query: string, variables: JsonRecord): Promise<CapturedCase> {
  const response = await runGraphqlRequest(query, variables);
  assertGraphqlOk(name, response);
  const capture = { name, query, variables, response };
  cases.push(capture);
  return capture;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function captureCaseWhen(
  name: string,
  query: string,
  variables: JsonRecord,
  predicate: (response: ConformanceGraphqlResult) => boolean,
): Promise<CapturedCase> {
  let lastResponse: ConformanceGraphqlResult | null = null;
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    const response = await runGraphqlRequest(query, variables);
    assertGraphqlOk(`${name} attempt ${attempt}`, response);
    lastResponse = response;
    if (predicate(response)) {
      const capture = { name, query, variables, response };
      cases.push(capture);
      return capture;
    }
    await sleep(2500);
  }
  throw new Error(`${name} never reached expected catalog search state: ${JSON.stringify(lastResponse)}`);
}

async function createMarket(label: string): Promise<string> {
  const capture = await captureCase(`${label}MarketCreate`, marketCreateDocument, {
    input: {
      name: `Catalogs Connection ${label} ${unique}`,
    },
  });
  assertNoUserErrors(`${label} marketCreate`, capture.response, 'marketCreate');
  const id = nestedId(capture.response, 'marketCreate', 'market');
  createdMarketIds.push(id);
  return id;
}

async function createCatalog(label: string, marketId: string, status: 'ACTIVE' | 'DRAFT'): Promise<string> {
  const capture = await captureCase(`${label}CatalogCreate`, catalogCreateDocument, {
    input: {
      title: `Catalogs Connection ${label} ${unique}`,
      status,
      context: {
        marketIds: [marketId],
      },
    },
  });
  assertNoUserErrors(`${label} catalogCreate`, capture.response, 'catalogCreate');
  const id = nestedId(capture.response, 'catalogCreate', 'catalog');
  createdCatalogIds.push(id);
  return id;
}

async function cleanupCatalog(id: string): Promise<void> {
  const response = await runGraphqlRequest(catalogDeleteDocument, { id });
  cleanup.push({ kind: 'catalog', id, response });
}

async function cleanupMarket(id: string): Promise<void> {
  const response = await runGraphqlRequest(marketDeleteDocument, { id });
  cleanup.push({ kind: 'market', id, response });
}

let captureFailure: unknown = null;

try {
  const alphaMarketId = await createMarket('Alpha');
  const betaMarketId = await createMarket('Beta');
  await createCatalog('Alpha', alphaMarketId, 'ACTIVE');
  await createCatalog('Beta', betaMarketId, 'DRAFT');

  const firstPage = await captureCaseWhen(
    'catalogsConnectionFirstPage',
    firstPageDocument,
    {
      query: searchQuery,
      first: 1,
      countLimit: 1,
    },
    (response) => {
      const firstNodes = connectionNodes(response, 'firstPage');
      return firstNodes.length === 1 && firstNodes[0]['title'] === `Catalogs Connection Beta ${unique}`;
    },
  );
  if (connectionNodes(firstPage.response, 'wrongType').length !== 0) {
    throw new Error(`Wrong-type catalog query returned nodes: ${JSON.stringify(firstPage.response)}`);
  }

  await captureCaseWhen(
    'catalogsConnectionNextPage',
    nextPageDocument,
    {
      query: searchQuery,
      first: 1,
      after: connectionEndCursor(firstPage.response, 'firstPage'),
    },
    (response) => {
      const nextNodes = connectionNodes(response, 'nextPage');
      return nextNodes.length === 1 && nextNodes[0]['title'] === `Catalogs Connection Alpha ${unique}`;
    },
  );
} catch (error) {
  captureFailure = error;
} finally {
  for (const id of [...createdCatalogIds].reverse()) {
    await cleanupCatalog(id);
  }
  for (const id of [...createdMarketIds].reverse()) {
    await cleanupMarket(id);
  }
}

if (captureFailure) {
  throw captureFailure;
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
      cleanup,
      notes:
        'Creates two disposable MarketCatalog records, captures filtered reverse-title pagination and count/type filters, then deletes the disposable records.',
    },
    null,
    2,
  )}\n`,
  'utf8',
);
console.log(`Wrote ${outputPath}`);
