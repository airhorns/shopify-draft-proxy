/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase<TData = unknown> = {
  name: string;
  query: string;
  request: { variables: Record<string, unknown> };
  response: ConformanceGraphqlResult<TData>;
};

type RecordedUpstreamCall = {
  operationName: string;
  query: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    body: unknown;
  };
};

type JsonRecord = Record<string, unknown>;

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
const outputPath = path.join(outputDir, 'market-mutation-first-hydration.json');

const preflightQuery = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-mutation-targets-hydrate.graphql'),
  'utf8',
);
const updateMutation = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-mutation-first-update.graphql'),
  'utf8',
);
const updateReadQuery = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-mutation-first-update-read.graphql'),
  'utf8',
);
const updateTopLevelReadQuery = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-mutation-first-update-top-level-read.graphql'),
  'utf8',
);
const deleteMutation = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-delete-cascade-delete.graphql'),
  'utf8',
);
const deleteReadQuery = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-delete-cascade-read.graphql'),
  'utf8',
);
const updateUnknownMutation = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-update-unknown-id-validation.graphql'),
  'utf8',
);
const deleteUnknownMutation = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-delete-unknown-id-validation.graphql'),
  'utf8',
);

const marketCreateMutation = `#graphql
mutation MutationFirstMarketCreate($input: MarketCreateInput!) {
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

const catalogCreateMutation = `#graphql
mutation MutationFirstCatalogCreate($input: CatalogCreateInput!) {
  catalogCreate(input: $input) {
    catalog {
      id
      title
      status
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const catalogDeleteMutation = `#graphql
mutation MutationFirstCatalogCleanup($id: ID!) {
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

const priceListCreateMutation = `#graphql
mutation MutationFirstPriceListCreate($input: PriceListCreateInput!) {
  priceListCreate(input: $input) {
    priceList {
      id
      name
      currency
      catalog {
        id
        title
        status
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const priceListDeleteMutation = `#graphql
mutation MutationFirstPriceListCleanup($id: ID!) {
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

const webPresenceCreateMutation = `#graphql
mutation MutationFirstWebPresenceCreate($input: WebPresenceCreateInput!) {
  webPresenceCreate(input: $input) {
    webPresence {
      id
      subfolderSuffix
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const webPresenceDeleteMutation = `#graphql
mutation MutationFirstWebPresenceCleanup($id: ID!) {
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

const marketLinkWebPresenceMutation = `#graphql
mutation MutationFirstMarketLinkWebPresence($id: ID!, $input: MarketUpdateInput!) {
  marketUpdate(id: $id, input: $input) {
    market {
      id
      webPresences(first: 10) {
        nodes {
          id
          subfolderSuffix
        }
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const primaryMarketProbeQuery = `#graphql
query MutationFirstPrimaryMarketProbe {
  primaryMarket {
    id
    name
    handle
    status
    type
  }
}
`;

const wrongResourceProductId = process.env['SHOPIFY_CONFORMANCE_PRODUCT_ID'] ?? 'gid://shopify/Product/9801098789170';
const unknownMarketId = 'gid://shopify/Market/999999999999';
const unique = Date.now().toString(36);
const letterUnique = unique.replace(/[0-9]/gu, (digit) => 'abcdefghij'[Number(digit)] ?? 'a');

async function captureCase<TData = unknown>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase<TData>> {
  const response = await runGraphqlRequest<TData>(query, variables);
  return {
    name,
    query,
    request: { variables },
    response,
  };
}

function rootPayload(response: ConformanceGraphqlResult<unknown>, root: string): JsonRecord | null {
  const data = response.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) return null;
  const payload = (data as JsonRecord)[root];
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) return null;
  return payload as JsonRecord;
}

function userErrors(response: ConformanceGraphqlResult<unknown>, root: string): unknown[] {
  const payload = rootPayload(response, root);
  const errors = payload?.['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors(response: ConformanceGraphqlResult<unknown>, root: string, label: string): void {
  const errors = userErrors(response, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function readPayloadId(
  response: ConformanceGraphqlResult<unknown>,
  root: string,
  objectKey: string,
  label: string,
): string {
  const payload = rootPayload(response, root);
  const object = payload?.[objectKey];
  const id =
    typeof object === 'object' && object !== null && !Array.isArray(object) ? (object as JsonRecord)['id'] : null;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return ${objectKey}.id: ${JSON.stringify(response.payload)}`);
  }
  return id;
}

function responseData(response: ConformanceGraphqlResult<unknown>, label: string): JsonRecord {
  const data = response.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw new Error(`${label} did not return object data: ${JSON.stringify(response.payload)}`);
  }
  return data as JsonRecord;
}

function resourceIdTail(id: string): string {
  return id.split('/').pop() ?? id;
}

function nodesAt(data: JsonRecord, key: string): JsonRecord[] {
  const root = data[key];
  if (typeof root !== 'object' || root === null || Array.isArray(root)) return [];
  const nodes = (root as JsonRecord)['nodes'];
  return Array.isArray(nodes)
    ? nodes.filter((node): node is JsonRecord => typeof node === 'object' && node !== null && !Array.isArray(node))
    : [];
}

function captureAsUpstreamCall(operationName: string, capture: CapturedCase<unknown>): RecordedUpstreamCall {
  return {
    operationName,
    query: capture.query,
    variables: capture.request.variables,
    response: {
      status: capture.response.status,
      body: capture.response.payload,
    },
  };
}

function assertTopLevelUpdateRead(
  response: ConformanceGraphqlResult<unknown>,
  expected: {
    primaryMarketId: string;
    updateMarketId: string;
    otherMarketId: string;
    updateCatalogId: string;
    otherCatalogId: string;
    updatePriceListId: string;
    otherPriceListId: string;
    updateWebPresenceId: string;
    otherWebPresenceId: string;
  },
): void {
  const data = responseData(response, 'mutation-first update top-level read');
  const primaryMarkets = nodesAt(data, 'primaryMarketFromList');
  if (!primaryMarkets.some((market) => market['id'] === expected.primaryMarketId)) {
    throw new Error(`top-level markets primary lookup missed ${expected.primaryMarketId}: ${JSON.stringify(data)}`);
  }
  if (!nodesAt(data, 'updateMarketFromList').some((market) => market['id'] === expected.updateMarketId)) {
    throw new Error(`top-level markets update lookup missed ${expected.updateMarketId}: ${JSON.stringify(data)}`);
  }
  if (!nodesAt(data, 'otherMarketFromList').some((market) => market['id'] === expected.otherMarketId)) {
    throw new Error(`top-level markets other lookup missed ${expected.otherMarketId}: ${JSON.stringify(data)}`);
  }
  const catalogIds = new Set(nodesAt(data, 'catalogs').map((catalog) => catalog['id']));
  for (const id of [expected.updateCatalogId, expected.otherCatalogId]) {
    if (!catalogIds.has(id)) {
      throw new Error(`top-level catalogs query missed ${id}: ${JSON.stringify(data)}`);
    }
  }
  const catalogsCount = data['catalogsCount'];
  if (
    typeof catalogsCount !== 'object' ||
    catalogsCount === null ||
    Array.isArray(catalogsCount) ||
    typeof (catalogsCount as JsonRecord)['count'] !== 'number' ||
    ((catalogsCount as JsonRecord)['count'] as number) < 2
  ) {
    throw new Error(`top-level catalogsCount did not include the disposable catalogs: ${JSON.stringify(data)}`);
  }
  if (!nodesAt(data, 'catalogs').some((catalog) => catalog['id'] === expected.updateCatalogId)) {
    throw new Error(`top-level catalogs update lookup missed ${expected.updateCatalogId}: ${JSON.stringify(data)}`);
  }
  if (!nodesAt(data, 'catalogs').some((catalog) => catalog['id'] === expected.otherCatalogId)) {
    throw new Error(`top-level catalogs other lookup missed ${expected.otherCatalogId}: ${JSON.stringify(data)}`);
  }
  const priceLists = nodesAt(data, 'priceLists');
  for (const id of [expected.updatePriceListId, expected.otherPriceListId]) {
    const node = priceLists.find((priceList) => priceList['id'] === id);
    if (
      !node ||
      typeof node['name'] !== 'string' ||
      typeof node['currency'] !== 'string' ||
      typeof node['catalog'] !== 'object' ||
      node['catalog'] === null
    ) {
      throw new Error(`top-level priceLists did not include fully shaped ${id}: ${JSON.stringify(data)}`);
    }
  }
  const webPresenceIds = new Set(nodesAt(data, 'webPresences').map((presence) => presence['id']));
  for (const id of [expected.updateWebPresenceId, expected.otherWebPresenceId]) {
    if (!webPresenceIds.has(id)) {
      throw new Error(`top-level webPresences query missed ${id}: ${JSON.stringify(data)}`);
    }
  }
}

async function capturePreflight(name: string, ids: string[]): Promise<RecordedUpstreamCall> {
  const variables = { ids };
  const response = await runGraphqlRequest(preflightQuery, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${name} preflight failed: ${JSON.stringify(response.payload)}`);
  }
  return {
    operationName: 'MarketsMutationPreflightHydrate',
    query: preflightQuery,
    variables,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

async function createRelatedMarket(kind: string): Promise<{
  marketId: string;
  catalogId: string;
  priceListId: string;
  webPresenceId: string;
  cases: Array<CapturedCase<unknown>>;
}> {
  const cases: Array<CapturedCase<unknown>> = [];
  let marketId: string | null = null;
  let catalogId: string | null = null;
  let priceListId: string | null = null;
  let webPresenceId: string | null = null;
  try {
    const market = await captureCase('setup market create', marketCreateMutation, {
      input: {
        name: `Mutation First ${kind} ${unique}`,
      },
    });
    assertNoUserErrors(market.response, 'marketCreate', `${kind} marketCreate`);
    marketId = readPayloadId(market.response, 'marketCreate', 'market', `${kind} marketCreate`);
    cases.push(market);

    const catalog = await captureCase('setup catalog create', catalogCreateMutation, {
      input: {
        title: `Mutation First ${kind} Catalog ${unique}`,
        status: 'ACTIVE',
        context: {
          marketIds: [marketId],
        },
      },
    });
    assertNoUserErrors(catalog.response, 'catalogCreate', `${kind} catalogCreate`);
    catalogId = readPayloadId(catalog.response, 'catalogCreate', 'catalog', `${kind} catalogCreate`);
    cases.push(catalog);

    const priceList = await captureCase('setup price list create', priceListCreateMutation, {
      input: {
        name: `Mutation First ${kind} Prices ${unique}`,
        currency: 'USD',
        catalogId,
        parent: {
          adjustment: {
            type: 'PERCENTAGE_DECREASE',
            value: 10,
          },
        },
      },
    });
    assertNoUserErrors(priceList.response, 'priceListCreate', `${kind} priceListCreate`);
    priceListId = readPayloadId(priceList.response, 'priceListCreate', 'priceList', `${kind} priceListCreate`);
    cases.push(priceList);

    const webPresence = await captureCase('setup web presence create', webPresenceCreateMutation, {
      input: {
        defaultLocale: 'en',
        alternateLocales: [],
        subfolderSuffix: `${kind.toLowerCase()}${letterUnique}`,
      },
    });
    assertNoUserErrors(webPresence.response, 'webPresenceCreate', `${kind} webPresenceCreate`);
    webPresenceId = readPayloadId(
      webPresence.response,
      'webPresenceCreate',
      'webPresence',
      `${kind} webPresenceCreate`,
    );
    cases.push(webPresence);

    const link = await captureCase('setup market web presence link', marketLinkWebPresenceMutation, {
      id: marketId,
      input: {
        webPresencesToAdd: [webPresenceId],
      },
    });
    assertNoUserErrors(link.response, 'marketUpdate', `${kind} webPresence link`);
    cases.push(link);

    return {
      marketId,
      catalogId,
      priceListId,
      webPresenceId,
      cases,
    };
  } catch (error) {
    await Promise.allSettled([
      cleanupPriceList(priceListId),
      cleanupCatalog(catalogId),
      cleanupWebPresence(webPresenceId),
      cleanupMarket(marketId),
    ]);
    throw error;
  }
}

async function cleanupMarket(id: string | null): Promise<ConformanceGraphqlResult<unknown> | null> {
  if (!id) return null;
  return runGraphqlRequest(deleteMutation, { id });
}

async function cleanupCatalog(id: string | null): Promise<ConformanceGraphqlResult<unknown> | null> {
  if (!id) return null;
  return runGraphqlRequest(catalogDeleteMutation, { id });
}

async function cleanupPriceList(id: string | null): Promise<ConformanceGraphqlResult<unknown> | null> {
  if (!id) return null;
  return runGraphqlRequest(priceListDeleteMutation, { id });
}

async function cleanupWebPresence(id: string | null): Promise<ConformanceGraphqlResult<unknown> | null> {
  if (!id) return null;
  return runGraphqlRequest(webPresenceDeleteMutation, { id });
}

let updateMarketId: string | null = null;
let updateCatalogId: string | null = null;
let updatePriceListId: string | null = null;
let updateWebPresenceId: string | null = null;
let otherMarketId: string | null = null;
let otherCatalogId: string | null = null;
let otherPriceListId: string | null = null;
let otherWebPresenceId: string | null = null;
let deleteMarketId: string | null = null;
let deleteCatalogId: string | null = null;
let deletePriceListId: string | null = null;
let deleteWebPresenceId: string | null = null;
const upstreamCalls: RecordedUpstreamCall[] = [];
const cleanup: JsonRecord = {};

try {
  const primaryMarketProbe = await captureCase('primary market probe', primaryMarketProbeQuery, {});
  const primaryMarket = responseData(primaryMarketProbe.response, 'primary market probe')['primaryMarket'];
  const primaryMarketId =
    typeof primaryMarket === 'object' && primaryMarket !== null && !Array.isArray(primaryMarket)
      ? (primaryMarket as JsonRecord)['id']
      : null;
  if (typeof primaryMarketId !== 'string' || primaryMarketId.length === 0) {
    throw new Error(
      `primary market probe did not return an id: ${JSON.stringify(primaryMarketProbe.response.payload)}`,
    );
  }

  const updateSetup = await createRelatedMarket('Update');
  updateMarketId = updateSetup.marketId;
  updateCatalogId = updateSetup.catalogId;
  updatePriceListId = updateSetup.priceListId;
  updateWebPresenceId = updateSetup.webPresenceId;
  const otherSetup = await createRelatedMarket('Other');
  otherMarketId = otherSetup.marketId;
  otherCatalogId = otherSetup.catalogId;
  otherPriceListId = otherSetup.priceListId;
  otherWebPresenceId = otherSetup.webPresenceId;

  await sleep(5000);

  const updatePreflight = await capturePreflight('mutation-first update', [updateMarketId]);
  upstreamCalls.push(updatePreflight);
  const update = await captureCase('mutation-first marketUpdate', updateMutation, {
    id: updateMarketId,
    input: {
      name: `Mutation First Update Renamed ${unique}`,
      handle: `mutation-first-update-renamed-${unique}`,
    },
  });
  assertNoUserErrors(update.response, 'marketUpdate', 'mutation-first marketUpdate');
  const updateRead = await captureCase('mutation-first update downstream read', updateReadQuery, {
    marketId: updateMarketId,
    catalogId: updateCatalogId,
  });
  const updateTopLevelRead = await captureCase('mutation-first update top-level read', updateTopLevelReadQuery, {
    primaryMarketQuery: `id:${resourceIdTail(primaryMarketId)}`,
    updateMarketQuery: `id:${resourceIdTail(updateMarketId)}`,
    otherMarketQuery: `id:${resourceIdTail(otherMarketId)}`,
    catalogsFirst: 100,
    priceListsFirst: 100,
    webPresencesFirst: 100,
  });
  assertTopLevelUpdateRead(updateTopLevelRead.response, {
    primaryMarketId,
    updateMarketId,
    otherMarketId,
    updateCatalogId,
    otherCatalogId,
    updatePriceListId,
    otherPriceListId,
    updateWebPresenceId,
    otherWebPresenceId,
  });
  upstreamCalls.push(captureAsUpstreamCall('MarketMutationFirstUpdateTopLevelRead', updateTopLevelRead));

  const deleteSetup = await createRelatedMarket('Delete');
  deleteMarketId = deleteSetup.marketId;
  deleteCatalogId = deleteSetup.catalogId;
  deletePriceListId = deleteSetup.priceListId;
  deleteWebPresenceId = deleteSetup.webPresenceId;

  const deletePreflight = await capturePreflight('mutation-first delete', [deleteMarketId]);
  upstreamCalls.push(deletePreflight);
  const marketDelete = await captureCase('mutation-first marketDelete', deleteMutation, {
    id: deleteMarketId,
  });
  assertNoUserErrors(marketDelete.response, 'marketDelete', 'mutation-first marketDelete');
  const deleteRead = await captureCase('mutation-first delete downstream read', deleteReadQuery, {
    marketId: deleteMarketId,
    catalogId: deleteCatalogId,
    webPresencesFirst: 100,
  });

  upstreamCalls.push(await capturePreflight('unknown update validation', [unknownMarketId]));
  const updateUnknown = await captureCase('marketUpdate unknown id validation', updateUnknownMutation, {
    id: unknownMarketId,
    input: {
      name: 'Nope',
    },
  });
  upstreamCalls.push(await capturePreflight('unknown delete validation', [unknownMarketId]));
  const deleteUnknown = await captureCase('marketDelete unknown id validation', deleteUnknownMutation, {
    id: unknownMarketId,
  });
  const updateWrongResource = await captureCase('marketUpdate wrong resource id validation', updateMutation, {
    id: wrongResourceProductId,
    input: {
      name: 'Nope',
    },
  });
  const deleteWrongResource = await captureCase('marketDelete wrong resource id validation', deleteMutation, {
    id: wrongResourceProductId,
  });

  cleanup['updatePriceListDelete'] = await cleanupPriceList(updatePriceListId);
  updatePriceListId = null;
  cleanup['updateCatalogDelete'] = await cleanupCatalog(updateCatalogId);
  updateCatalogId = null;
  cleanup['updateWebPresenceDelete'] = await cleanupWebPresence(updateWebPresenceId);
  updateWebPresenceId = null;
  cleanup['updateMarketDelete'] = await cleanupMarket(updateMarketId);
  updateMarketId = null;
  cleanup['otherPriceListDelete'] = await cleanupPriceList(otherPriceListId);
  otherPriceListId = null;
  cleanup['otherCatalogDelete'] = await cleanupCatalog(otherCatalogId);
  otherCatalogId = null;
  cleanup['otherWebPresenceDelete'] = await cleanupWebPresence(otherWebPresenceId);
  otherWebPresenceId = null;
  cleanup['otherMarketDelete'] = await cleanupMarket(otherMarketId);
  otherMarketId = null;
  cleanup['deletePriceListDelete'] = await cleanupPriceList(deletePriceListId);
  deletePriceListId = null;
  cleanup['deleteCatalogDelete'] = await cleanupCatalog(deleteCatalogId);
  deleteCatalogId = null;
  cleanup['deleteWebPresenceDelete'] = await cleanupWebPresence(deleteWebPresenceId);
  deleteWebPresenceId = null;

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        scope:
          'Markets mutation-first hydration for existing marketUpdate and marketDelete targets, with catalog/price-list/web-presence relations, top-level list/count readback, and unknown-id validation.',
        liveSetup: {
          setupCreatesDisposableNonPrimaryMarkets: true,
          primaryMarketId,
          updateMarketId: updateSetup.marketId,
          updateCatalogId: updateSetup.catalogId,
          updatePriceListId: updateSetup.priceListId,
          updateWebPresenceId: updateSetup.webPresenceId,
          otherMarketId: otherSetup.marketId,
          otherCatalogId: otherSetup.catalogId,
          otherPriceListId: otherSetup.priceListId,
          otherWebPresenceId: otherSetup.webPresenceId,
          deleteMarketId: deleteSetup.marketId,
          deleteCatalogId: deleteSetup.catalogId,
          deletePriceListId: deleteSetup.priceListId,
          deleteWebPresenceId: deleteSetup.webPresenceId,
          unknownMarketId,
          wrongResourceProductId,
        },
        mutationFirstUpdate: {
          primaryMarketProbe,
          setup: updateSetup.cases,
          otherSetup: otherSetup.cases,
          preflight: updatePreflight,
          update,
          downstreamRead: updateRead,
          topLevelRead: updateTopLevelRead,
        },
        mutationFirstDelete: {
          setup: deleteSetup.cases,
          preflight: deletePreflight,
          delete: marketDelete,
          downstreamRead: deleteRead,
        },
        validation: {
          updateUnknown,
          deleteUnknown,
          updateWrongResource,
          deleteWrongResource,
        },
        cleanup,
        upstreamCalls,
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
        updateMarketId: updateSetup.marketId,
        otherMarketId: otherSetup.marketId,
        deleteMarketId: deleteSetup.marketId,
        upstreamCalls: upstreamCalls.length,
      },
      null,
      2,
    ),
  );
} finally {
  const finalCleanup = await Promise.allSettled([
    cleanupPriceList(updatePriceListId),
    cleanupCatalog(updateCatalogId),
    cleanupWebPresence(updateWebPresenceId),
    cleanupMarket(updateMarketId),
    cleanupPriceList(otherPriceListId),
    cleanupCatalog(otherCatalogId),
    cleanupWebPresence(otherWebPresenceId),
    cleanupMarket(otherMarketId),
    cleanupPriceList(deletePriceListId),
    cleanupCatalog(deleteCatalogId),
    cleanupWebPresence(deleteWebPresenceId),
    cleanupMarket(deleteMarketId),
  ]);
  const rejected = finalCleanup.filter((result) => result.status === 'rejected');
  if (rejected.length > 0) {
    console.error(JSON.stringify({ cleanupWarnings: rejected.map((result) => String(result.reason)) }, null, 2));
  }
}
