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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);

const marketDetailOutputPath = path.join(outputDir, 'market-detail.json');
const marketsCatalogOutputPath = path.join(outputDir, 'markets-catalog.json');
const marketCatalogsOutputPath = path.join(outputDir, 'market-catalogs.json');
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
    ... on MarketCatalog {
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
      marketWebPresence: schemaInventory.data?.marketWebPresenceType ?? null,
      marketsResolvedValues: schemaInventory.data?.marketsResolvedValuesType ?? null,
      buyerSignalInput: schemaInventory.data?.buyerSignalInput ?? null,
    },
  },
  readOnlyBaselines: {
    markets: marketsCatalog,
    market: marketDetail,
    catalogs: marketCatalogs,
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
        'market-web-presences.json',
        'markets-resolved-values.json',
        'markets-baseline.json',
      ],
      first,
      buyerSignal,
      firstMarketId,
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
