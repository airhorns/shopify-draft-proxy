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

type CapturedCase<TData = unknown> = {
  name: string;
  query: string;
  request: { variables: Record<string, unknown> };
  response: ConformanceGraphqlResult<TData>;
};

type PayloadWithUserErrors = {
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
const outputPath = path.join(outputDir, 'delete-cascades-parity.json');

const productId = process.env['SHOPIFY_CONFORMANCE_PRODUCT_ID'] ?? 'gid://shopify/Product/9801098789170';
const variantId =
  process.env['SHOPIFY_CONFORMANCE_PRODUCT_VARIANT_ID'] ?? 'gid://shopify/ProductVariant/49875425296690';
const webPresencesFirst = 100;

const marketSetupReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-delete-cascade-setup-read.graphql'),
  'utf8',
);
const marketDeleteDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-delete-cascade-delete.graphql'),
  'utf8',
);
const marketAfterDeleteReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'market-delete-cascade-read.graphql'),
  'utf8',
);
const catalogSetupReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-delete-detaches-price-list-setup-read.graphql'),
  'utf8',
);
const catalogDeleteDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-delete-detaches-price-list-delete.graphql'),
  'utf8',
);
const catalogAfterDeleteReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-delete-detaches-price-list-read.graphql'),
  'utf8',
);
const priceListSetupReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-delete-clears-fixed-prices-setup-read.graphql'),
  'utf8',
);
const priceListDeleteDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-delete-clears-fixed-prices-delete.graphql'),
  'utf8',
);
const priceListAfterDeleteReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-delete-clears-fixed-prices-read.graphql'),
  'utf8',
);

const webPresenceCreateMutation = `#graphql
mutation DeleteCascadeWebPresenceCreate($input: WebPresenceCreateInput!) {
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
mutation DeleteCascadeWebPresenceCleanup($id: ID!) {
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

const marketCreateMutation = `#graphql
mutation DeleteCascadeMarketCreate($input: MarketCreateInput!) {
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

const marketDeleteCleanupMutation = `#graphql
mutation DeleteCascadeMarketCleanup($id: ID!) {
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

const priceListCreateMutation = `#graphql
mutation DeleteCascadePriceListCreate($input: PriceListCreateInput!) {
  priceListCreate(input: $input) {
    priceList {
      id
      name
      currency
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const priceListDeleteCleanupMutation = `#graphql
mutation DeleteCascadePriceListCleanup($id: ID!) {
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

const catalogCreateMutation = `#graphql
mutation DeleteCascadeCatalogCreate($input: CatalogCreateInput!) {
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

const catalogDeleteCleanupMutation = `#graphql
mutation DeleteCascadeCatalogCleanup($id: ID!) {
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

const fixedPricesAddMutation = `#graphql
mutation DeleteCascadeFixedPricesAdd($priceListId: ID!, $prices: [PriceListPriceInput!]!) {
  priceListFixedPricesAdd(priceListId: $priceListId, prices: $prices) {
    prices {
      price {
        amount
        currencyCode
      }
      compareAtPrice {
        amount
        currencyCode
      }
      originType
      variant {
        id
        sku
        product {
          id
          title
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

const fixedPricesDeleteMutation = `#graphql
mutation DeleteCascadeFixedPricesCleanup($priceListId: ID!, $variantIds: [ID!]!) {
  priceListFixedPricesDelete(priceListId: $priceListId, variantIds: $variantIds) {
    deletedFixedPriceVariantIds
    userErrors {
      field
      message
      code
    }
  }
}
`;

function randomLetters(length: number): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz';
  return Array.from({ length }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join('');
}

function userErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = (data as Record<string, unknown>)[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const errors = (payload as PayloadWithUserErrors).userErrors;
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string, label: string): void {
  const errors = userErrors(result, root);
  if (result.status !== 200 || result.payload.errors || errors.length > 0) {
    throw new Error(
      `${label} failed: status=${result.status} userErrors=${JSON.stringify(errors)} errors=${JSON.stringify(
        result.payload.errors ?? null,
      )}`,
    );
  }
}

function assertGraphqlOk<TData>(result: ConformanceGraphqlResult<TData>, label: string): void {
  if (result.status !== 200 || result.payload.errors) {
    throw new Error(`${label} failed: status=${result.status} errors=${JSON.stringify(result.payload.errors ?? null)}`);
  }
}

async function captureCase<TData>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase<TData>> {
  const response = await runGraphqlRequest<TData>(query, variables);
  assertGraphqlOk(response, name);
  return {
    name,
    query,
    request: { variables },
    response,
  };
}

function readCreatedId<TData>(
  result: ConformanceGraphqlResult<TData>,
  root: string,
  child: string,
  label: string,
): string {
  const data = result.payload.data;
  const payload = typeof data === 'object' && data !== null ? (data as Record<string, unknown>)[root] : null;
  const node = typeof payload === 'object' && payload !== null ? (payload as Record<string, unknown>)[child] : null;
  const id = typeof node === 'object' && node !== null ? (node as { id?: unknown }).id : null;
  if (typeof id !== 'string') {
    throw new Error(`${label} did not return an id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

async function createPriceList(label: string, currency = 'USD'): Promise<string> {
  const result = await runGraphqlRequest(priceListCreateMutation, {
    input: {
      name: label,
      currency,
      parent: {
        adjustment: {
          type: 'PERCENTAGE_DECREASE',
          value: 10,
        },
      },
    },
  });
  assertNoUserErrors(result, 'priceListCreate', label);
  return readCreatedId(result, 'priceListCreate', 'priceList', label);
}

async function createCatalog(title: string, marketId: string, priceListId?: string): Promise<string> {
  const input: Record<string, unknown> = {
    title,
    status: 'ACTIVE',
    context: {
      marketIds: [marketId],
    },
  };
  if (priceListId) input['priceListId'] = priceListId;
  const result = await runGraphqlRequest(catalogCreateMutation, { input });
  assertNoUserErrors(result, 'catalogCreate', title);
  return readCreatedId(result, 'catalogCreate', 'catalog', title);
}

async function createMarket(
  label: string,
  options: { webPresenceId?: string; countryCode?: string } = {},
): Promise<string> {
  const input: Record<string, unknown> = {
    name: label,
  };
  if (options.countryCode) {
    input['conditions'] = {
      regionsCondition: {
        regions: [{ countryCode: options.countryCode }],
      },
    };
  }
  if (options.webPresenceId) input['webPresences'] = [options.webPresenceId];
  const result = await runGraphqlRequest(marketCreateMutation, { input });
  assertNoUserErrors(result, 'marketCreate', label);
  return readCreatedId(result, 'marketCreate', 'market', label);
}

async function createWebPresence(suffix: string): Promise<string> {
  const result = await runGraphqlRequest(webPresenceCreateMutation, {
    input: {
      defaultLocale: 'en',
      subfolderSuffix: suffix,
    },
  });
  assertNoUserErrors(result, 'webPresenceCreate', `webPresenceCreate ${suffix}`);
  return readCreatedId(result, 'webPresenceCreate', 'webPresence', `webPresenceCreate ${suffix}`);
}

async function addFixedPrice(priceListId: string): Promise<ConformanceGraphqlResult<unknown>> {
  const result = await runGraphqlRequest(fixedPricesAddMutation, {
    priceListId,
    prices: [
      {
        variantId,
        price: {
          amount: '12.50',
          currencyCode: 'USD',
        },
        compareAtPrice: {
          amount: '14.00',
          currencyCode: 'USD',
        },
      },
    ],
  });
  assertNoUserErrors(result, 'priceListFixedPricesAdd', 'priceListFixedPricesAdd setup');
  return result;
}

async function cleanupFixedPrice(priceListId: string): Promise<ConformanceGraphqlResult<unknown>> {
  return runGraphqlRequest(fixedPricesDeleteMutation, {
    priceListId,
    variantIds: [variantId],
  });
}

function upstreamCall(entry: CapturedCase<unknown>) {
  return {
    operationName: operationName(entry.query),
    variables: entry.request.variables,
    query: entry.query,
    response: {
      status: entry.response.status,
      body: entry.response.payload,
    },
  };
}

function operationName(query: string): string {
  const match = /\b(?:query|mutation)\s+([_A-Za-z][_0-9A-Za-z]*)/u.exec(query);
  if (!match) throw new Error(`Unable to read operation name from query: ${query}`);
  return match[1];
}

const unique = randomLetters(10);
const liveSetup: Record<string, unknown> = {
  productId,
  variantId,
  marketCountryCode: 'BR',
};
const cleanup: Array<{
  type: 'catalog' | 'market' | 'priceList' | 'webPresence' | 'fixedPrice';
  id: string;
  response: ConformanceGraphqlResult<unknown>;
}> = [];

const createdCatalogIds: string[] = [];
const createdMarketIds: string[] = [];
const createdPriceListIds: string[] = [];
const createdWebPresenceIds: string[] = [];

let marketScenario: Record<string, unknown> | null = null;
let catalogScenario: Record<string, unknown> | null = null;
let priceListScenario: Record<string, unknown> | null = null;

try {
  const marketWebPresenceId = await createWebPresence(`h${unique}`);
  createdWebPresenceIds.push(marketWebPresenceId);
  const marketId = await createMarket(`Delete Cascade ${unique}`, {
    webPresenceId: marketWebPresenceId,
    countryCode: 'BR',
  });
  createdMarketIds.push(marketId);
  const marketCatalogId = await createCatalog(`Delete Cascade Catalog ${unique}`, marketId);
  createdCatalogIds.push(marketCatalogId);
  const marketSetupRead = await captureCase('market delete cascade setup read', marketSetupReadDocument, {
    marketId,
    catalogId: marketCatalogId,
    webPresencesFirst,
  });
  const marketDelete = await captureCase('market delete cascade delete', marketDeleteDocument, { id: marketId });
  assertNoUserErrors(marketDelete.response, 'marketDelete', 'marketDelete cascade');
  createdMarketIds.splice(createdMarketIds.indexOf(marketId), 1);
  createdWebPresenceIds.splice(createdWebPresenceIds.indexOf(marketWebPresenceId), 1);
  const marketAfterDeleteRead = await captureCase(
    'market delete cascade downstream read',
    marketAfterDeleteReadDocument,
    {
      marketId,
      catalogId: marketCatalogId,
      webPresencesFirst,
    },
  );
  marketScenario = {
    setupRead: marketSetupRead,
    delete: marketDelete,
    downstreamRead: marketAfterDeleteRead,
  };

  const catalogMarketId = await createMarket(`Catalog Delete Cascade ${unique}`);
  createdMarketIds.push(catalogMarketId);
  const catalogPriceListId = await createPriceList(`Catalog Delete Cascade ${unique}`);
  createdPriceListIds.push(catalogPriceListId);
  const catalogId = await createCatalog(`Catalog Delete Cascade ${unique}`, catalogMarketId, catalogPriceListId);
  createdCatalogIds.push(catalogId);
  const catalogSetupRead = await captureCase('catalog delete setup read', catalogSetupReadDocument, {
    catalogId,
    priceListId: catalogPriceListId,
  });
  const catalogDelete = await captureCase('catalog delete detaches price list', catalogDeleteDocument, {
    id: catalogId,
  });
  assertNoUserErrors(catalogDelete.response, 'catalogDelete', 'catalogDelete cascade');
  createdCatalogIds.splice(createdCatalogIds.indexOf(catalogId), 1);
  const catalogAfterDeleteRead = await captureCase('catalog delete downstream read', catalogAfterDeleteReadDocument, {
    catalogId,
    priceListId: catalogPriceListId,
  });
  catalogScenario = {
    setupRead: catalogSetupRead,
    delete: catalogDelete,
    downstreamRead: catalogAfterDeleteRead,
  };

  const priceListMarketId = await createMarket(`Price List Delete Cascade ${unique}`);
  createdMarketIds.push(priceListMarketId);
  const priceListId = await createPriceList(`Price List Delete Cascade ${unique}`);
  createdPriceListIds.push(priceListId);
  const priceListCatalogId = await createCatalog(`Price List Delete Cascade ${unique}`, priceListMarketId, priceListId);
  createdCatalogIds.push(priceListCatalogId);
  const fixedPriceSetup = await addFixedPrice(priceListId);
  const priceListSetupRead = await captureCase('price list delete setup read', priceListSetupReadDocument, {
    catalogId: priceListCatalogId,
    priceListId,
  });
  const priceListDelete = await captureCase('price list delete clears fixed prices', priceListDeleteDocument, {
    id: priceListId,
  });
  assertNoUserErrors(priceListDelete.response, 'priceListDelete', 'priceListDelete cascade');
  createdPriceListIds.splice(createdPriceListIds.indexOf(priceListId), 1);
  const priceListAfterDeleteRead = await captureCase(
    'price list delete downstream read',
    priceListAfterDeleteReadDocument,
    {
      catalogId: priceListCatalogId,
      priceListId,
    },
  );
  priceListScenario = {
    fixedPriceSetup: {
      query: fixedPricesAddMutation,
      request: { variables: { priceListId, variantId, productId } },
      response: fixedPriceSetup,
    },
    setupRead: priceListSetupRead,
    delete: priceListDelete,
    downstreamRead: priceListAfterDeleteRead,
  };
} finally {
  for (const id of createdCatalogIds.toReversed()) {
    cleanup.push({ type: 'catalog', id, response: await runGraphqlRequest(catalogDeleteCleanupMutation, { id }) });
  }
  for (const id of createdPriceListIds.toReversed()) {
    cleanup.push({ type: 'fixedPrice', id, response: await cleanupFixedPrice(id) });
    cleanup.push({ type: 'priceList', id, response: await runGraphqlRequest(priceListDeleteCleanupMutation, { id }) });
  }
  for (const id of createdMarketIds.toReversed()) {
    cleanup.push({ type: 'market', id, response: await runGraphqlRequest(marketDeleteCleanupMutation, { id }) });
  }
  for (const id of createdWebPresenceIds.toReversed()) {
    cleanup.push({ type: 'webPresence', id, response: await runGraphqlRequest(webPresenceDeleteMutation, { id }) });
  }
}

if (!marketScenario || !catalogScenario || !priceListScenario) {
  throw new Error('Delete cascade capture did not complete every scenario.');
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scope: 'Markets delete cascades for marketDelete, catalogDelete, and priceListDelete',
  liveSetup,
  marketDeleteCascade: marketScenario,
  catalogDeleteDetachPriceList: catalogScenario,
  priceListDeleteClearFixedPrices: priceListScenario,
  cleanup,
  upstreamCalls: [
    upstreamCall((marketScenario as { setupRead: CapturedCase }).setupRead),
    upstreamCall((catalogScenario as { setupRead: CapturedCase }).setupRead),
    upstreamCall((priceListScenario as { setupRead: CapturedCase }).setupRead),
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain,
      apiVersion,
      scenarioKeys: ['marketDeleteCascade', 'catalogDeleteDetachPriceList', 'priceListDeleteClearFixedPrices'],
      cleanup: cleanup.map((entry) => ({ type: entry.type, id: entry.id, status: entry.response.status })),
    },
    null,
    2,
  ),
);
