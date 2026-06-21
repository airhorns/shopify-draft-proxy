/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translations-unknown-resource';
const unknownProductId = 'gid://shopify/Product/1';

const productCreateMutation = `#graphql
  mutation LocalizationTranslationsKnownResourceProductCreate($product: ProductCreateInput!) {
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
  mutation LocalizationTranslationsKnownResourceProductDelete($input: ProductDeleteInput!) {
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
  query LocalizationTranslationsUnknownResourceShopLocales {
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
  mutation LocalizationShopLocaleDisable($locale: String!) {
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
  query LocalizationTranslationsUnknownResourceSetupRead($resourceId: ID!) {
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

const unknownResourceMutation = `#graphql
  mutation LocalizationTranslationsUnknownResource(
    $resourceId: ID!
    $translations: [TranslationInput!]!
    $keys: [String!]!
    $locales: [String!]!
  ) {
    register: translationsRegister(resourceId: $resourceId, translations: $translations) {
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
    remove: translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales) {
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
    title: `Translation unknown resource ${captureToken}`,
    handle: `translation-unknown-resource-${captureToken}`,
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
    const unknownVariables = {
      resourceId: unknownProductId,
      translations: [
        {
          locale: 'fr',
          key: 'title',
          value: `Titre missing ${captureToken}`,
          translatableContentDigest: digest,
        },
      ],
      keys: ['title'],
      locales: ['fr'],
    };
    const knownRegisterVariables = {
      resourceId: createdProductId,
      translations: [
        {
          locale: 'fr',
          key: 'title',
          value: `Titre known ${captureToken}`,
          translatableContentDigest: digest,
        },
      ],
    };
    const knownRemoveVariables = {
      resourceId: createdProductId,
      keys: ['title'],
      locales: ['fr'],
    };

    const unknownResource = await runGraphql(unknownResourceMutation, unknownVariables);
    const knownRegister = await runGraphql(registerMutation, knownRegisterVariables);
    const knownRemove = await runGraphql(removeMutation, knownRemoveVariables);

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
          unknownResource: {
            variables: unknownVariables,
            response: unknownResource,
          },
          knownRegister: {
            variables: knownRegisterVariables,
            response: knownRegister,
          },
          knownRemove: {
            variables: knownRemoveVariables,
            response: knownRemove,
          },
          cleanup,
          upstreamCalls: [
            {
              operationName: 'LocalizationTranslationsUnknownResourceSetupRead',
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
