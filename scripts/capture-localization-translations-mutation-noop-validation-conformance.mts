/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translations-mutation-noop-validation';
const emptyKeysLocale = 'fr';
const disabledLocale = 'it';
const unknownKeyLocale = 'fr';
const setupReadDocumentPath =
  'config/parity-requests/localization/localization-translations-mutation-noop-validation-read.graphql';

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

const translationsReadQuery = `#graphql
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

function translatableContentDigest(payload: ConformanceGraphqlPayload<unknown>, key: string): string {
  const resource = dataObject(payload)['translatableResource'];
  if (!isRecord(resource) || !Array.isArray(resource['translatableContent'])) {
    throw new Error(`Setup read did not return translatable content: ${JSON.stringify(payload)}`);
  }
  const content = resource['translatableContent'].find(
    (entry) => isRecord(entry) && entry['key'] === key && typeof entry['digest'] === 'string',
  );
  if (!isRecord(content) || typeof content['digest'] !== 'string') {
    throw new Error(`Setup read did not return a ${key} digest: ${JSON.stringify(payload)}`);
  }
  return content['digest'];
}

async function main(): Promise<void> {
  const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
  const setupReadQuery = await readFile(setupReadDocumentPath, 'utf8');
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
    descriptionHtml: `<p>Translation noop validation body ${captureToken}</p>`,
    status: 'DRAFT',
  };

  let createdProductId: string | null = null;
  let shouldRestoreItalianLocale = false;
  let shouldRestoreFrenchLocale = false;
  let shouldDisableFrenchLocale = false;
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
    const initiallyFrenchEnabled = shopLocaleIsEnabled(initialShopLocales, emptyKeysLocale);
    shouldRestoreFrenchLocale = initiallyFrenchEnabled;
    const preCaptureFrenchDisable = initiallyFrenchEnabled
      ? await runGraphql(shopLocaleDisableMutation, { locale: emptyKeysLocale })
      : null;
    if (preCaptureFrenchDisable) {
      assertNoUserErrors(
        payloadField(preCaptureFrenchDisable, 'shopLocaleDisable'),
        'preCapture French shopLocaleDisable',
      );
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
    const digest = translatableContentDigest(setupRead, 'title');
    const bodyDigest = translatableContentDigest(setupRead, 'body_html');

    const unknownKeyRemoveVariables = {
      resourceId: createdProductId,
      keys: ['bogus_key'],
      locales: [unknownKeyLocale],
    };
    const unknownKeyRemove = await runGraphql(removeMutation, unknownKeyRemoveVariables);
    assertNoUserErrors(payloadField(unknownKeyRemove, 'translationsRemove'), 'unknown-key translationsRemove');

    const emptyKeysLocaleEnableVariables = { locale: emptyKeysLocale };
    const emptyKeysLocaleEnable = await runGraphql(shopLocaleEnableMutation, emptyKeysLocaleEnableVariables);
    assertNoUserErrors(payloadField(emptyKeysLocaleEnable, 'shopLocaleEnable'), 'empty-keys shopLocaleEnable');
    shouldRestoreFrenchLocale = false;
    shouldDisableFrenchLocale = !initiallyFrenchEnabled;

    const emptyKeysRegisterVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: emptyKeysLocale,
          key: 'title',
          value: `Titre ${captureToken}`,
          translatableContentDigest: digest,
        },
        {
          locale: emptyKeysLocale,
          key: 'body_html',
          value: `<p>Description ${captureToken}</p>`,
          translatableContentDigest: bodyDigest,
        },
      ],
    };
    const emptyKeysRegister = await runGraphql(registerMutation, emptyKeysRegisterVariables);
    assertNoUserErrors(payloadField(emptyKeysRegister, 'translationsRegister'), 'empty-keys translationsRegister');

    const emptyKeysRemoveVariables = {
      resourceId: createdProductId,
      keys: [],
      locales: [emptyKeysLocale],
    };
    const emptyKeysRemove = await runGraphql(removeMutation, emptyKeysRemoveVariables);
    assertNoUserErrors(payloadField(emptyKeysRemove, 'translationsRemove'), 'empty-keys translationsRemove');

    const emptyKeysReadAfterVariables = { resourceId: createdProductId };
    const emptyKeysReadAfter = await runGraphql(translationsReadQuery, emptyKeysReadAfterVariables);

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

    const disabledLocaleRegisterAfterDisableVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: disabledLocale,
          key: 'title',
          value: `Disabled ${captureToken}`,
          translatableContentDigest: digest,
        },
      ],
    };
    const disabledLocaleRegisterAfterDisable = await runGraphql(
      registerMutation,
      disabledLocaleRegisterAfterDisableVariables,
    );
    const disabledLocaleRegisterAfterDisableErrors = userErrors(
      payloadField(disabledLocaleRegisterAfterDisable, 'translationsRegister'),
    );
    if (disabledLocaleRegisterAfterDisableErrors.length !== 1) {
      throw new Error(
        `Expected one disabled-locale translationsRegister userError: ${JSON.stringify(disabledLocaleRegisterAfterDisable)}`,
      );
    }

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

    cleanup = await bestEffortCleanup({
      runGraphql,
      createdProductId,
      shouldRestoreItalianLocale,
      shouldRestoreFrenchLocale,
      shouldDisableFrenchLocale,
    });
    createdProductId = null;
    shouldRestoreItalianLocale = false;
    shouldRestoreFrenchLocale = false;
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
            initialShopLocales,
            preCaptureDisable,
            preCaptureFrenchDisable,
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
          emptyKeysRemove: {
            localeEnable: { variables: emptyKeysLocaleEnableVariables, response: emptyKeysLocaleEnable },
            register: { variables: emptyKeysRegisterVariables, response: emptyKeysRegister },
            remove: { variables: emptyKeysRemoveVariables, response: emptyKeysRemove },
            readAfter: { variables: emptyKeysReadAfterVariables, response: emptyKeysReadAfter },
          },
          disabledLocaleRemove: {
            localeEnable: { variables: localeEnableVariables, response: localeEnable },
            register: { variables: disabledLocaleRegisterVariables, response: disabledLocaleRegister },
            localeDisable: { variables: localeDisableVariables, response: localeDisable },
            remove: { variables: disabledLocaleRemoveVariables, response: disabledLocaleRemove },
            registerAfterDisable: {
              variables: disabledLocaleRegisterAfterDisableVariables,
              response: disabledLocaleRegisterAfterDisable,
            },
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
    if (
      createdProductId !== null ||
      shouldRestoreItalianLocale ||
      shouldRestoreFrenchLocale ||
      shouldDisableFrenchLocale
    ) {
      cleanup = await bestEffortCleanup({
        runGraphql,
        createdProductId,
        shouldRestoreItalianLocale,
        shouldRestoreFrenchLocale,
        shouldDisableFrenchLocale,
      });
      console.log(`Cleanup after failure: ${JSON.stringify(cleanup)}`);
    }
  }
}

async function bestEffortCleanup(options: {
  runGraphql: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlPayload<unknown>>;
  createdProductId: string | null;
  shouldRestoreItalianLocale: boolean;
  shouldRestoreFrenchLocale: boolean;
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
  if (options.shouldRestoreItalianLocale) {
    try {
      cleanup['shopLocaleEnable'] = await options.runGraphql(shopLocaleEnableMutation, {
        locale: disabledLocale,
      });
    } catch (error: unknown) {
      cleanup['shopLocaleEnableError'] = String(error);
    }
  }
  if (options.shouldDisableFrenchLocale) {
    try {
      cleanup['shopLocaleDisableFr'] = await options.runGraphql(shopLocaleDisableMutation, {
        locale: emptyKeysLocale,
      });
    } catch (error: unknown) {
      cleanup['shopLocaleDisableFrError'] = String(error);
    }
  }
  if (options.shouldRestoreFrenchLocale) {
    try {
      cleanup['shopLocaleEnableFr'] = await options.runGraphql(shopLocaleEnableMutation, {
        locale: emptyKeysLocale,
      });
    } catch (error: unknown) {
      cleanup['shopLocaleEnableFrError'] = String(error);
    }
  }
  return cleanup;
}

await main();
