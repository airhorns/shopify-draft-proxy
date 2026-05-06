/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translations-mutation-noop-validation';
const disabledLocale = 'it';
const unknownKeyLocale = 'fr';

const productCreateMutation = `#graphql
  mutation LocalizationTranslationsMutationNoopValidationProductCreate($product: ProductCreateInput!) {
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
  mutation LocalizationTranslationsMutationNoopValidationProductDelete($input: ProductDeleteInput!) {
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
  query LocalizationTranslationsMutationNoopValidationShopLocales {
    shopLocales {
      locale
      name
      primary
      published
    }
  }
`;

const shopLocaleEnableMutation = `#graphql
  mutation LocalizationTranslationsMutationNoopValidationShopLocaleEnable($locale: String!) {
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
  mutation LocalizationTranslationsMutationNoopValidationShopLocaleDisable($locale: String!) {
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
  query LocalizationTranslationsMutationNoopValidationRead($resourceId: ID!) {
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
      translations(locale: "it") {
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

const removeMutation = `#graphql
  mutation LocalizationTranslationsRemove($resourceId: ID!, $keys: [String!]!, $locales: [String!]!) {
    translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales) {
      translations {
        key
        value
        locale
      }
      userErrors {
        field
        message
        code
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

function userErrors(payload: JsonRecord): unknown[] {
  const value = payload['userErrors'];
  return Array.isArray(value) ? value : [];
}

function assertNoUserErrors(payload: JsonRecord, label: string): void {
  const errors = userErrors(payload);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function shopLocales(payload: ConformanceGraphqlPayload<unknown>): JsonRecord[] {
  const locales = dataObject(payload)['shopLocales'];
  return Array.isArray(locales) ? locales.filter(isRecord) : [];
}

function shopLocaleIsEnabled(payload: ConformanceGraphqlPayload<unknown>, locale: string): boolean {
  return shopLocales(payload).some((entry) => entry['locale'] === locale);
}

function primaryLocale(payload: ConformanceGraphqlPayload<unknown>): string {
  const primary = shopLocales(payload).find((entry) => entry['primary'] === true);
  if (!primary || typeof primary['locale'] !== 'string') {
    throw new Error(`Could not find primary shop locale: ${JSON.stringify(payload)}`);
  }
  return primary['locale'];
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
    title: `Translation noop validation ${captureToken}`,
    handle: `translation-noop-validation-${captureToken}`,
    status: 'DRAFT',
  };

  let createdProductId: string | null = null;
  let shouldRestoreItalianLocale = false;
  let cleanup: JsonRecord = {};

  try {
    const initialShopLocales = await runGraphql(shopLocalesQuery);
    const primaryLocaleCode = primaryLocale(initialShopLocales);
    if (primaryLocaleCode !== 'en') {
      throw new Error(`Expected primary locale en for this capture, got ${primaryLocaleCode}`);
    }

    shouldRestoreItalianLocale = shopLocaleIsEnabled(initialShopLocales, disabledLocale);
    const preCaptureDisable = shouldRestoreItalianLocale
      ? await runGraphql(shopLocaleDisableMutation, { locale: disabledLocale })
      : null;
    if (preCaptureDisable) {
      assertNoUserErrors(payloadField(preCaptureDisable, 'shopLocaleDisable'), 'preCapture shopLocaleDisable');
    }

    const productCreate = await runGraphql(productCreateMutation, { product: productInput });
    const productCreatePayload = payloadField(productCreate, 'productCreate');
    assertNoUserErrors(productCreatePayload, 'productCreate');
    const product = productCreatePayload['product'];
    if (!isRecord(product) || typeof product['id'] !== 'string') {
      throw new Error(`Product setup did not return a product id: ${JSON.stringify(productCreate)}`);
    }
    createdProductId = product['id'];

    const setupReadVariables = { resourceId: createdProductId };
    const setupRead = await runGraphql(setupReadQuery, setupReadVariables);
    const digest = titleDigest(setupRead);

    const unknownKeyRemoveVariables = {
      resourceId: createdProductId,
      keys: ['bogus_key'],
      locales: [unknownKeyLocale],
    };
    const unknownKeyRemove = await runGraphql(removeMutation, unknownKeyRemoveVariables);
    assertNoUserErrors(payloadField(unknownKeyRemove, 'translationsRemove'), 'unknown-key translationsRemove');

    const localeEnableVariables = { locale: disabledLocale };
    const localeEnable = await runGraphql(shopLocaleEnableMutation, localeEnableVariables);
    assertNoUserErrors(payloadField(localeEnable, 'shopLocaleEnable'), 'shopLocaleEnable');

    const disabledLocaleRegisterVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: disabledLocale,
          key: 'title',
          value: `Titolo ${captureToken}`,
          translatableContentDigest: digest,
        },
      ],
    };
    const disabledLocaleRegister = await runGraphql(registerMutation, disabledLocaleRegisterVariables);
    assertNoUserErrors(payloadField(disabledLocaleRegister, 'translationsRegister'), 'disabled-locale setup register');

    const localeDisableVariables = { locale: disabledLocale };
    const localeDisable = await runGraphql(shopLocaleDisableMutation, localeDisableVariables);
    assertNoUserErrors(payloadField(localeDisable, 'shopLocaleDisable'), 'shopLocaleDisable');

    const disabledLocaleRemoveVariables = {
      resourceId: createdProductId,
      keys: ['title'],
      locales: [disabledLocale],
    };
    const disabledLocaleRemove = await runGraphql(removeMutation, disabledLocaleRemoveVariables);
    assertNoUserErrors(payloadField(disabledLocaleRemove, 'translationsRemove'), 'disabled-locale translationsRemove');

    const primaryLocaleRegisterVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: primaryLocaleCode,
          key: 'title',
          value: `Primary ${captureToken}`,
          translatableContentDigest: digest,
        },
      ],
    };
    const primaryLocaleRegister = await runGraphql(registerMutation, primaryLocaleRegisterVariables);
    const primaryLocaleErrors = userErrors(payloadField(primaryLocaleRegister, 'translationsRegister'));
    if (primaryLocaleErrors.length !== 1) {
      throw new Error(`Expected one primary-locale userError: ${JSON.stringify(primaryLocaleRegister)}`);
    }

    cleanup = await bestEffortCleanup({ runGraphql, createdProductId, shouldRestoreItalianLocale });
    createdProductId = null;
    shouldRestoreItalianLocale = false;

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
            initialShopLocales,
            preCaptureDisable,
            productCreate: { variables: { product: productInput }, response: productCreate },
          },
          setupRead: {
            variables: setupReadVariables,
            response: setupRead,
          },
          unknownKeyRemove: {
            variables: unknownKeyRemoveVariables,
            response: unknownKeyRemove,
          },
          disabledLocaleRemove: {
            localeEnable: { variables: localeEnableVariables, response: localeEnable },
            register: { variables: disabledLocaleRegisterVariables, response: disabledLocaleRegister },
            localeDisable: { variables: localeDisableVariables, response: localeDisable },
            remove: { variables: disabledLocaleRemoveVariables, response: disabledLocaleRemove },
          },
          primaryLocaleRegister: {
            variables: primaryLocaleRegisterVariables,
            response: primaryLocaleRegister,
          },
          cleanup,
          upstreamCalls: [
            {
              operationName: 'LocalizationTranslationsMutationNoopValidationRead',
              variables: setupReadVariables,
              query: setupReadQuery,
              response: {
                status: 200,
                body: setupRead,
              },
            },
          ],
        },
        null,
        2,
      )}\n`,
      'utf8',
    );
    console.log(`Wrote ${outputPath}`);
  } finally {
    if (createdProductId !== null || shouldRestoreItalianLocale) {
      cleanup = await bestEffortCleanup({ runGraphql, createdProductId, shouldRestoreItalianLocale });
      console.log(`Cleanup after failure: ${JSON.stringify(cleanup)}`);
    }
  }
}

async function bestEffortCleanup(options: {
  runGraphql: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlPayload<unknown>>;
  createdProductId: string | null;
  shouldRestoreItalianLocale: boolean;
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
  if (options.shouldRestoreItalianLocale) {
    try {
      cleanup['shopLocaleEnable'] = await options.runGraphql(shopLocaleEnableMutation, {
        locale: disabledLocale,
      });
    } catch (error: unknown) {
      cleanup['shopLocaleEnableError'] = String(error);
    }
  }
  return cleanup;
}

await main();
