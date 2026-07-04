/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translations-validation-order';
const disabledLocale = 'it';
const missingMarketId = 'gid://shopify/Market/999999999999';

const productCreateMutation = `#graphql
  mutation LocalizationTranslationsValidationOrderProductCreate($product: ProductCreateInput!) {
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
  mutation LocalizationTranslationsValidationOrderProductDelete($input: ProductDeleteInput!) {
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
  query LocalizationTranslationsValidationOrderShopLocales {
    shopLocales {
      locale
      name
      primary
      published
    }
  }
`;

const shopLocaleEnableMutation = `#graphql
  mutation LocalizationTranslationsValidationOrderShopLocaleEnable($locale: String!) {
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
  mutation LocalizationTranslationsValidationOrderShopLocaleDisable($locale: String!) {
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
  query LocalizationTranslationsValidationOrderSetupRead($resourceId: ID!) {
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
  return Array.isArray(value) ? value.filter(isRecord) : [];
}

function assertNoUserErrors(payload: JsonRecord, label: string): void {
  const errors = userErrors(payload);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertSingleUserError(
  payload: JsonRecord,
  label: string,
  expected: { field: string[]; message: string; code: string },
): void {
  const errors = userErrors(payload);
  if (errors.length !== 1) {
    throw new Error(`${label} expected one userError, got ${JSON.stringify(errors)}`);
  }
  const [error] = errors;
  if (
    JSON.stringify(error?.['field']) !== JSON.stringify(expected.field) ||
    error?.['message'] !== expected.message ||
    error?.['code'] !== expected.code
  ) {
    throw new Error(`${label} unexpected userError: ${JSON.stringify(error)}`);
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
    title: `Translation validation order ${captureToken}`,
    handle: `translation-validation-order-${captureToken}`,
    status: 'DRAFT',
  };

  let createdProductId: string | null = null;
  let shouldRestoreDisabledLocale = false;
  let cleanup: JsonRecord = {};

  try {
    const initialShopLocales = await runGraphql(shopLocalesQuery);
    const primaryLocaleCode = primaryLocale(initialShopLocales);
    if (primaryLocaleCode !== 'en') {
      throw new Error(`Expected primary locale en for this capture, got ${primaryLocaleCode}`);
    }

    shouldRestoreDisabledLocale = shopLocaleIsEnabled(initialShopLocales, disabledLocale);
    const preCaptureDisable = shouldRestoreDisabledLocale
      ? await runGraphql(shopLocaleDisableMutation, { locale: disabledLocale })
      : null;
    if (preCaptureDisable !== null) {
      assertNoUserErrors(payloadField(preCaptureDisable, 'shopLocaleDisable'), 'pre-capture shopLocaleDisable');
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

    const blankNonEnabledLocaleVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: disabledLocale,
          key: 'title',
          value: '',
          translatableContentDigest: digest,
        },
      ],
    };
    const blankNonEnabledLocale = await runGraphql(registerMutation, blankNonEnabledLocaleVariables);
    assertSingleUserError(payloadField(blankNonEnabledLocale, 'translationsRegister'), 'blank non-enabled locale', {
      field: ['translations', '0', 'locale'],
      message: 'Locale is not a valid locale for the shop',
      code: 'INVALID_LOCALE_FOR_SHOP',
    });

    const blankPrimaryLocaleVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: primaryLocaleCode,
          key: 'title',
          value: '',
          translatableContentDigest: digest,
        },
      ],
    };
    const blankPrimaryLocale = await runGraphql(registerMutation, blankPrimaryLocaleVariables);
    assertSingleUserError(payloadField(blankPrimaryLocale, 'translationsRegister'), 'blank primary locale', {
      field: ['translations', '0', 'locale'],
      message: "Locale cannot be the same as the shop's primary locale",
      code: 'INVALID_LOCALE_FOR_SHOP',
    });

    const missingMarketNonEnabledLocaleVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: disabledLocale,
          key: 'title',
          value: 'Missing market wins over non-enabled locale',
          marketId: missingMarketId,
          translatableContentDigest: digest,
        },
      ],
    };
    const missingMarketNonEnabledLocale = await runGraphql(registerMutation, missingMarketNonEnabledLocaleVariables);
    assertSingleUserError(
      payloadField(missingMarketNonEnabledLocale, 'translationsRegister'),
      'missing market non-enabled locale',
      {
        field: ['translations', '0', 'marketId'],
        message: "The market corresponding to the `marketId` argument doesn't exist",
        code: 'MARKET_DOES_NOT_EXIST',
      },
    );

    const missingMarketPrimaryLocaleVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: primaryLocaleCode,
          key: 'title',
          value: 'Missing market wins over primary locale',
          marketId: missingMarketId,
          translatableContentDigest: digest,
        },
      ],
    };
    const missingMarketPrimaryLocale = await runGraphql(registerMutation, missingMarketPrimaryLocaleVariables);
    assertSingleUserError(
      payloadField(missingMarketPrimaryLocale, 'translationsRegister'),
      'missing market primary locale',
      {
        field: ['translations', '0', 'marketId'],
        message: "The market corresponding to the `marketId` argument doesn't exist",
        code: 'MARKET_DOES_NOT_EXIST',
      },
    );

    cleanup = await bestEffortCleanup({ runGraphql, createdProductId, shouldRestoreDisabledLocale });
    createdProductId = null;
    shouldRestoreDisabledLocale = false;

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
          blankNonEnabledLocale: {
            variables: blankNonEnabledLocaleVariables,
            response: blankNonEnabledLocale,
          },
          blankPrimaryLocale: {
            variables: blankPrimaryLocaleVariables,
            response: blankPrimaryLocale,
          },
          missingMarketNonEnabledLocale: {
            variables: missingMarketNonEnabledLocaleVariables,
            response: missingMarketNonEnabledLocale,
          },
          missingMarketPrimaryLocale: {
            variables: missingMarketPrimaryLocaleVariables,
            response: missingMarketPrimaryLocale,
          },
          cleanup,
          upstreamCalls: [
            {
              operationName: 'LocalizationTranslationsValidationOrderSetupRead',
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
    if (createdProductId !== null || shouldRestoreDisabledLocale) {
      cleanup = await bestEffortCleanup({ runGraphql, createdProductId, shouldRestoreDisabledLocale });
      console.log(`Cleanup after failure: ${JSON.stringify(cleanup)}`);
    }
  }
}

async function bestEffortCleanup(options: {
  runGraphql: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlPayload<unknown>>;
  createdProductId: string | null;
  shouldRestoreDisabledLocale: boolean;
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
  if (options.shouldRestoreDisabledLocale) {
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
