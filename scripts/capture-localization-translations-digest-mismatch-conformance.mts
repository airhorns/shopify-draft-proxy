/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translations-digest-mismatch';

const productCreateMutation = `#graphql
  mutation LocalizationTranslationsDigestProductCreate($product: ProductCreateInput!) {
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

const productUpdateMutation = `#graphql
  mutation LocalizationTranslationsDigestProductUpdate($product: ProductUpdateInput!) {
    productUpdate(product: $product) {
      product {
        id
        title
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation LocalizationTranslationsDigestProductDelete($input: ProductDeleteInput!) {
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
  query LocalizationTranslationsDigestShopLocales {
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
  mutation LocalizationTranslationsDigestShopLocaleDisable($locale: String!) {
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
  query LocalizationTranslationsDigestSetupRead($resourceId: ID!) {
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

function assertInvalidDigest(payload: JsonRecord, label: string): void {
  const errors = userErrors(payload);
  if (
    errors.length !== 1 ||
    JSON.stringify(errors[0]?.['field']) !== JSON.stringify(['translations', '0', 'translatableContentDigest']) ||
    errors[0]?.['message'] !== 'Translatable content hash is invalid' ||
    errors[0]?.['code'] !== 'INVALID_TRANSLATABLE_CONTENT'
  ) {
    throw new Error(`${label} did not return INVALID_TRANSLATABLE_CONTENT: ${JSON.stringify(payload)}`);
  }
  const translations = payload['translations'];
  if (!Array.isArray(translations) || translations.length !== 0) {
    throw new Error(`${label} unexpectedly returned translations: ${JSON.stringify(payload)}`);
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
  const originalTitle = `Digest validation ${captureToken}`;
  const updatedTitle = `Digest validation updated ${captureToken}`;
  const productInput = {
    title: originalTitle,
    handle: `digest-validation-${captureToken}`,
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
    const correctDigestVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: 'fr',
          key: 'title',
          value: `Titre digest ${captureToken}`,
          translatableContentDigest: currentDigest,
        },
      ],
    };
    const wrongDigestVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: 'fr',
          key: 'title',
          value: `Titre wrong digest ${captureToken}`,
          translatableContentDigest: 'deadbeef0000000000000000000000000000000000000000000000000000dead',
        },
      ],
    };

    const correctDigestRegister = await runGraphql(registerMutation, correctDigestVariables);
    assertNoUserErrors(
      payloadField(correctDigestRegister, 'translationsRegister'),
      'correct digest translationsRegister',
    );
    const wrongDigestRegister = await runGraphql(registerMutation, wrongDigestVariables);
    assertInvalidDigest(payloadField(wrongDigestRegister, 'translationsRegister'), 'wrong digest translationsRegister');

    const productUpdateVariables = { product: { id: createdProductId, title: updatedTitle } };
    const productUpdate = await runGraphql(productUpdateMutation, productUpdateVariables);
    assertNoUserErrors(payloadField(productUpdate, 'productUpdate'), 'productUpdate');

    const staleDigestVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: 'fr',
          key: 'title',
          value: `Titre stale digest ${captureToken}`,
          translatableContentDigest: currentDigest,
        },
      ],
    };
    const staleDigestRegister = await runGraphql(registerMutation, staleDigestVariables);
    assertInvalidDigest(payloadField(staleDigestRegister, 'translationsRegister'), 'stale digest translationsRegister');

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
            productUpdate: {
              variables: productUpdateVariables,
              response: productUpdate,
            },
          },
          setupRead: {
            variables: setupReadVariables,
            response: setupRead,
          },
          correctDigestRegister: {
            variables: correctDigestVariables,
            response: correctDigestRegister,
          },
          wrongDigestRegister: {
            variables: wrongDigestVariables,
            response: wrongDigestRegister,
          },
          staleDigestRegister: {
            variables: staleDigestVariables,
            response: staleDigestRegister,
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
