/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translations-invalid-key';

const productCreateMutation = `#graphql
  mutation LocalizationTranslationsInvalidKeyProductCreate($product: ProductCreateInput!) {
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
  mutation LocalizationTranslationsInvalidKeyProductDelete($input: ProductDeleteInput!) {
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
  query LocalizationTranslationsInvalidKeyShopLocales {
    shopLocales {
      locale
      name
      primary
      published
    }
  }
`;

const shopLocaleEnableMutation = `#graphql
  mutation LocalizationShopLocaleEnable($locale: String!) {
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
  mutation LocalizationTranslationsInvalidKeyShopLocaleDisable($locale: String!) {
    shopLocaleDisable(locale: $locale) {
      locale
      userErrors {
        field
        message
      }
    }
  }
`;

const setupReadQuery = `#graphql
  query LocalizationTranslationsInvalidKeySetupRead($resourceId: ID!) {
    shopLocales {
      locale
      name
      primary
      published
    }
    translatableResource(resourceId: $resourceId) {
      resourceId
      translatableContent {
        key
        value
        digest
        locale
        type
      }
      translations(locale: "fr") {
        key
        value
        locale
        outdated
        market {
          id
        }
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

const downstreamReadQuery = `#graphql
  query LocalizationTranslationsRead($resourceId: ID!) {
    translatableResource(resourceId: $resourceId) {
      resourceId
      translations(locale: "fr") {
        key
        value
        locale
        outdated
        market {
          id
        }
      }
    }
  }
`;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function dataObject(payload: ConformanceGraphqlPayload<unknown>): JsonRecord {
  if (!isRecord(payload.data)) {
    throw new Error(`Expected GraphQL data object, got ${JSON.stringify(payload)}`);
  }
  return payload.data;
}

function payloadField(payload: ConformanceGraphqlPayload<unknown>, field: string): JsonRecord {
  const value = dataObject(payload)[field];
  if (!isRecord(value)) {
    throw new Error(`Expected ${field} payload object, got ${JSON.stringify(payload)}`);
  }
  return value;
}

function userErrors(payload: JsonRecord): JsonRecord[] {
  const value = payload['userErrors'];
  if (!Array.isArray(value)) {
    throw new Error(`Expected userErrors array, got ${JSON.stringify(payload)}`);
  }
  return value.filter(isRecord);
}

function assertNoUserErrors(payload: JsonRecord, label: string): void {
  const errors = userErrors(payload);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertInvalidKeyPartialSuccess(payload: JsonRecord, expectedTitleValue: string): void {
  const errors = userErrors(payload);
  if (
    errors.length !== 1 ||
    JSON.stringify(errors[0]?.['field']) !== JSON.stringify(['translations', '1', 'key']) ||
    errors[0]?.['message'] !== 'Key incorrect_key is not a valid translatable field' ||
    errors[0]?.['code'] !== 'INVALID_KEY_FOR_MODEL'
  ) {
    throw new Error(`translationsRegister did not return indexed INVALID_KEY_FOR_MODEL: ${JSON.stringify(payload)}`);
  }

  const translations = payload['translations'];
  if (!Array.isArray(translations) || translations.length !== 1 || !isRecord(translations[0])) {
    throw new Error(`translationsRegister did not return only the valid row: ${JSON.stringify(payload)}`);
  }
  const validTranslation = translations[0];
  if (
    validTranslation['key'] !== 'title' ||
    validTranslation['value'] !== expectedTitleValue ||
    validTranslation['locale'] !== 'fr' ||
    validTranslation['outdated'] !== false ||
    validTranslation['market'] !== null
  ) {
    throw new Error(`translationsRegister valid-row echo differed from expected shape: ${JSON.stringify(payload)}`);
  }
}

function assertDownstreamOmittedInvalidKey(
  payload: ConformanceGraphqlPayload<unknown>,
  expectedTitleValue: string,
): void {
  const resource = dataObject(payload)['translatableResource'];
  if (!isRecord(resource) || !Array.isArray(resource['translations'])) {
    throw new Error(`Downstream read did not return translations: ${JSON.stringify(payload)}`);
  }

  const translations = resource['translations'].filter(isRecord);
  if (translations.some((translation) => translation['key'] === 'incorrect_key')) {
    throw new Error(`Downstream read returned the invalid-key translation: ${JSON.stringify(payload)}`);
  }
  if (
    translations.length !== 1 ||
    translations[0]?.['key'] !== 'title' ||
    translations[0]?.['value'] !== expectedTitleValue ||
    translations[0]?.['locale'] !== 'fr'
  ) {
    throw new Error(`Downstream read did not return exactly the valid title translation: ${JSON.stringify(payload)}`);
  }
}

function shopLocaleIsEnabled(payload: ConformanceGraphqlPayload<unknown>, locale: string): boolean {
  const locales = dataObject(payload)['shopLocales'];
  return Array.isArray(locales) && locales.some((entry) => isRecord(entry) && entry['locale'] === locale);
}

function titleDigest(payload: ConformanceGraphqlPayload<unknown>): string {
  const resource = dataObject(payload)['translatableResource'];
  if (!isRecord(resource) || !Array.isArray(resource['translatableContent'])) {
    throw new Error(`Setup read did not return translatable content: ${JSON.stringify(payload)}`);
  }
  const title = resource['translatableContent'].find(
    (entry) => isRecord(entry) && entry['key'] === 'title' && typeof entry['digest'] === 'string',
  );
  if (!isRecord(title) || typeof title['digest'] !== 'string') {
    throw new Error(`Setup read did not return a title digest: ${JSON.stringify(payload)}`);
  }
  return title['digest'];
}

async function main(): Promise<void> {
  const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
  const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  const { runGraphqlRequest } = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });

  async function runGraphql(query: string, variables: JsonRecord = {}): Promise<ConformanceGraphqlPayload<unknown>> {
    const { status, payload } = await runGraphqlRequest(query, variables);
    if (status < 200 || status >= 300 || payload.errors) {
      throw new Error(`GraphQL request failed (${status}): ${JSON.stringify(payload)}`);
    }
    return payload;
  }

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
  const outputPath = path.join(outputDir, `${scenarioId}.json`);
  const captureToken = Date.now().toString();
  const productInput = {
    title: `Invalid key validation ${captureToken}`,
    handle: `invalid-key-validation-${captureToken}`,
    status: 'DRAFT',
  };

  let createdProductId: string | null = null;
  let shouldDisableFrenchLocale = false;
  let cleanup: JsonRecord = {};

  try {
    const initialShopLocales = await runGraphql(shopLocalesQuery);
    shouldDisableFrenchLocale = !shopLocaleIsEnabled(initialShopLocales, 'fr');
    const localeSetup = await runGraphql(shopLocaleEnableMutation, { locale: 'fr' });
    if (shouldDisableFrenchLocale) {
      assertNoUserErrors(payloadField(localeSetup, 'shopLocaleEnable'), 'shopLocaleEnable');
    }

    const productCreateVariables = { product: productInput };
    const productCreate = await runGraphql(productCreateMutation, productCreateVariables);
    const productCreatePayload = payloadField(productCreate, 'productCreate');
    assertNoUserErrors(productCreatePayload, 'productCreate');
    const product = productCreatePayload['product'];
    if (!isRecord(product) || typeof product['id'] !== 'string') {
      throw new Error(`Product setup did not return a product id: ${JSON.stringify(productCreate)}`);
    }
    createdProductId = product['id'];

    const setupReadVariables = { resourceId: createdProductId };
    const setupRead = await runGraphql(setupReadQuery, setupReadVariables);
    const currentDigest = titleDigest(setupRead);
    const validTitleValue = `Titre valid ${captureToken}`;
    const invalidKeyRegisterVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: 'fr',
          key: 'title',
          value: validTitleValue,
          translatableContentDigest: currentDigest,
        },
        {
          locale: 'fr',
          key: 'incorrect_key',
          value: `Invalid key value ${captureToken}`,
          translatableContentDigest: currentDigest,
        },
      ],
    };

    const invalidKeyRegister = await runGraphql(registerMutation, invalidKeyRegisterVariables);
    assertInvalidKeyPartialSuccess(payloadField(invalidKeyRegister, 'translationsRegister'), validTitleValue);

    const downstreamReadVariables = { resourceId: createdProductId };
    const downstreamRead = await runGraphql(downstreamReadQuery, downstreamReadVariables);
    assertDownstreamOmittedInvalidKey(downstreamRead, validTitleValue);

    cleanup = await bestEffortCleanup({
      runGraphql,
      createdProductId,
      shouldDisableFrenchLocale,
    });
    createdProductId = null;
    shouldDisableFrenchLocale = false;

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
            localeSetup: {
              variables: { locale: 'fr' },
              response: localeSetup,
            },
            productCreate: {
              variables: productCreateVariables,
              response: productCreate,
            },
          },
          setupRead: {
            variables: setupReadVariables,
            response: setupRead,
          },
          invalidKeyRegister: {
            variables: invalidKeyRegisterVariables,
            response: invalidKeyRegister,
          },
          downstreamRead: {
            variables: downstreamReadVariables,
            response: downstreamRead,
          },
          cleanup,
          upstreamCalls: [],
        },
        null,
        2,
      )}\n`,
      'utf8',
    );
    console.log(`Wrote ${outputPath}`);
  } finally {
    if (createdProductId !== null || shouldDisableFrenchLocale) {
      cleanup = await bestEffortCleanup({
        runGraphql,
        createdProductId,
        shouldDisableFrenchLocale,
      });
      console.log(`Cleanup after failure: ${JSON.stringify(cleanup)}`);
    }
  }
}

async function bestEffortCleanup(options: {
  runGraphql: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlPayload<unknown>>;
  createdProductId: string | null;
  shouldDisableFrenchLocale: boolean;
}): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};
  if (options.createdProductId !== null) {
    try {
      cleanup['productDelete'] = await options.runGraphql(productDeleteMutation, {
        input: { id: options.createdProductId },
      });
    } catch (error: unknown) {
      cleanup['productDeleteError'] = String(error);
    }
  }
  if (options.shouldDisableFrenchLocale) {
    try {
      cleanup['shopLocaleDisable'] = await options.runGraphql(shopLocaleDisableMutation, {
        locale: 'fr',
      });
    } catch (error: unknown) {
      cleanup['shopLocaleDisableError'] = String(error);
    }
  }
  return cleanup;
}

await main();
