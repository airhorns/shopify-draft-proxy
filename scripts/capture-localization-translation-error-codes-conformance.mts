/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translations-error-codes';

const productCreateMutation = `#graphql
  mutation LocalizationTranslationErrorCodesProductCreate($product: ProductCreateInput!) {
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
  mutation LocalizationTranslationErrorCodesProductDelete($input: ProductDeleteInput!) {
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
  query LocalizationTranslationErrorCodesShopLocales {
    shopLocales {
      locale
      name
      primary
      published
    }
  }
`;

const shopLocaleEnableMutation = `#graphql
  mutation LocalizationTranslationErrorCodesShopLocaleEnable($locale: String!) {
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
  mutation LocalizationTranslationErrorCodesShopLocaleDisable($locale: String!) {
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
  query LocalizationTranslationsErrorCodesSetupRead($resourceId: ID!) {
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

function assertNoUserErrors(payload: JsonRecord, label: string): void {
  const userErrors = payload['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
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
    title: `HAR-710 translation errors ${captureToken}`,
    handle: `har-710-translation-errors-${captureToken}`,
    status: 'DRAFT',
  };

  let createdProductId: string | null = null;
  let shouldDisableFrenchLocale = false;
  let localeSetupCapture: ConformanceGraphqlPayload<unknown> | null = null;
  let cleanup: JsonRecord = {};

  try {
    const initialShopLocales = await runGraphql(shopLocalesQuery);
    shouldDisableFrenchLocale = !shopLocaleIsEnabled(initialShopLocales, 'fr');
    const localeSetup = shouldDisableFrenchLocale
      ? await runGraphql(shopLocaleEnableMutation, { locale: 'fr' })
      : initialShopLocales;
    localeSetupCapture = shouldDisableFrenchLocale ? localeSetup : null;
    if (shouldDisableFrenchLocale) {
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

    const setupReadVariables = { resourceId: createdProductId };
    const setupRead = await runGraphql(setupReadQuery, setupReadVariables);
    const digest = titleDigest(setupRead);
    const validTranslation = {
      locale: 'fr',
      key: 'title',
      value: `Titre HAR-710 ${captureToken}`,
      translatableContentDigest: digest,
    };
    const emptyRegisterVariables = {
      resourceId: createdProductId,
      translations: [],
    };
    const blankValueVariables = {
      resourceId: createdProductId,
      translations: [
        {
          ...validTranslation,
          value: '',
        },
      ],
    };
    const tooManyKeysVariables = {
      resourceId: createdProductId,
      translations: Array.from({ length: 101 }, () => validTranslation),
    };
    const removeEmptyLocalesVariables = {
      resourceId: createdProductId,
      keys: ['title'],
      locales: [],
    };

    const emptyRegister = await runGraphql(registerMutation, emptyRegisterVariables);
    const blankValue = await runGraphql(registerMutation, blankValueVariables);
    const tooManyKeys = await runGraphql(registerMutation, tooManyKeysVariables);
    const removeEmptyLocales = await runGraphql(removeMutation, removeEmptyLocalesVariables);

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
            productCreate: { variables: { product: productInput }, response: productCreate },
            localeSetup: localeSetupCapture,
          },
          setupRead: {
            variables: setupReadVariables,
            response: setupRead,
          },
          emptyRegister: {
            variables: emptyRegisterVariables,
            response: emptyRegister,
          },
          blankValue: {
            variables: blankValueVariables,
            response: blankValue,
          },
          tooManyKeys: {
            variables: tooManyKeysVariables,
            response: tooManyKeys,
          },
          removeEmptyLocales: {
            variables: removeEmptyLocalesVariables,
            response: removeEmptyLocales,
          },
          cleanup,
          upstreamCalls: [
            {
              operationName: 'LocalizationTranslationsErrorCodesSetupRead',
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
