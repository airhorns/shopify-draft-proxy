// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');

const marketDetailOutputPath = path.join(outputDir, 'market-detail.json');
const marketsCatalogOutputPath = path.join(outputDir, 'markets-catalog.json');
const marketCatalogsOutputPath = path.join(outputDir, 'market-catalogs.json');
const marketCatalogDetailOutputPath = path.join(outputDir, 'market-catalog-detail.json');
const priceListDetailOutputPath = path.join(outputDir, 'price-list-detail.json');
const priceListsOutputPath = path.join(outputDir, 'price-lists.json');
const priceListPricesFilteredOutputPath = path.join(outputDir, 'price-list-prices-filtered.json');
const marketWebPresencesOutputPath = path.join(outputDir, 'market-web-presences.json');
const marketsResolvedValuesOutputPath = path.join(outputDir, 'markets-resolved-values.json');
const marketsBaselineOutputPath = path.join(outputDir, 'markets-baseline.json');

const { runGraphql, runGraphqlRequest: runGraphqlProbe } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketFields = `#graphql
  fragment MarketReadFields on Market {
    id
    name
    handle
    status
    type
    conditions {
      conditionTypes
      regionsCondition {
        applicationLevel
        regions(first: 10) {
          edges {
            cursor
            node {
              __typename
              id
              name
              ... on MarketRegionCountry {
                code
                currency {
                  currencyCode
                  currencyName
                  enabled
                }
              }
            }
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
      }
    }
    currencySettings {
      baseCurrency {
        currencyCode
        currencyName
        enabled
      }
      localCurrencies
      roundingEnabled
    }
    priceInclusions {
      inclusiveDutiesPricingStrategy
      inclusiveTaxPricingStrategy
    }
    catalogs(first: 5) {
      edges {
        cursor
        node {
          id
          title
          status
          publication {
            id
            autoPublish
          }
          priceList {
            id
            name
            currency
          }
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    webPresences(first: 5) {
      edges {
        cursor
        node {
          id
          subfolderSuffix
          domain {
            id
            host
            url
            sslEnabled
          }
          rootUrls {
            locale
            url
          }
          defaultLocale {
            locale
            name
            primary
            published
          }
          alternateLocales {
            locale
            name
            primary
            published
          }
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const marketCatalogFields = `#graphql
  fragment MarketCatalogReadFields on Catalog {
    __typename
    id
    title
    status
    priceList {
      id
      name
      currency
    }
    publication {
      id
      autoPublish
    }
    operations {
      __typename
    }
    ... on MarketCatalog {
      marketsCount {
        count
        precision
      }
      markets(first: 5) {
        edges {
          cursor
          node {
            id
            name
            handle
            status
            type
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
    }
  }
`;

const priceListFields = `#graphql
  fragment PriceListReadFields on PriceList {
    __typename
    id
    name
    currency
    fixedPricesCount
    parent {
      adjustment {
        type
        value
      }
    }
    catalog {
      id
      title
      status
    }
    prices(first: 5, query: $priceQuery, originType: $originType) {
      edges {
        cursor
        node {
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
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const marketWebPresenceFields = `#graphql
  fragment MarketWebPresenceReadFields on MarketWebPresence {
    id
    subfolderSuffix
    domain {
      id
      host
      url
      sslEnabled
    }
    rootUrls {
      locale
      url
    }
    defaultLocale {
      locale
      name
      primary
      published
    }
    alternateLocales {
      locale
      name
      primary
      published
    }
    markets(first: 5) {
      edges {
        cursor
        node {
          id
          name
          handle
          status
          type
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const marketsCatalogQuery = `#graphql
  ${marketFields}
  query MarketsCatalogRead($first: Int!) {
    markets(first: $first) {
      edges {
        cursor
        node {
          ...MarketReadFields
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const marketDetailQuery = `#graphql
  ${marketFields}
  query MarketDetailRead($id: ID!) {
    market(id: $id) {
      ...MarketReadFields
    }
  }
`;

const marketCatalogsQuery = `#graphql
  ${marketCatalogFields}
  query MarketCatalogsRead($first: Int!) {
    catalogs(first: $first, type: MARKET) {
      edges {
        cursor
        node {
          ...MarketCatalogReadFields
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const marketCatalogDetailQuery = `#graphql
  ${marketCatalogFields}
  query MarketCatalogDetailRead($catalogId: ID!, $catalogCountLimit: Int) {
    catalog(id: $catalogId) {
      ...MarketCatalogReadFields
    }
    catalogsCount(type: MARKET, limit: $catalogCountLimit) {
      count
      precision
    }
  }
`;

const priceListDetailQuery = `#graphql
  ${priceListFields}
  query PriceListDetailRead($priceListId: ID!, $priceQuery: String, $originType: PriceListPriceOriginType) {
    priceList(id: $priceListId) {
      ...PriceListReadFields
    }
  }
`;

const priceListsQuery = `#graphql
  query PriceListsRead($first: Int!) {
    priceLists(first: $first) {
      edges {
        cursor
        node {
          __typename
          id
          name
          currency
          fixedPricesCount
          parent {
            adjustment {
              type
              value
            }
          }
          catalog {
            id
            title
            status
          }
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const priceListPricesFilteredQuery = `#graphql
  ${priceListFields}
  query PriceListPricesFilteredRead($priceListId: ID!, $priceQuery: String, $originType: PriceListPriceOriginType) {
    priceList(id: $priceListId) {
      ...PriceListReadFields
    }
  }
`;

const marketWebPresencesQuery = `#graphql
  ${marketWebPresenceFields}
  query MarketWebPresencesRead($first: Int!) {
    webPresences(first: $first) {
      edges {
        cursor
        node {
          ...MarketWebPresenceReadFields
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const marketsResolvedValuesQuery = `#graphql
  ${marketCatalogFields}
  ${marketWebPresenceFields}
  query MarketsResolvedValuesRead($first: Int!, $buyerSignal: BuyerSignalInput!) {
    marketsResolvedValues(buyerSignal: $buyerSignal) {
      currencyCode
      priceInclusivity {
        dutiesIncluded
        taxesIncluded
      }
      catalogs(first: $first) {
        edges {
          cursor
          node {
            ...MarketCatalogReadFields
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      webPresences(first: $first) {
        edges {
          cursor
          node {
            ...MarketWebPresenceReadFields
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
    }
  }
`;

const localeScopeProbeQuery = `#graphql
  query MarketWebPresenceLocaleScopeProbe($first: Int!) {
    webPresences(first: $first) {
      edges {
        node {
          id
          defaultLocale {
            locale
            name
            primary
            published
          }
          alternateLocales {
            locale
            name
            primary
            published
          }
        }
      }
    }
  }
`;

const schemaInventoryQuery = `#graphql
  query MarketsSchemaInventory {
    queryRoot: __type(name: "QueryRoot") {
      fields {
        name
        args {
          name
          type {
            ...TypeRef
          }
        }
        type {
          ...TypeRef
        }
      }
    }
    mutationRoot: __type(name: "Mutation") {
      fields {
        name
        args {
          name
          type {
            ...TypeRef
          }
        }
        type {
          ...TypeRef
        }
      }
    }
    marketType: __type(name: "Market") {
      fields {
        name
        isDeprecated
        deprecationReason
        type {
          ...TypeRef
        }
      }
    }
    marketConditionsType: __type(name: "MarketConditions") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    marketCatalogType: __type(name: "MarketCatalog") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    priceListType: __type(name: "PriceList") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    marketWebPresenceType: __type(name: "MarketWebPresence") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    marketsResolvedValuesType: __type(name: "MarketsResolvedValues") {
      fields {
        name
        type {
          ...TypeRef
        }
      }
    }
    buyerSignalInput: __type(name: "BuyerSignalInput") {
      inputFields {
        name
        type {
          ...TypeRef
        }
      }
    }
  }

  fragment TypeRef on __Type {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
  }
`;

const marketsQueryRoots = [
  'market',
  'markets',
  'catalog',
  'catalogs',
  'catalogsCount',
  'priceList',
  'priceLists',
  'webPresences',
  'marketsResolvedValues',
  'availableBackupRegions',
  'backupRegion',
  'marketLocalizableResource',
  'marketLocalizableResources',
  'marketLocalizableResourcesByIds',
];

const marketsMutationRoots = [
  'marketCreate',
  'marketUpdate',
  'marketDelete',
  'webPresenceCreate',
  'webPresenceUpdate',
  'webPresenceDelete',
  'marketLocalizationsRegister',
  'marketLocalizationsRemove',
  'backupRegionUpdate',
  'marketCurrencySettingsUpdate',
];

function filterRootFields(schemaInventory, rootKey, names) {
  const fields = schemaInventory.data?.[rootKey]?.fields ?? [];
  return fields.filter((field) => names.includes(field.name));
}

function mutationValidationProbePlan() {
  return [
    {
      root: 'market lifecycle',
      operations: ['marketCreate', 'marketUpdate', 'marketDelete'],
      requiredScopes: ['read_markets', 'write_markets'],
      safety: 'side-effect-heavy; can create, mutate, activate, or delete buyer-facing market configuration',
      recommendedProbe: 'schema and invalid-ID/input validation only until a disposable market fixture is available',
    },
    {
      root: 'market web presences',
      operations: ['webPresenceCreate', 'webPresenceUpdate', 'webPresenceDelete'],
      requiredScopes: ['read_markets', 'write_markets'],
      safety: 'side-effect-heavy; can change buyer-facing market domains, subfolders, and SEO routing',
      recommendedProbe: 'schema and invalid-ID/input validation only; avoid success-path writes on shared stores',
    },
    {
      root: 'market localizations',
      operations: ['marketLocalizationsRegister', 'marketLocalizationsRemove'],
      requiredScopes: ['read_markets', 'write_markets'],
      safety: 'side-effect-heavy; changes market-specific localized resource values',
      recommendedProbe: 'schema and invalid resource/localization payload validation before any live write capture',
    },
    {
      root: 'backup regions and currency settings',
      operations: ['backupRegionUpdate', 'marketCurrencySettingsUpdate'],
      requiredScopes: ['read_markets', 'write_markets'],
      safety: 'side-effect-heavy; changes fallback regions or pricing/currency behavior for markets',
      recommendedProbe: 'schema and invalid-ID/input validation only unless a disposable shop capability is prepared',
    },
  ];
}

await mkdir(outputDir, { recursive: true });

const first = 3;
const buyerSignal = { countryCode: 'US' };
const schemaInventory = await runGraphql(schemaInventoryQuery);
const marketsCatalog = await runGraphql(marketsCatalogQuery, { first });
const firstMarketId = marketsCatalog.data?.markets?.edges?.[0]?.node?.id ?? null;
const marketDetail =
  typeof firstMarketId === 'string' && firstMarketId.length > 0
    ? await runGraphql(marketDetailQuery, { id: firstMarketId })
    : { data: { market: null } };
const marketCatalogs = await runGraphql(marketCatalogsQuery, { first });
const firstCatalog = marketCatalogs.data?.catalogs?.edges?.[0]?.node ?? null;
const firstCatalogId = typeof firstCatalog?.id === 'string' && firstCatalog.id.length > 0 ? firstCatalog.id : null;
const firstPriceListId =
  typeof firstCatalog?.priceList?.id === 'string' && firstCatalog.priceList.id.length > 0
    ? firstCatalog.priceList.id
    : null;
const marketCatalogDetail =
  typeof firstCatalogId === 'string'
    ? await runGraphql(marketCatalogDetailQuery, { catalogId: firstCatalogId, catalogCountLimit: null })
    : { data: { catalog: null, catalogsCount: { count: 0, precision: 'EXACT' } } };
const priceListDetail =
  typeof firstPriceListId === 'string'
    ? await runGraphql(priceListDetailQuery, { priceListId: firstPriceListId, priceQuery: null, originType: null })
    : { data: { priceList: null } };
const priceLists = await runGraphql(priceListsQuery, { first });
const firstPriceNode = priceListDetail.data?.priceList?.prices?.edges?.[0]?.node ?? null;
const firstVariantId = typeof firstPriceNode?.variant?.id === 'string' ? firstPriceNode.variant.id : null;
const firstVariantLegacyId = firstVariantId?.split('/').at(-1) ?? null;
const priceListPricesFiltered =
  typeof firstPriceListId === 'string'
    ? await runGraphql(priceListPricesFilteredQuery, {
        priceListId: firstPriceListId,
        priceQuery: firstVariantLegacyId ? `variant_id:${firstVariantLegacyId}` : 'variant_id:0',
        originType: null,
      })
    : { data: { priceList: null } };
const marketWebPresences = await runGraphql(marketWebPresencesQuery, { first });
const marketsResolvedValues = await runGraphql(marketsResolvedValuesQuery, { first, buyerSignal });
const localeScopeProbe = await runGraphqlProbe(localeScopeProbeQuery, { first });

const marketsBaseline = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  schemaInventory: {
    queryRoots: filterRootFields(schemaInventory, 'queryRoot', marketsQueryRoots),
    mutationRoots: filterRootFields(schemaInventory, 'mutationRoot', marketsMutationRoots),
    objectTypes: {
      market: schemaInventory.data?.marketType ?? null,
      marketConditions: schemaInventory.data?.marketConditionsType ?? null,
      marketCatalog: schemaInventory.data?.marketCatalogType ?? null,
      priceList: schemaInventory.data?.priceListType ?? null,
      marketWebPresence: schemaInventory.data?.marketWebPresenceType ?? null,
      marketsResolvedValues: schemaInventory.data?.marketsResolvedValuesType ?? null,
      buyerSignalInput: schemaInventory.data?.buyerSignalInput ?? null,
    },
  },
  readOnlyBaselines: {
    markets: marketsCatalog,
    market: marketDetail,
    catalogs: marketCatalogs,
    catalog: marketCatalogDetail,
    priceList: priceListDetail,
    priceLists,
    priceListPricesFiltered,
    webPresences: marketWebPresences,
    marketsResolvedValues,
  },
  accessScopeProbes: {
    marketWebPresenceLocaleFields: {
      query: 'MarketWebPresence.defaultLocale and MarketWebPresence.alternateLocales',
      expectedAccess: '`read_locales` access scope or `read_markets_home` access scope',
      result: localeScopeProbe,
    },
  },
  mutationValidationProbePlan: mutationValidationProbePlan(),
};

await writeFile(marketsCatalogOutputPath, `${JSON.stringify(marketsCatalog, null, 2)}\n`, 'utf8');
await writeFile(marketDetailOutputPath, `${JSON.stringify(marketDetail, null, 2)}\n`, 'utf8');
await writeFile(marketCatalogsOutputPath, `${JSON.stringify(marketCatalogs, null, 2)}\n`, 'utf8');
await writeFile(marketCatalogDetailOutputPath, `${JSON.stringify(marketCatalogDetail, null, 2)}\n`, 'utf8');
await writeFile(priceListDetailOutputPath, `${JSON.stringify(priceListDetail, null, 2)}\n`, 'utf8');
await writeFile(priceListsOutputPath, `${JSON.stringify(priceLists, null, 2)}\n`, 'utf8');
await writeFile(priceListPricesFilteredOutputPath, `${JSON.stringify(priceListPricesFiltered, null, 2)}\n`, 'utf8');
await writeFile(marketWebPresencesOutputPath, `${JSON.stringify(marketWebPresences, null, 2)}\n`, 'utf8');
await writeFile(marketsResolvedValuesOutputPath, `${JSON.stringify(marketsResolvedValues, null, 2)}\n`, 'utf8');
await writeFile(marketsBaselineOutputPath, `${JSON.stringify(marketsBaseline, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      apiVersion,
      files: [
        'market-detail.json',
        'markets-catalog.json',
        'market-catalogs.json',
        'market-catalog-detail.json',
        'price-list-detail.json',
        'price-lists.json',
        'price-list-prices-filtered.json',
        'market-web-presences.json',
        'markets-resolved-values.json',
        'markets-baseline.json',
      ],
      first,
      buyerSignal,
      firstMarketId,
      firstCatalogId,
      firstPriceListId,
      firstVariantId,
      firstVariantLegacyId,
      localeScopeProbeStatus: localeScopeProbe.status,
      localeScopeProbeError:
        Array.isArray(localeScopeProbe.payload?.errors) && localeScopeProbe.payload.errors.length > 0
          ? localeScopeProbe.payload.errors[0]?.message
          : null,
    },
    null,
    2,
  ),
);
