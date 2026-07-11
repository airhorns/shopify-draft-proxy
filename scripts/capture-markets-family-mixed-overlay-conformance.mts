/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type UserError = {
  field?: string[] | null;
  message?: string;
  code?: string | null;
};

type CapturedCase<TData = unknown> = {
  name: string;
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<TData>;
};

type MarketCreateData = {
  marketCreate?: {
    market?: { id?: string | null } | null;
    userErrors?: UserError[];
  } | null;
};

type CatalogCreateData = {
  catalogCreate?: {
    catalog?: { id?: string | null } | null;
    userErrors?: UserError[];
  } | null;
};

type PriceListCreateData = {
  priceListCreate?: {
    priceList?: { id?: string | null } | null;
    userErrors?: UserError[];
  } | null;
};

type WebPresenceCreateData = {
  webPresenceCreate?: {
    webPresence?: { id?: string | null; subfolderSuffix?: string | null } | null;
    userErrors?: UserError[];
  } | null;
};

type DeleteData = {
  deletedId?: string | null;
  userErrors?: UserError[];
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
const outputPath = path.join(outputDir, 'markets-family-mixed-overlay-read.json');

const marketCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-market-create.graphql'),
  'utf8',
);
const catalogCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-context-update-catalog-create.graphql'),
  'utf8',
);
const priceListCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-update-relation-read-price-list-create.graphql'),
  'utf8',
);
const webPresenceCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'web-presence-delete-create.graphql'),
  'utf8',
);
const mixedReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'markets-family-mixed-overlay-read.graphql'),
  'utf8',
);
const priceListReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'markets-family-mixed-overlay-price-list-read.graphql'),
  'utf8',
);

const marketDeleteDocument = `#graphql
mutation MarketsFamilyMixedOverlayMarketCleanup($id: ID!) {
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

const catalogDeleteDocument = `#graphql
mutation MarketsFamilyMixedOverlayCatalogCleanup($id: ID!) {
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

const priceListDeleteDocument = `#graphql
mutation MarketsFamilyMixedOverlayPriceListCleanup($id: ID!) {
  priceListDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const webPresenceDeleteDocument = `#graphql
mutation MarketsFamilyMixedOverlayWebPresenceCleanup($id: ID!) {
  webPresenceDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

const cases: Array<CapturedCase<unknown>> = [];
const cleanup: Array<{ type: string; id: string; response: ConformanceGraphqlResult<unknown> }> = [];
const createdMarketIds: string[] = [];
const createdCatalogIds: string[] = [];
const createdPriceListIds: string[] = [];
const createdWebPresenceIds: string[] = [];

const unique = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
const token = `ZZZMARKETSMIX${unique}`;
const preexistingMarketName = `${token} Pre Market`;
const stagedMarketName = `${token} Staged Market`;
const preexistingCatalogTitle = `${token} Pre Catalog`;
const stagedCatalogTitle = `${token} Staged Catalog`;
const preexistingPriceListName = `${token} Pre Prices`;
const stagedPriceListName = `${token} Staged Prices`;
const catalogQuery = `title:${token}`;

function randomLetters(length: number): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz';
  return Array.from({ length }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join('');
}

function dataObject(result: ConformanceGraphqlResult<unknown>): JsonRecord {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw new Error(`Missing response data: ${JSON.stringify(result.payload)}`);
  }
  return data as JsonRecord;
}

function rootPayload(result: ConformanceGraphqlResult<unknown>, root: string): JsonRecord {
  const payload = dataObject(result)[root];
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) {
    throw new Error(`Missing root payload ${root}: ${JSON.stringify(result.payload)}`);
  }
  return payload as JsonRecord;
}

function userErrors(result: ConformanceGraphqlResult<unknown>, root: string): UserError[] {
  const errors = rootPayload(result, root)['userErrors'];
  return Array.isArray(errors) ? (errors as UserError[]) : [];
}

function assertNoGraphqlErrors(result: ConformanceGraphqlResult<unknown>, label: string): void {
  if (result.status !== 200 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(result: ConformanceGraphqlResult<unknown>, root: string, label: string): void {
  assertNoGraphqlErrors(result, label);
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function nestedId(result: ConformanceGraphqlResult<unknown>, root: string, field: string): string {
  const node = rootPayload(result, root)[field];
  if (typeof node !== 'object' || node === null || Array.isArray(node)) {
    throw new Error(`Missing ${root}.${field}: ${JSON.stringify(result.payload)}`);
  }
  const id = (node as JsonRecord)['id'];
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`Missing ${root}.${field}.id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function connectionNodes(result: ConformanceGraphqlResult<unknown>, root: string): JsonRecord[] {
  const connection = dataObject(result)[root];
  if (typeof connection !== 'object' || connection === null || Array.isArray(connection)) {
    throw new Error(`Missing ${root} connection: ${JSON.stringify(result.payload)}`);
  }
  const nodes = (connection as JsonRecord)['nodes'];
  if (!Array.isArray(nodes)) {
    throw new Error(`Missing ${root}.nodes: ${JSON.stringify(result.payload)}`);
  }
  return nodes.filter((node): node is JsonRecord => typeof node === 'object' && node !== null && !Array.isArray(node));
}

function nodeStrings(nodes: JsonRecord[], key: string): string[] {
  return nodes.flatMap((node) => {
    const value = node[key];
    return typeof value === 'string' ? [value] : [];
  });
}

function catalogPriceListNames(result: ConformanceGraphqlResult<unknown>): string[] {
  return connectionNodes(result, 'catalogs').flatMap((catalog) => {
    const priceList = catalog['priceList'];
    if (typeof priceList !== 'object' || priceList === null || Array.isArray(priceList)) return [];
    const name = (priceList as JsonRecord)['name'];
    return typeof name === 'string' ? [name] : [];
  });
}

function countValue(result: ConformanceGraphqlResult<unknown>, root: string): number | null {
  const count = rootPayload(result, root)['count'];
  return typeof count === 'number' ? count : null;
}

function mixedReadHasExpectedRows(
  result: ConformanceGraphqlResult<unknown>,
  expected: {
    markets: string[];
    catalogs: string[];
    priceLists: string[];
    webPresenceSuffix: string;
    catalogCount: number;
  },
): boolean {
  if (result.status !== 200 || result.payload.errors) {
    return false;
  }
  const marketNames = nodeStrings(connectionNodes(result, 'markets'), 'name');
  const catalogTitles = nodeStrings(connectionNodes(result, 'catalogs'), 'title');
  const priceListNames = nodeStrings(connectionNodes(result, 'priceLists'), 'name');
  const webPresenceSuffixes = nodeStrings(connectionNodes(result, 'webPresences'), 'subfolderSuffix');
  const nestedPriceListNames = catalogPriceListNames(result);
  return (
    expected.markets.every((name) => marketNames.includes(name)) &&
    expected.catalogs.every((title) => catalogTitles.includes(title)) &&
    expected.priceLists.every((name) => priceListNames.includes(name)) &&
    expected.priceLists.every((name) => nestedPriceListNames.includes(name)) &&
    webPresenceSuffixes.includes(expected.webPresenceSuffix) &&
    countValue(result, 'catalogsCount') === expected.catalogCount
  );
}

async function captureCase<TData>(name: string, query: string, variables: JsonRecord): Promise<CapturedCase<TData>> {
  return {
    name,
    query,
    variables,
    response: await runGraphqlRequest<TData>(query, variables),
  };
}

async function captureWhen(
  name: string,
  query: string,
  variables: JsonRecord,
  predicate: (response: ConformanceGraphqlResult<unknown>) => boolean,
): Promise<CapturedCase<unknown>> {
  let latest: CapturedCase<unknown> | undefined;
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    latest = await captureCase(name, query, variables);
    assertNoGraphqlErrors(latest.response, `${name} attempt ${attempt}`);
    if (predicate(latest.response)) {
      cases.push(latest);
      return latest;
    }
    await sleep(2500);
  }
  throw new Error(`${name} never reached expected mixed Markets-family state: ${JSON.stringify(latest?.response)}`);
}

async function captureMutation<TData>(
  name: string,
  query: string,
  variables: JsonRecord,
  root: string,
): Promise<CapturedCase<TData>> {
  const capture = await captureCase<TData>(name, query, variables);
  assertNoUserErrors(capture.response as ConformanceGraphqlResult<unknown>, root, name);
  cases.push(capture as CapturedCase<unknown>);
  return capture;
}

async function cleanupId(type: string, id: string, query: string): Promise<void> {
  cleanup.push({
    type,
    id,
    response: await runGraphqlRequest<DeleteData>(query, { id }),
  });
}

async function createMarket(name: string): Promise<string> {
  const capture = await captureMutation<MarketCreateData>(
    `marketCreate ${name}`,
    marketCreateDocument,
    { input: { name, enabled: true } },
    'marketCreate',
  );
  const id = nestedId(capture.response as ConformanceGraphqlResult<unknown>, 'marketCreate', 'market');
  createdMarketIds.push(id);
  return id;
}

async function createCatalog(title: string, marketId: string): Promise<string> {
  const capture = await captureMutation<CatalogCreateData>(
    `catalogCreate ${title}`,
    catalogCreateDocument,
    {
      input: {
        title,
        status: 'ACTIVE',
        context: { marketIds: [marketId] },
      },
    },
    'catalogCreate',
  );
  const id = nestedId(capture.response as ConformanceGraphqlResult<unknown>, 'catalogCreate', 'catalog');
  createdCatalogIds.push(id);
  return id;
}

async function createPriceList(name: string, catalogId: string, adjustmentValue: number): Promise<string> {
  const capture = await captureMutation<PriceListCreateData>(
    `priceListCreate ${name}`,
    priceListCreateDocument,
    {
      input: {
        name,
        currency: 'USD',
        catalogId,
        parent: {
          adjustment: { type: 'PERCENTAGE_DECREASE', value: adjustmentValue },
        },
      },
    },
    'priceListCreate',
  );
  const id = nestedId(capture.response as ConformanceGraphqlResult<unknown>, 'priceListCreate', 'priceList');
  createdPriceListIds.push(id);
  return id;
}

async function createWebPresence(subfolderSuffix: string): Promise<string> {
  const capture = await captureMutation<WebPresenceCreateData>(
    `webPresenceCreate ${subfolderSuffix}`,
    webPresenceCreateDocument,
    {
      input: {
        defaultLocale: 'en',
        alternateLocales: [],
        subfolderSuffix,
      },
    },
    'webPresenceCreate',
  );
  const id = nestedId(capture.response as ConformanceGraphqlResult<unknown>, 'webPresenceCreate', 'webPresence');
  createdWebPresenceIds.push(id);
  return id;
}

let captureFailure: unknown = null;
let baselineRead: CapturedCase<unknown> | null = null;

try {
  const preexistingWebPresenceSuffix = `har${randomLetters(10)}`;
  const preexistingMarketId = await createMarket(preexistingMarketName);
  const preexistingCatalogId = await createCatalog(preexistingCatalogTitle, preexistingMarketId);
  await createPriceList(preexistingPriceListName, preexistingCatalogId, 5);
  await createWebPresence(preexistingWebPresenceSuffix);

  const mixedReadVariables = {
    marketQuery: token,
    catalogQuery,
    marketsFirst: 4,
    catalogsFirst: 2,
    catalogsWindowFirst: 1,
    priceListsFirst: 250,
    webPresencesFirst: 250,
  };

  baselineRead = await captureWhen(
    'baseline mixed Markets-family read before staged delta',
    mixedReadDocument,
    mixedReadVariables,
    (response) =>
      mixedReadHasExpectedRows(response, {
        markets: [preexistingMarketName],
        catalogs: [preexistingCatalogTitle],
        priceLists: [preexistingPriceListName],
        webPresenceSuffix: preexistingWebPresenceSuffix,
        catalogCount: 1,
      }),
  );

  const stagedMarketId = await createMarket(stagedMarketName);
  const stagedCatalogId = await createCatalog(stagedCatalogTitle, stagedMarketId);
  const stagedPriceListId = await createPriceList(stagedPriceListName, stagedCatalogId, 10);

  await captureWhen('mixed Markets-family read after staged delta', mixedReadDocument, mixedReadVariables, (response) =>
    mixedReadHasExpectedRows(response, {
      markets: [preexistingMarketName, stagedMarketName],
      catalogs: [preexistingCatalogTitle, stagedCatalogTitle],
      priceLists: [preexistingPriceListName, stagedPriceListName],
      webPresenceSuffix: preexistingWebPresenceSuffix,
      catalogCount: 2,
    }),
  );

  const priceListRead = await captureCase('staged price list singular readback', priceListReadDocument, {
    id: stagedPriceListId,
  });
  assertNoGraphqlErrors(priceListRead.response, 'staged price list readback');
  cases.push(priceListRead);
} catch (error) {
  captureFailure = error;
} finally {
  for (const id of createdPriceListIds.slice().reverse()) {
    await cleanupId('priceList', id, priceListDeleteDocument);
  }
  for (const id of createdCatalogIds.slice().reverse()) {
    await cleanupId('catalog', id, catalogDeleteDocument);
  }
  for (const id of createdMarketIds.slice().reverse()) {
    await cleanupId('market', id, marketDeleteDocument);
  }
  for (const id of createdWebPresenceIds.slice().reverse()) {
    await cleanupId('webPresence', id, webPresenceDeleteDocument);
  }
}

if (captureFailure) {
  throw captureFailure;
}
if (!baselineRead) {
  throw new Error('Missing baseline mixed Markets-family read for upstreamCalls cassette.');
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scope: 'mixed Markets-family LiveHybrid baseline plus staged overlay read',
      token,
      cases,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'MarketsFamilyMixedOverlayRead',
          variables: baselineRead.variables,
          query: mixedReadDocument,
          response: {
            status: baselineRead.response.status,
            body: baselineRead.response.payload,
          },
        },
      ],
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
      token,
      cases: cases.map((entry) => ({ name: entry.name, status: entry.response.status })),
      cleanup: cleanup.map((entry) => ({ type: entry.type, id: entry.id, status: entry.response.status })),
    },
    null,
    2,
  ),
);
