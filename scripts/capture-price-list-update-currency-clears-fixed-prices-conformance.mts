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
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<TData>;
};

type Connection<TNode> = {
  nodes?: Array<TNode | null> | null;
  edges?: Array<{ node?: TNode | null } | null> | null;
};

type MarketNode = {
  id?: string | null;
};

type ProductVariantNode = {
  id?: string | null;
  title?: string | null;
  sku?: string | null;
};

type ProductNode = {
  id?: string | null;
  title?: string | null;
  variants?: Connection<ProductVariantNode> | null;
};

type ProductDiscoveryData = {
  products?: Connection<ProductNode> | null;
};

type MarketsReadData = {
  markets?: {
    nodes?: Array<MarketNode | null> | null;
  } | null;
};

type PriceListCreateData = {
  priceListCreate?: {
    priceList?: { id?: string; currency?: string } | null;
    userErrors?: UserError[] | null;
  } | null;
};

type CatalogCreateData = {
  catalogCreate?: {
    catalog?: { id?: string } | null;
    userErrors?: UserError[] | null;
  } | null;
};

type PriceListUpdateData = {
  priceListUpdate?: {
    priceList?: {
      id?: string;
      currency?: string;
      fixedPricesCount?: number;
      prices?: { edges?: unknown[] | null } | null;
      catalog?: { id?: string } | null;
    } | null;
    userErrors?: UserError[] | null;
  } | null;
};

type PriceListReadData = {
  priceList?: {
    id?: string;
    currency?: string;
    fixedPricesCount?: number;
    prices?: { edges?: unknown[] | null } | null;
    catalog?: { id?: string } | null;
  } | null;
  catalog?: {
    id?: string;
    priceList?: {
      id?: string;
      currency?: string;
      fixedPricesCount?: number;
    } | null;
  } | null;
};

type FixedPricesAddData = {
  priceListFixedPricesAdd?: {
    prices?: unknown[] | null;
    userErrors?: UserError[] | null;
  } | null;
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
const outputPath = path.join(outputDir, 'price-list-update-currency-clears-fixed-prices.json');

const marketsReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-relation-markets-read.graphql'),
  'utf8',
);
const variantHydrateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-update-currency-fixed-prices-hydrate.graphql'),
  'utf8',
);
const priceListCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-create-catalog-validation.graphql'),
  'utf8',
);
const catalogCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-create-relation-validation.graphql'),
  'utf8',
);
const fixedPricesAddDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-fixed-prices-add.graphql'),
  'utf8',
);
const priceListUpdateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-update-currency-fixed-prices.graphql'),
  'utf8',
);
const readbackDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-update-currency-fixed-prices-read.graphql'),
  'utf8',
);

const productDiscoveryDocument = `#graphql
query PriceListUpdateCurrencyFixedPricesProductDiscovery($first: Int!) {
  products(first: $first) {
    nodes {
      id
      title
      variants(first: 5) {
        nodes {
          id
          title
          sku
        }
      }
    }
  }
}
`;

const catalogDeleteDocument = `#graphql
mutation PriceListUpdateCurrencyFixedPricesCatalogCleanup($id: ID!) {
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
mutation PriceListUpdateCurrencyFixedPricesPriceListCleanup($id: ID!) {
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

function connectionNodes<TNode>(connection: Connection<TNode> | null | undefined): TNode[] {
  const nodes = connection?.nodes ?? connection?.edges?.map((edge) => edge?.node ?? null) ?? [];
  return nodes.filter((node): node is TNode => node !== null && node !== undefined);
}

function assertGraphqlOk<TData>(label: string, result: ConformanceGraphqlResult<TData>): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function userErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = (data as Record<string, unknown>)[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const errors = (payload as { userErrors?: UserError[] | null }).userErrors;
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string, label: string): void {
  assertGraphqlOk(label, result);
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function firstMarketId(result: ConformanceGraphqlResult<MarketsReadData>): string {
  const id = result.payload.data?.markets?.nodes?.find((market) => typeof market?.id === 'string')?.id;
  if (typeof id !== 'string') {
    throw new Error(`markets read did not return a market id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function firstTwoVariantIds(result: ConformanceGraphqlResult<ProductDiscoveryData>): string[] {
  const variants = connectionNodes(result.payload.data?.products)
    .flatMap((product) => connectionNodes(product.variants))
    .map((variant) => variant.id)
    .filter((id): id is string => typeof id === 'string');
  const unique = [...new Set(variants)];
  if (unique.length < 2) {
    throw new Error(`Need at least two product variants for fixed-price capture: ${JSON.stringify(result.payload)}`);
  }
  return unique.slice(0, 2);
}

function priceListId(result: ConformanceGraphqlResult<PriceListCreateData>): string {
  const id = result.payload.data?.priceListCreate?.priceList?.id;
  if (typeof id !== 'string') {
    throw new Error(`priceListCreate did not return an id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function catalogId(result: ConformanceGraphqlResult<CatalogCreateData>): string {
  const id = result.payload.data?.catalogCreate?.catalog?.id;
  if (typeof id !== 'string') {
    throw new Error(`catalogCreate did not return an id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function priceEdges(value: { prices?: { edges?: unknown[] | null } | null } | null | undefined): unknown[] {
  return value?.prices?.edges ?? [];
}

function assertFixedPriceCount(
  label: string,
  priceList: { fixedPricesCount?: number; prices?: { edges?: unknown[] | null } | null } | null | undefined,
  expected: number,
): void {
  if (priceList?.fixedPricesCount !== expected || priceEdges(priceList).length !== expected) {
    throw new Error(`${label} expected ${expected} fixed prices: ${JSON.stringify(priceList)}`);
  }
}

function assertCatalogRelation<TData extends PriceListReadData | PriceListUpdateData>(
  label: string,
  result: ConformanceGraphqlResult<TData>,
  expectedCatalogId: string,
): void {
  const data = result.payload.data;
  const priceList =
    data && 'priceListUpdate' in data
      ? (data.priceListUpdate?.priceList ?? null)
      : 'priceList' in (data ?? {})
        ? ((data as PriceListReadData).priceList ?? null)
        : null;
  if (priceList?.catalog?.id !== expectedCatalogId) {
    throw new Error(`${label} did not retain catalog relation: ${JSON.stringify(result.payload)}`);
  }
}

function assertRejectedUpdatePreservedPrices(result: ConformanceGraphqlResult<PriceListUpdateData>): void {
  const errors = userErrors(result, 'priceListUpdate');
  if (!errors.some((error) => error.code === 'INVALID_ADJUSTMENT_VALUE')) {
    throw new Error(`rejected update did not return INVALID_ADJUSTMENT_VALUE: ${JSON.stringify(result.payload)}`);
  }
  assertFixedPriceCount('rejected update', result.payload.data?.priceListUpdate?.priceList, 2);
}

async function captureCase<TData>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase<TData>> {
  const response = await runGraphqlRequest<TData>(query, variables);
  return { name, query, variables, response };
}

function alternateCurrency(currency: string): string {
  return currency === 'USD' ? 'CAD' : 'USD';
}

const initialCurrency = 'USD';
const changedCurrency = alternateCurrency(initialCurrency);
const unique = Date.now().toString(36);
const createdCatalogIds: string[] = [];
const createdPriceListIds: string[] = [];
const cases: Array<CapturedCase<unknown>> = [];
const cleanup: Array<{
  type: 'catalog' | 'priceList';
  id: string;
  response: ConformanceGraphqlResult;
}> = [];

const productDiscoveryVariables = { first: 20 };
const productDiscovery = await captureCase<ProductDiscoveryData>(
  'product variant discovery for price-list currency fixed-price capture',
  productDiscoveryDocument,
  productDiscoveryVariables,
);
assertGraphqlOk('product variant discovery', productDiscovery.response);
const variantIds = firstTwoVariantIds(productDiscovery.response);

const marketsReadVariables = { first: 1 };
const marketsRead = await captureCase<MarketsReadData>(
  'markets read for price-list currency fixed-price capture',
  marketsReadDocument,
  marketsReadVariables,
);
assertGraphqlOk('markets read', marketsRead.response);
const marketId = firstMarketId(marketsRead.response);
cases.push(marketsRead);

try {
  const hydrate = await captureCase('variant hydrate for local fixed-price replay', variantHydrateDocument, {
    variantIds,
  });
  assertGraphqlOk('variant hydrate', hydrate.response);
  cases.push(hydrate);

  const priceListCreate = await captureCase<PriceListCreateData>(
    'priceListCreate disposable currency-change subject',
    priceListCreateDocument,
    {
      input: {
        name: `Currency fixed price clear ${unique}`,
        currency: initialCurrency,
        parent: {
          adjustment: {
            type: 'PERCENTAGE_DECREASE',
            value: 10,
          },
        },
      },
    },
  );
  assertNoUserErrors(priceListCreate.response, 'priceListCreate', 'priceListCreate setup');
  const createdPriceListId = priceListId(priceListCreate.response);
  createdPriceListIds.push(createdPriceListId);
  cases.push(priceListCreate);

  const catalogCreate = await captureCase<CatalogCreateData>(
    'catalogCreate attaches disposable price list',
    catalogCreateDocument,
    {
      input: {
        title: `Currency fixed price catalog ${unique}`,
        status: 'ACTIVE',
        context: {
          marketIds: [marketId],
        },
        priceListId: createdPriceListId,
      },
    },
  );
  assertNoUserErrors(catalogCreate.response, 'catalogCreate', 'catalogCreate setup');
  const createdCatalogId = catalogId(catalogCreate.response);
  createdCatalogIds.push(createdCatalogId);
  cases.push(catalogCreate);

  const fixedPricesAdd = await captureCase<FixedPricesAddData>(
    'priceListFixedPricesAdd seeds multiple fixed prices',
    fixedPricesAddDocument,
    {
      priceListId: createdPriceListId,
      prices: [
        {
          variantId: variantIds[0],
          price: { amount: '12.34', currencyCode: initialCurrency },
        },
        {
          variantId: variantIds[1],
          price: { amount: '23.45', currencyCode: initialCurrency },
        },
      ],
    },
  );
  assertNoUserErrors(fixedPricesAdd.response, 'priceListFixedPricesAdd', 'fixed prices add');
  if ((fixedPricesAdd.response.payload.data?.priceListFixedPricesAdd?.prices ?? []).length !== 2) {
    throw new Error(`fixedPricesAdd did not return two prices: ${JSON.stringify(fixedPricesAdd.response.payload)}`);
  }
  cases.push(fixedPricesAdd);

  const readAfterAdd = await captureCase<PriceListReadData>('read after fixed prices add', readbackDocument, {
    priceListId: createdPriceListId,
    catalogId: createdCatalogId,
  });
  assertGraphqlOk('read after fixed prices add', readAfterAdd.response);
  assertFixedPriceCount('read after add', readAfterAdd.response.payload.data?.priceList, 2);
  assertCatalogRelation('read after add', readAfterAdd.response, createdCatalogId);
  cases.push(readAfterAdd);

  const sameCurrencyUpdate = await captureCase<PriceListUpdateData>(
    'priceListUpdate same currency preserves fixed prices',
    priceListUpdateDocument,
    {
      id: createdPriceListId,
      input: {
        currency: initialCurrency,
      },
    },
  );
  assertNoUserErrors(sameCurrencyUpdate.response, 'priceListUpdate', 'same-currency update');
  assertFixedPriceCount(
    'same-currency update',
    sameCurrencyUpdate.response.payload.data?.priceListUpdate?.priceList,
    2,
  );
  assertCatalogRelation('same-currency update', sameCurrencyUpdate.response, createdCatalogId);
  cases.push(sameCurrencyUpdate);

  const readAfterSameCurrency = await captureCase<PriceListReadData>(
    'read after same-currency update',
    readbackDocument,
    { priceListId: createdPriceListId, catalogId: createdCatalogId },
  );
  assertGraphqlOk('read after same-currency update', readAfterSameCurrency.response);
  assertFixedPriceCount('read after same-currency update', readAfterSameCurrency.response.payload.data?.priceList, 2);
  assertCatalogRelation('read after same-currency update', readAfterSameCurrency.response, createdCatalogId);
  cases.push(readAfterSameCurrency);

  const rejectedUpdate = await captureCase<PriceListUpdateData>(
    'priceListUpdate rejected parent adjustment preserves currency and fixed prices',
    priceListUpdateDocument,
    {
      id: createdPriceListId,
      input: {
        parent: {
          adjustment: {
            type: 'PERCENTAGE_DECREASE',
            value: 250,
          },
        },
      },
    },
  );
  assertGraphqlOk('rejected parent update', rejectedUpdate.response);
  assertRejectedUpdatePreservedPrices(rejectedUpdate.response);
  assertCatalogRelation('rejected parent update', rejectedUpdate.response, createdCatalogId);
  cases.push(rejectedUpdate);

  const readAfterRejected = await captureCase<PriceListReadData>('read after rejected update', readbackDocument, {
    priceListId: createdPriceListId,
    catalogId: createdCatalogId,
  });
  assertGraphqlOk('read after rejected update', readAfterRejected.response);
  assertFixedPriceCount('read after rejected update', readAfterRejected.response.payload.data?.priceList, 2);
  assertCatalogRelation('read after rejected update', readAfterRejected.response, createdCatalogId);
  cases.push(readAfterRejected);

  const changedCurrencyUpdate = await captureCase<PriceListUpdateData>(
    'priceListUpdate changed currency clears fixed prices',
    priceListUpdateDocument,
    {
      id: createdPriceListId,
      input: {
        currency: changedCurrency,
      },
    },
  );
  assertNoUserErrors(changedCurrencyUpdate.response, 'priceListUpdate', 'changed-currency update');
  const changedPriceList = changedCurrencyUpdate.response.payload.data?.priceListUpdate?.priceList;
  if (changedPriceList?.currency !== changedCurrency) {
    throw new Error(
      `changed-currency update did not set ${changedCurrency}: ${JSON.stringify(changedCurrencyUpdate.response.payload)}`,
    );
  }
  assertFixedPriceCount('changed-currency update', changedPriceList, 0);
  assertCatalogRelation('changed-currency update', changedCurrencyUpdate.response, createdCatalogId);
  cases.push(changedCurrencyUpdate);

  const readAfterChangedCurrency = await captureCase<PriceListReadData>(
    'read after changed-currency update',
    readbackDocument,
    { priceListId: createdPriceListId, catalogId: createdCatalogId },
  );
  assertGraphqlOk('read after changed-currency update', readAfterChangedCurrency.response);
  if (readAfterChangedCurrency.response.payload.data?.priceList?.currency !== changedCurrency) {
    throw new Error(
      `read after changed-currency did not retain ${changedCurrency}: ${JSON.stringify(readAfterChangedCurrency.response.payload)}`,
    );
  }
  assertFixedPriceCount(
    'read after changed-currency update',
    readAfterChangedCurrency.response.payload.data?.priceList,
    0,
  );
  assertCatalogRelation('read after changed-currency update', readAfterChangedCurrency.response, createdCatalogId);
  cases.push(readAfterChangedCurrency);
} finally {
  for (const id of createdCatalogIds.slice().reverse()) {
    cleanup.push({
      type: 'catalog',
      id,
      response: await runGraphqlRequest(catalogDeleteDocument, { id }),
    });
  }
  for (const id of createdPriceListIds.slice().reverse()) {
    cleanup.push({
      type: 'priceList',
      id,
      response: await runGraphqlRequest(priceListDeleteDocument, { id }),
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
      scope:
        'priceListUpdate currency changes clear variant fixed prices while same-currency and rejected updates preserve them',
      setup: {
        initialCurrency,
        changedCurrency,
        productDiscovery,
        variantIds,
        cleanup:
          'The capture creates a disposable price list and catalog relation, adds two fixed prices, changes the price-list currency, then deletes the catalog and price list.',
      },
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
      cases: cases.map((entry) => entry.name),
      cleanup: cleanup.map((entry) => ({ type: entry.type, id: entry.id, status: entry.response.status })),
    },
    null,
    2,
  ),
);
