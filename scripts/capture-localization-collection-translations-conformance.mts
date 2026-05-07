/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-collection-translation-lifecycle';

const collectionCreateMutation = `#graphql
  mutation LocalizationCollectionTranslationCollectionCreate($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        id
        title
        handle
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionDeleteMutation = `#graphql
  mutation LocalizationCollectionTranslationCollectionDelete($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const shopLocalesQuery = `#graphql
  query LocalizationCollectionTranslationShopLocales {
    shopLocales {
      locale
      name
      primary
      published
    }
  }
`;

const shopLocaleEnableMutation = `#graphql
  mutation LocalizationCollectionTranslationShopLocaleEnable($locale: String!) {
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
  mutation LocalizationCollectionTranslationShopLocaleDisable($locale: String!) {
    shopLocaleDisable(locale: $locale) {
      locale
      userErrors {
        field
        message
      }
    }
  }
`;

const readQuery = `#graphql
  query LocalizationCollectionTranslationRead(
    $first: Int!
    $resourceType: TranslatableResourceType!
    $ids: [ID!]!
    $resourceId: ID!
  ) {
    allShopLocales: shopLocales {
      locale
      name
      primary
      published
    }
    resources: translatableResources(first: $first, resourceType: $resourceType) {
      nodes {
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
    byIds: translatableResourcesByIds(first: $first, resourceIds: $ids) {
      nodes {
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
    single: translatableResource(resourceId: $resourceId) {
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

const singleTranslationReadQuery = `#graphql
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function graphql(query: string, variables: Record<string, unknown> = {}): Promise<JsonRecord> {
  const payload = await runGraphql<JsonRecord>(query, variables);
  return payload as JsonRecord;
}

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

function collectionTitleDigest(payload: JsonRecord): string {
  const single = dataObject(payload)['single'];
  if (!isRecord(single) || !Array.isArray(single['translatableContent'])) {
    throw new Error(`Expected single Collection translatable content: ${JSON.stringify(payload)}`);
  }

  for (const item of single['translatableContent']) {
    if (isRecord(item) && item['key'] === 'title' && typeof item['digest'] === 'string') {
      return item['digest'];
    }
  }

  throw new Error('Could not find Collection title digest in localization read capture.');
}

async function bestEffortCleanup(options: {
  collectionId: string | null;
  shouldDisableFrenchLocale: boolean;
}): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};

  if (options.collectionId) {
    try {
      cleanup['translationsRemove'] = await graphql(removeMutation, {
        resourceId: options.collectionId,
        keys: ['title'],
        locales: ['fr'],
      });
    } catch (error) {
      cleanup['translationsRemoveError'] = String(error);
    }

    try {
      cleanup['collectionDelete'] = await graphql(collectionDeleteMutation, {
        input: { id: options.collectionId },
      });
    } catch (error) {
      cleanup['collectionDeleteError'] = String(error);
    }
  }

  if (options.shouldDisableFrenchLocale) {
    try {
      cleanup['shopLocaleDisable'] = await graphql(shopLocaleDisableMutation, { locale: 'fr' });
    } catch (error) {
      cleanup['shopLocaleDisableError'] = String(error);
    }
  }

  return cleanup;
}

const captureToken = randomSuffix();
const collectionInput = {
  title: `Localization Collection ${captureToken}`,
  handle: `localization-collection-${captureToken}`,
};

let collectionId: string | null = null;
let shouldDisableFrenchLocale = false;
let cleanup: JsonRecord = {};

try {
  const initialShopLocales = await graphql(shopLocalesQuery);
  shouldDisableFrenchLocale = !shopLocaleIsEnabled(initialShopLocales, 'fr');
  const localeSetup = shouldDisableFrenchLocale
    ? await graphql(shopLocaleEnableMutation, { locale: 'fr' })
    : initialShopLocales;
  if (shouldDisableFrenchLocale) {
    assertNoUserErrors(payloadField(localeSetup, 'shopLocaleEnable'), 'shopLocaleEnable');
  }

  const collectionCreate = await graphql(collectionCreateMutation, { input: collectionInput });
  const collectionCreatePayload = payloadField(collectionCreate, 'collectionCreate');
  assertNoUserErrors(collectionCreatePayload, 'collectionCreate');
  const collection = collectionCreatePayload['collection'];
  if (!isRecord(collection) || typeof collection['id'] !== 'string') {
    throw new Error(`Collection setup did not return a Collection id: ${JSON.stringify(collectionCreate)}`);
  }
  collectionId = collection['id'];

  const readVariables = {
    first: 1,
    resourceType: 'COLLECTION',
    ids: [collectionId],
    resourceId: collectionId,
  };
  const removeVariables = {
    resourceId: collectionId,
    keys: ['title'],
    locales: ['fr'],
  };

  const readBeforeRegister = await graphql(readQuery, readVariables);
  const digest = collectionTitleDigest(readBeforeRegister);
  const translationValue = `Collection francaise ${captureToken}`;
  const registerVariables = {
    resourceId: collectionId,
    translations: [
      {
        locale: 'fr',
        key: 'title',
        value: translationValue,
        translatableContentDigest: digest,
      },
    ],
  };

  const removeBeforeRegister = await graphql(removeMutation, removeVariables);
  assertNoUserErrors(payloadField(removeBeforeRegister, 'translationsRemove'), 'translationsRemove no-op');
  const register = await graphql(registerMutation, registerVariables);
  assertNoUserErrors(payloadField(register, 'translationsRegister'), 'translationsRegister');
  const readAfterRegister = await graphql(readQuery, readVariables);
  const remove = await graphql(removeMutation, removeVariables);
  assertNoUserErrors(payloadField(remove, 'translationsRemove'), 'translationsRemove');
  const readAfterRemove = await graphql(readQuery, readVariables);
  const readAfterRemoveSingle = await graphql(singleTranslationReadQuery, { resourceId: collectionId });

  cleanup = await bestEffortCleanup({ collectionId, shouldDisableFrenchLocale });

  const capture = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    setup: {
      disposableCollection: collection,
      localeWasInitiallyEnabled: !shouldDisableFrenchLocale,
    },
    readBeforeRegister: {
      request: { variables: readVariables },
      response: readBeforeRegister,
    },
    removeBeforeRegister: {
      request: { variables: removeVariables },
      response: removeBeforeRegister,
    },
    register: {
      request: { variables: registerVariables },
      response: register,
    },
    readAfterRegister: {
      request: { variables: readVariables },
      response: readAfterRegister,
    },
    remove: {
      request: { variables: removeVariables },
      response: remove,
    },
    readAfterRemove: {
      request: { variables: readVariables },
      response: readAfterRemove,
    },
    readAfterRemoveSingle: {
      request: { variables: { resourceId: collectionId } },
      response: readAfterRemoveSingle,
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'LocalizationCollectionTranslationRead',
        variables: readVariables,
        query: readQuery,
        response: {
          status: 200,
          body: readBeforeRegister,
        },
      },
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath, storeDomain, apiVersion, collectionId }, null, 2));
} catch (error) {
  cleanup = await bestEffortCleanup({ collectionId, shouldDisableFrenchLocale });
  console.error(JSON.stringify({ cleanupAfterFailure: cleanup }, null, 2));
  throw error;
}
