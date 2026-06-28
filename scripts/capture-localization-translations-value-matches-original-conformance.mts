/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translations-value-matches-original';
const locale = 'es';

const productCreateMutation = `#graphql
  mutation LocalizationTranslationsValueMatchesProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
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
  mutation LocalizationTranslationsValueMatchesProductDelete($input: ProductDeleteInput!) {
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
  query LocalizationTranslationsValueMatchesShopLocales {
    shopLocales {
      locale
      name
      primary
      published
    }
  }
`;

const shopLocaleEnableMutation = `#graphql
  mutation LocalizationTranslationsValueMatchesShopLocaleEnable($locale: String!) {
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
  mutation LocalizationTranslationsValueMatchesShopLocaleDisable($locale: String!) {
    shopLocaleDisable(locale: $locale) {
      locale
      userErrors {
        field
        message
      }
    }
  }
`;

const marketsReadQuery = `#graphql
  query LocalizationTranslationsValueMatchesMarketsRead($first: Int!) {
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

const readQuery = `#graphql
  query LocalizationTranslationsValueMatchesRead(
    $resourceId: ID!
    $marketId: ID!
    $resourceIds: [ID!]!
    $marketsFirst: Int!
  ) {
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
    byIds: translatableResourcesByIds(first: 10, resourceIds: $resourceIds) {
      nodes {
        resourceId
        translatableContent {
          key
          value
          digest
          locale
          type
        }
      }
    }
    markets(first: $marketsFirst) {
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
  mutation LocalizationTranslationsValueMatchesRemove(
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

function assertValueMatchesOriginal(field: JsonRecord, context: string): void {
  const translations = field['translations'];
  const errors = userErrors(field);
  const expectedError = {
    field: ['translations', '0', 'value'],
    message: 'Value cannot match original content',
    code: 'FAILS_RESOURCE_VALIDATION',
  };
  if (!Array.isArray(translations) || translations.length !== 0) {
    throw new Error(`${context} should not return translations: ${JSON.stringify(field)}`);
  }
  if (JSON.stringify(errors) !== JSON.stringify([expectedError])) {
    throw new Error(`${context} did not return VALUE_MATCHES_ORIGINAL_CONTENT shape: ${JSON.stringify(errors)}`);
  }
}

function shopLocaleIsEnabled(payload: JsonRecord, targetLocale: string): boolean {
  const locales = dataObject(payload)['shopLocales'];
  return Array.isArray(locales) && locales.some((entry) => isRecord(entry) && entry['locale'] === targetLocale);
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

function productContent(payload: JsonRecord, key: string): { value: string; digest: string } {
  const resource = dataObject(payload)['translatableResource'];
  if (!isRecord(resource) || !Array.isArray(resource['translatableContent'])) {
    throw new Error(`Expected translatableResource content in read response: ${JSON.stringify(payload)}`);
  }
  for (const item of resource['translatableContent']) {
    if (
      isRecord(item) &&
      item['key'] === key &&
      typeof item['value'] === 'string' &&
      typeof item['digest'] === 'string'
    ) {
      return { value: item['value'], digest: item['digest'] };
    }
  }
  throw new Error(`Could not find product ${key} content in read response.`);
}

function assertTranslationValue(field: JsonRecord, expectedValue: string, context: string): void {
  assertNoUserErrors(field, context);
  const translations = field['translations'];
  if (!Array.isArray(translations) || translations.length !== 1) {
    throw new Error(`${context} should return one translation: ${JSON.stringify(field)}`);
  }
  const [translation] = translations;
  if (!isRecord(translation) || translation['key'] !== 'title' || translation['value'] !== expectedValue) {
    throw new Error(`${context} returned unexpected translation: ${JSON.stringify(field)}`);
  }
}

async function bestEffortCleanup(options: {
  runGraphql: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlPayload<unknown>>;
  createdProductId: string | null;
  marketId: string | null;
  shouldDisableLocale: boolean;
}): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};
  if (options.createdProductId !== null && options.marketId !== null) {
    try {
      cleanup['translationsRemove'] = await options.runGraphql(removeMutation, {
        resourceId: options.createdProductId,
        keys: ['title'],
        locales: [locale],
        marketIds: [options.marketId],
      });
    } catch (error: unknown) {
      cleanup['translationsRemoveError'] = String(error);
    }
  }
  if (options.createdProductId !== null) {
    try {
      cleanup['productDelete'] = await options.runGraphql(productDeleteMutation, {
        input: { id: options.createdProductId },
      });
    } catch (error: unknown) {
      cleanup['productDeleteError'] = String(error);
    }
  }
  if (options.shouldDisableLocale) {
    try {
      cleanup['shopLocaleDisable'] = await options.runGraphql(shopLocaleDisableMutation, { locale });
    } catch (error: unknown) {
      cleanup['shopLocaleDisableError'] = String(error);
    }
  }
  return cleanup;
}

const captureToken = randomSuffix();
const productInput = {
  title: `Value matches original fixture ${captureToken}`,
  handle: `value-matches-original-${captureToken}`,
  status: 'DRAFT',
};

let createdProductId: string | null = null;
let marketId: string | null = null;
let shouldDisableLocale = false;
let cleanup: JsonRecord = {};

try {
  const initialShopLocales = await runGraphql(shopLocalesQuery);
  shouldDisableLocale = !shopLocaleIsEnabled(initialShopLocales, locale);
  const localeSetup = await runGraphql(shopLocaleEnableMutation, { locale });
  if (shouldDisableLocale) {
    assertNoUserErrors(payloadField(localeSetup, 'shopLocaleEnable'), 'shopLocaleEnable');
  }

  const marketsRead = await runGraphql(marketsReadQuery, { first: 10 });
  const market = firstMarket(marketsRead);
  marketId = market['id'] as string;

  const productCreateVariables = { product: productInput };
  const productCreate = await runGraphql(productCreateMutation, productCreateVariables);
  const productCreatePayload = payloadField(productCreate, 'productCreate');
  assertNoUserErrors(productCreatePayload, 'productCreate');
  const product = productCreatePayload['product'];
  if (!isRecord(product) || typeof product['id'] !== 'string') {
    throw new Error(`Product setup did not return a product id: ${JSON.stringify(productCreate)}`);
  }
  createdProductId = product['id'];

  const readVariables = {
    resourceId: createdProductId,
    marketId,
    resourceIds: [createdProductId],
    marketsFirst: 10,
  };
  const readBeforeRegister = await runGraphql(readQuery, readVariables);
  const titleContent = productContent(readBeforeRegister, 'title');
  const differingValue = `Valor localizado ${captureToken}`;

  const marketOriginalWithoutBaseVariables = {
    resourceId: createdProductId,
    translations: [
      {
        locale,
        key: 'title',
        value: titleContent.value,
        marketId,
        translatableContentDigest: titleContent.digest,
      },
    ],
  };
  const removeMarketOriginalVariables = {
    resourceId: createdProductId,
    keys: ['title'],
    locales: [locale],
    marketIds: [marketId],
  };
  const shopLevelOriginalVariables = {
    resourceId: createdProductId,
    translations: [
      {
        locale,
        key: 'title',
        value: titleContent.value,
        translatableContentDigest: titleContent.digest,
      },
    ],
  };
  const valueMatchesBaseTranslationVariables = {
    resourceId: createdProductId,
    translations: [
      {
        locale,
        key: 'title',
        value: titleContent.value,
        marketId,
        translatableContentDigest: titleContent.digest,
      },
    ],
  };
  const valueMatchesBaseTranslationInvalidDigestVariables = {
    resourceId: createdProductId,
    translations: [
      {
        locale,
        key: 'title',
        value: titleContent.value,
        marketId,
        translatableContentDigest: `invalid-${titleContent.digest}`,
      },
    ],
  };
  const differingValueVariables = {
    resourceId: createdProductId,
    translations: [
      {
        locale,
        key: 'title',
        value: differingValue,
        marketId,
        translatableContentDigest: titleContent.digest,
      },
    ],
  };

  const marketOriginalWithoutBaseRegister = await runGraphql(registerMutation, marketOriginalWithoutBaseVariables);
  assertTranslationValue(
    payloadField(marketOriginalWithoutBaseRegister, 'translationsRegister'),
    titleContent.value,
    'market original without base translationsRegister',
  );
  const removeMarketOriginal = await runGraphql(removeMutation, removeMarketOriginalVariables);
  assertNoUserErrors(payloadField(removeMarketOriginal, 'translationsRemove'), 'remove market original');
  const shopLevelOriginalRegister = await runGraphql(registerMutation, shopLevelOriginalVariables);
  assertTranslationValue(
    payloadField(shopLevelOriginalRegister, 'translationsRegister'),
    titleContent.value,
    'shop-level original translationsRegister',
  );
  const valueMatchesBaseTranslationRegister = await runGraphql(registerMutation, valueMatchesBaseTranslationVariables);
  assertValueMatchesOriginal(
    payloadField(valueMatchesBaseTranslationRegister, 'translationsRegister'),
    'value-matches-base-translation translationsRegister',
  );
  const valueMatchesBaseTranslationInvalidDigestRegister = await runGraphql(
    registerMutation,
    valueMatchesBaseTranslationInvalidDigestVariables,
  );
  assertValueMatchesOriginal(
    payloadField(valueMatchesBaseTranslationInvalidDigestRegister, 'translationsRegister'),
    'value-matches-base-translation invalid-digest translationsRegister',
  );
  const readAfterReject = await runGraphql(readQuery, readVariables);
  const differingValueRegister = await runGraphql(registerMutation, differingValueVariables);
  assertTranslationValue(
    payloadField(differingValueRegister, 'translationsRegister'),
    differingValue,
    'differing value translationsRegister',
  );
  const readAfterDifferingValue = await runGraphql(readQuery, readVariables);

  const productIdForLog = createdProductId;
  cleanup = await bestEffortCleanup({
    runGraphql,
    createdProductId,
    marketId,
    shouldDisableLocale,
  });
  createdProductId = null;
  marketId = null;
  shouldDisableLocale = false;

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId,
        storeDomain,
        apiVersion,
        capturedAt: new Date().toISOString(),
        setup: {
          locale,
          localeSetup: {
            variables: { locale },
            response: localeSetup,
          },
          marketsRead: {
            variables: { first: 10 },
            response: marketsRead,
          },
          market,
          productCreate: {
            variables: productCreateVariables,
            response: productCreate,
          },
        },
        readBeforeRegister: {
          variables: readVariables,
          response: readBeforeRegister,
        },
        sourceTitle: titleContent.value,
        titleDigest: titleContent.digest,
        marketOriginalWithoutBaseRegister: {
          variables: marketOriginalWithoutBaseVariables,
          response: marketOriginalWithoutBaseRegister,
        },
        removeMarketOriginal: {
          variables: removeMarketOriginalVariables,
          response: removeMarketOriginal,
        },
        shopLevelOriginalRegister: {
          variables: shopLevelOriginalVariables,
          response: shopLevelOriginalRegister,
        },
        valueMatchesBaseTranslationRegister: {
          variables: valueMatchesBaseTranslationVariables,
          response: valueMatchesBaseTranslationRegister,
        },
        valueMatchesBaseTranslationInvalidDigestRegister: {
          variables: valueMatchesBaseTranslationInvalidDigestVariables,
          response: valueMatchesBaseTranslationInvalidDigestRegister,
        },
        readAfterReject: {
          variables: readVariables,
          response: readAfterReject,
        },
        differingValueRegister: {
          variables: differingValueVariables,
          response: differingValueRegister,
        },
        readAfterDifferingValue: {
          variables: readVariables,
          response: readAfterDifferingValue,
        },
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  console.log(JSON.stringify({ ok: true, outputPath, storeDomain, apiVersion, productId: productIdForLog }, null, 2));
} finally {
  if (createdProductId !== null || shouldDisableLocale) {
    cleanup = await bestEffortCleanup({
      runGraphql,
      createdProductId,
      marketId,
      shouldDisableLocale,
    });
    console.log(`Cleanup after failure: ${JSON.stringify(cleanup)}`);
  }
}
