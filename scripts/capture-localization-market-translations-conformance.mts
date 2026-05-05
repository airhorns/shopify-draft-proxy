/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translations-market-scoped';

const productCreateMutation = `#graphql
  mutation LocalizationMarketTranslationProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        handle
        title
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation LocalizationMarketTranslationProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const shopLocalesQuery = `#graphql
  query LocalizationMarketTranslationShopLocales {
    shopLocales {
      locale
      name
      primary
      published
      marketWebPresences {
        id
        subfolderSuffix
      }
    }
  }
`;

const shopLocaleEnableMutation = `#graphql
  mutation LocalizationMarketTranslationShopLocaleEnable($locale: String!) {
    shopLocaleEnable(locale: $locale) {
      shopLocale {
        locale
        name
        primary
        published
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const shopLocaleDisableMutation = `#graphql
  mutation LocalizationMarketTranslationShopLocaleDisable($locale: String!) {
    shopLocaleDisable(locale: $locale) {
      locale
      userErrors {
        field
        message
      }
    }
  }
`;

const marketWithWebPresenceQuery = `#graphql
  query LocalizationMarketTranslationMarketRead($first: Int!) {
    webPresences(first: $first) {
      nodes {
        id
        markets(first: 5) {
          nodes {
            id
            name
            handle
            status
            type
          }
        }
      }
    }
  }
`;

const marketsReadQuery = `#graphql
  query LocalizationMarketTranslationMarketsRead($first: Int!) {
    markets(first: $first) {
      nodes {
        id
        name
        handle
        status
        type
      }
    }
  }
`;

const marketScopedReadQuery = `#graphql
  query LocalizationTranslationsMarketScopedRead($resourceId: ID!, $marketId: ID!) {
    translatableResource(resourceId: $resourceId) {
      resourceId
      translatableContent {
        key
        value
        digest
        locale
        type
      }
      translations(locale: "es", marketId: $marketId) {
        key
        value
        locale
        outdated
        updatedAt
        market {
          id
        }
      }
    }
    allShopLocales: shopLocales {
      locale
      name
      primary
      published
      marketWebPresences {
        id
        subfolderSuffix
      }
    }
  }
`;

const registerMutation = `#graphql
  mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
    translationsRegister(resourceId: $resourceId, translations: $translations) {
      translations {
        key
        value
        locale
        outdated
        market {
          id
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

const removeMutation = `#graphql
  mutation LocalizationTranslationsMarketScopedRemove(
    $resourceId: ID!
    $keys: [String!]!
    $locales: [String!]!
    $marketIds: [ID!]!
  ) {
    translationsRemove(
      resourceId: $resourceId
      translationKeys: $keys
      locales: $locales
      marketIds: $marketIds
    ) {
      translations {
        key
        value
        locale
        outdated
        market {
          id
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function randomSuffix(): string {
  return Math.random().toString(36).slice(2, 10);
}

function dataObject(payload: JsonRecord): JsonRecord {
  const data = payload['data'];
  if (!isRecord(data)) {
    throw new Error(`Expected GraphQL data object: ${JSON.stringify(payload)}`);
  }
  return data;
}

function payloadField(payload: JsonRecord, fieldName: string): JsonRecord {
  const field = dataObject(payload)[fieldName];
  if (!isRecord(field)) {
    throw new Error(`Expected data.${fieldName} object: ${JSON.stringify(payload)}`);
  }
  return field;
}

function userErrors(field: JsonRecord): unknown[] {
  const errors = field['userErrors'];
  return Array.isArray(errors) ? errors : [];
}

function assertNoUserErrors(field: JsonRecord, context: string): void {
  const errors = userErrors(field);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function shopLocaleIsEnabled(payload: JsonRecord, locale: string): boolean {
  const locales = dataObject(payload)['shopLocales'];
  return Array.isArray(locales) && locales.some((entry) => isRecord(entry) && entry['locale'] === locale);
}

function firstMarketWithWebPresence(payload: JsonRecord): JsonRecord | null {
  const webPresences = dataObject(payload)['webPresences'];
  if (!isRecord(webPresences) || !Array.isArray(webPresences['nodes'])) {
    throw new Error(`Expected webPresences nodes in market setup response: ${JSON.stringify(payload)}`);
  }

  for (const webPresence of webPresences['nodes']) {
    if (!isRecord(webPresence)) continue;
    const markets = webPresence['markets'];
    if (!isRecord(markets) || !Array.isArray(markets['nodes'])) continue;
    for (const market of markets['nodes']) {
      if (isRecord(market) && typeof market['id'] === 'string') {
        return market;
      }
    }
  }

  return null;
}

function firstMarket(payload: JsonRecord): JsonRecord {
  const markets = dataObject(payload)['markets'];
  if (!isRecord(markets) || !Array.isArray(markets['nodes'])) {
    throw new Error(`Expected markets nodes in setup response: ${JSON.stringify(payload)}`);
  }

  for (const market of markets['nodes']) {
    if (isRecord(market) && typeof market['id'] === 'string') {
      return market;
    }
  }

  throw new Error('Market setup failed: no market returned from markets(first:).');
}

function productTitleDigest(payload: JsonRecord): string {
  const resource = dataObject(payload)['translatableResource'];
  if (!isRecord(resource) || !Array.isArray(resource['translatableContent'])) {
    throw new Error(`Expected translatableResource content in read response: ${JSON.stringify(payload)}`);
  }

  for (const item of resource['translatableContent']) {
    if (isRecord(item) && item['key'] === 'title' && typeof item['digest'] === 'string') {
      return item['digest'];
    }
  }

  throw new Error('Could not find product title digest in market-scoped localization read.');
}

async function bestEffortCleanup(options: {
  createdProductId: string | null;
  resourceId: string | null;
  marketId: string | null;
  shouldDisableSpanishLocale: boolean;
}): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};

  if (options.resourceId && options.marketId) {
    try {
      cleanup['translationsRemove'] = await runGraphql(removeMutation, {
        resourceId: options.resourceId,
        keys: ['title'],
        locales: ['es'],
        marketIds: [options.marketId],
      });
    } catch (error) {
      cleanup['translationsRemoveError'] = String(error);
    }
  }

  if (options.createdProductId) {
    try {
      cleanup['productDelete'] = await runGraphql(productDeleteMutation, {
        input: { id: options.createdProductId },
      });
    } catch (error) {
      cleanup['productDeleteError'] = String(error);
    }
  }

  if (options.shouldDisableSpanishLocale) {
    try {
      cleanup['shopLocaleDisable'] = await runGraphql(shopLocaleDisableMutation, { locale: 'es' });
    } catch (error) {
      cleanup['shopLocaleDisableError'] = String(error);
    }
  }

  return cleanup;
}

const captureToken = randomSuffix();
const productInput = {
  title: `HAR-709 market translation ${captureToken}`,
  handle: `har-709-market-translation-${captureToken}`,
  status: 'DRAFT',
};

let createdProductId: string | null = null;
let resourceId: string | null = null;
let marketId: string | null = null;
let marketSource = 'webPresence';
let shouldDisableSpanishLocale = false;
let cleanup: JsonRecord = {};

try {
  const initialShopLocales = await runGraphql(shopLocalesQuery);
  shouldDisableSpanishLocale = !shopLocaleIsEnabled(initialShopLocales, 'es');
  const localeSetup = shouldDisableSpanishLocale
    ? await runGraphql(shopLocaleEnableMutation, { locale: 'es' })
    : initialShopLocales;
  if (shouldDisableSpanishLocale) {
    assertNoUserErrors(payloadField(localeSetup, 'shopLocaleEnable'), 'shopLocaleEnable');
  }

  const productCreate = await runGraphql(productCreateMutation, { product: productInput });
  const productCreatePayload = payloadField(productCreate, 'productCreate');
  assertNoUserErrors(productCreatePayload, 'productCreate');
  const product = productCreatePayload['product'];
  if (!isRecord(product) || typeof product['id'] !== 'string') {
    throw new Error(`Product setup did not return a product id: ${JSON.stringify(productCreate)}`);
  }
  createdProductId = product['id'];
  resourceId = createdProductId;

  const marketWebPresenceSetup = await runGraphql(marketWithWebPresenceQuery, { first: 10 });
  const marketFromWebPresence = firstMarketWithWebPresence(marketWebPresenceSetup);
  let market = marketFromWebPresence;
  if (market === null) {
    marketSource = 'markets';
    market = firstMarket(await runGraphql(marketsReadQuery, { first: 10 }));
  }
  marketId = market['id'] as string;

  const readVariables = { resourceId, marketId };
  const readBeforeRegister = await runGraphql(marketScopedReadQuery, readVariables);
  const digest = productTitleDigest(readBeforeRegister);
  const translationValue = `Titulo HAR-709 ${captureToken}`;
  const registerVariables = {
    resourceId,
    translations: [
      {
        locale: 'es',
        key: 'title',
        value: translationValue,
        marketId,
        translatableContentDigest: digest,
      },
    ],
  };
  const removeVariables = {
    resourceId,
    keys: ['title'],
    locales: ['es'],
    marketIds: [marketId],
  };

  const register = await runGraphql(registerMutation, registerVariables);
  assertNoUserErrors(payloadField(register, 'translationsRegister'), 'translationsRegister');
  const readAfterRegister = await runGraphql(marketScopedReadQuery, readVariables);
  const remove = await runGraphql(removeMutation, removeVariables);
  assertNoUserErrors(payloadField(remove, 'translationsRemove'), 'translationsRemove');
  const readAfterRemove = await runGraphql(marketScopedReadQuery, readVariables);

  cleanup = await bestEffortCleanup({
    createdProductId,
    resourceId,
    marketId,
    shouldDisableSpanishLocale,
  });

  const capture = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    setup: {
      disposableProduct: product,
      market,
      marketSource,
      localeWasInitiallyEnabled: !shouldDisableSpanishLocale,
    },
    marketScopedTranslationLifecycle: {
      resourceId,
      locale: 'es',
      marketId,
      titleDigest: digest,
      translationValue,
      readRequest: { variables: readVariables },
      registerRequest: { variables: registerVariables },
      removeRequest: { variables: removeVariables },
      readBeforeRegister,
      register,
      readAfterRegister,
      remove,
      readAfterRemove,
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'LocalizationTranslationsMarketScopedRead',
        variables: readVariables,
        query: 'sha:hand-synthesized-from-readBeforeRegister-response',
        response: {
          status: 200,
          body: readBeforeRegister,
        },
      },
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath, storeDomain, apiVersion, resourceId, marketId }, null, 2));
} catch (error) {
  cleanup = await bestEffortCleanup({
    createdProductId,
    resourceId,
    marketId,
    shouldDisableSpanishLocale,
  });
  console.error(JSON.stringify({ cleanupAfterFailure: cleanup }, null, 2));
  throw error;
}
