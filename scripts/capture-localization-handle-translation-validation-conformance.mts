/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-handle-translation-validation';

const productCreateMutation = `#graphql
  mutation LocalizationHandleTranslationProductCreate($product: ProductCreateInput!) {
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
  mutation LocalizationHandleTranslationProductDelete($input: ProductDeleteInput!) {
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
  query LocalizationHandleTranslationShopLocales {
    shopLocales {
      locale
      name
      primary
      published
    }
  }
`;

const shopLocaleEnableMutation = `#graphql
  mutation LocalizationHandleTranslationShopLocaleEnable($locale: String!) {
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
  mutation LocalizationHandleTranslationShopLocaleDisable($locale: String!) {
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

const readTranslationsQuery = `#graphql
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

function assertHandleTranslationValue(payload: JsonRecord, label: string, expectedValue: string): void {
  assertNoUserErrors(payload, label);
  const translations = payload['translations'];
  if (!Array.isArray(translations) || translations.length !== 1 || !isRecord(translations[0])) {
    throw new Error(`${label} did not return exactly one translation: ${JSON.stringify(payload)}`);
  }
  if (translations[0]['key'] !== 'handle' || translations[0]['value'] !== expectedValue) {
    throw new Error(`${label} did not return normalized handle ${expectedValue}: ${JSON.stringify(payload)}`);
  }
}

function assertTooLongHandleError(payload: JsonRecord, label: string): void {
  const errors = userErrors(payload);
  if (
    errors.length !== 1 ||
    errors[0]?.['code'] !== 'FAILS_RESOURCE_VALIDATION' ||
    JSON.stringify(errors[0]?.['field']) !== JSON.stringify(['translations', '0', 'value']) ||
    errors[0]?.['message'] !== 'Value fails validation on resource: ["Handle is too long (maximum is 255 characters)"]'
  ) {
    throw new Error(`${label} did not return the expected too-long handle error: ${JSON.stringify(payload)}`);
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

function handleDigest(payload: ConformanceGraphqlPayload<unknown>): string {
  const resource = dataObject(payload)['translatableResource'];
  if (!isRecord(resource) || !Array.isArray(resource['translatableContent'])) {
    throw new Error(`Setup read did not return translatable content: ${JSON.stringify(payload)}`);
  }
  const handle = resource['translatableContent'].find(
    (entry) => isRecord(entry) && entry['key'] === 'handle' && typeof entry['digest'] === 'string',
  );
  if (!isRecord(handle) || typeof handle['digest'] !== 'string') {
    throw new Error(`Setup read did not return a handle digest: ${JSON.stringify(payload)}`);
  }
  return handle['digest'];
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
    title: `Localization handle translation ${captureToken}`,
    handle: `localization-handle-translation-${captureToken}`,
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
    const digest = handleDigest(setupRead);

    const tooLongHandleVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: 'fr',
          key: 'handle',
          value: 'a'.repeat(256),
          translatableContentDigest: digest,
        },
      ],
    };
    const tooLongHandle = await runGraphql(registerMutation, tooLongHandleVariables);
    assertTooLongHandleError(payloadField(tooLongHandle, 'translationsRegister'), 'too-long handle');

    const readAfterTooLongHandleVariables = { resourceId: createdProductId };
    const readAfterTooLongHandle = await runGraphql(readTranslationsQuery, readAfterTooLongHandleVariables);

    const normalizedCases = [
      ['normalizedHandleSpaces', 'Bad Value With Spaces', 'bad-value-with-spaces'],
      ['normalizedHandleUppercase', 'UPPER', 'upper'],
      ['normalizedHandleLeadingDash', '-leading-dash', 'leading-dash'],
      ['normalizedHandleTrailingDash', 'trailing-dash-', 'trailing-dash'],
      ['normalizedHandleDoubleDash', 'double--dash', 'double-dash'],
      ['normalizedHandlePunctuationOnly', '%%%', 'store-localization/generic-dynamic-content-translation'],
      ['normalizedHandleSlash', 'a/b', 'a-b'],
    ] as const;

    const normalizedCaptures: Record<string, { variables: JsonRecord; response: ConformanceGraphqlPayload<unknown> }> =
      {};
    for (const [label, value, expectedValue] of normalizedCases) {
      const variables = {
        resourceId: createdProductId,
        translations: [
          {
            locale: 'fr',
            key: 'handle',
            value,
            translatableContentDigest: digest,
          },
        ],
      };
      const response = await runGraphql(registerMutation, variables);
      assertHandleTranslationValue(payloadField(response, 'translationsRegister'), label, expectedValue);
      normalizedCaptures[label] = { variables, response };
    }

    const validHandleValue = `localisation-handle-${captureToken}`;
    const validHandleVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: 'fr',
          key: 'handle',
          value: validHandleValue,
          translatableContentDigest: digest,
        },
      ],
    };
    const validHandle = await runGraphql(registerMutation, validHandleVariables);
    assertNoUserErrors(payloadField(validHandle, 'translationsRegister'), 'valid handle translationsRegister');

    const readAfterValidHandleVariables = { resourceId: createdProductId };
    const readAfterValidHandle = await runGraphql(readTranslationsQuery, readAfterValidHandleVariables);

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
          tooLongHandle: {
            variables: tooLongHandleVariables,
            response: tooLongHandle,
          },
          readAfterTooLongHandle: {
            variables: readAfterTooLongHandleVariables,
            response: readAfterTooLongHandle,
          },
          ...normalizedCaptures,
          validHandle: {
            variables: validHandleVariables,
            response: validHandle,
          },
          readAfterValidHandle: {
            variables: readAfterValidHandleVariables,
            response: readAfterValidHandle,
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
