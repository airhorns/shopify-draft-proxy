/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: ConformanceGraphqlPayload<unknown>;
};

const scenarioId = 'localization-scoped-hydration-lifecycle';
const translationLocale = 'af';
const disabledLocale = 'zu';

// These are byte-for-byte the documents emitted by the Rust mutation
// prerequisite planner. The parity cassette intentionally matches exact query
// text and variables so a production-query drift cannot masquerade as evidence.
const runtimeLocalePrerequisitesQuery =
  'query LocalizationMutationPrerequisites { availableLocales { isoCode name } shopLocales { locale name primary published marketWebPresences { id subfolderSuffix } } }';
const runtimeColdTranslationPrerequisitesQuery =
  'query LocalizationMutationPrerequisites($resourceId: ID!, $locale0: String!) { shopLocales { locale name primary published marketWebPresences { id subfolderSuffix } } translatableResource(resourceId: $resourceId) { resourceId translatableContent { key value digest locale type } translations0: translations(locale: $locale0) { key value locale outdated updatedAt market { id } } } }';
const runtimeWarmTranslationPrerequisitesQuery =
  'query LocalizationMutationPrerequisites($resourceId: ID!, $locale0: String!) { translatableResource(resourceId: $resourceId) { resourceId translatableContent { key value digest locale type } translations0: translations(locale: $locale0) { key value locale outdated updatedAt market { id } } } }';

const productCreateMutation = `#graphql
mutation LocalizationScopedHydrationProductCreate($product: ProductCreateInput!) {
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
mutation LocalizationScopedHydrationProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

const collectionCreateMutation = `#graphql
mutation LocalizationScopedHydrationCollectionCreate($input: CollectionInput!) {
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
mutation LocalizationScopedHydrationCollectionDelete($input: CollectionDeleteInput!) {
  collectionDelete(input: $input) {
    deletedCollectionId
    userErrors {
      field
      message
    }
  }
}
`;

const shopLocalesSetupQuery = `#graphql
query LocalizationScopedHydrationShopLocalesSetup {
  shopLocales {
    locale
    name
    primary
    published
    marketWebPresences {
      id
      subfolderSuffix
    }
  }
}
`;

const shopLocaleEnableMutation = `#graphql
mutation LocalizationScopedHydrationShopLocaleEnable($locale: String!, $marketWebPresenceIds: [ID!]) {
  shopLocaleEnable(locale: $locale, marketWebPresenceIds: $marketWebPresenceIds) {
    shopLocale {
      locale
      name
      primary
      published
      marketWebPresences {
        id
        subfolderSuffix
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const shopLocaleUpdateMutation = `#graphql
mutation LocalizationScopedHydrationShopLocaleUpdate($locale: String!, $shopLocale: ShopLocaleInput!) {
  shopLocaleUpdate(locale: $locale, shopLocale: $shopLocale) {
    shopLocale {
      locale
      name
      primary
      published
      marketWebPresences {
        id
        subfolderSuffix
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const shopLocaleDisableMutation = `#graphql
mutation LocalizationScopedHydrationShopLocaleDisable($locale: String!) {
  shopLocaleDisable(locale: $locale) {
    locale
    userErrors {
      field
      message
    }
  }
}
`;

const translationsRegisterMutation = `#graphql
mutation LocalizationScopedHydrationTranslationsRegister(
  $resourceId: ID!
  $translations: [TranslationInput!]!
) {
  translationsRegister(resourceId: $resourceId, translations: $translations) {
    translations {
      key
      value
      locale
      outdated
      updatedAt
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

const translationsRemoveMutation = `#graphql
mutation LocalizationScopedHydrationTranslationsRemove(
  $resourceId: ID!
  $translationKeys: [String!]!
  $locales: [String!]!
) {
  translationsRemove(resourceId: $resourceId, translationKeys: $translationKeys, locales: $locales) {
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

const mixedByIdsQuery = `#graphql
query LocalizationScopedHydrationMixedByIds($first: Int!, $resourceIds: [ID!]!) {
  translatableResourcesByIds(first: $first, resourceIds: $resourceIds) {
    nodes {
      resourceId
      translatableContent {
        key
        value
        digest
        locale
        type
      }
      translations(locale: "af") {
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
    edges {
      cursor
      node {
        resourceId
      }
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}
`;

const postRemoveReadQuery = `#graphql
query LocalizationScopedHydrationPostRemoveRead($resourceId: ID!) {
  translatableResource(resourceId: $resourceId) {
    resourceId
    translations(locale: "af") {
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
}
`;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function dataObject(payload: ConformanceGraphqlPayload<unknown>, context: string): JsonRecord {
  if (!isRecord(payload.data)) {
    throw new Error(`${context} expected GraphQL data: ${JSON.stringify(payload)}`);
  }
  return payload.data;
}

function payloadObject(payload: ConformanceGraphqlPayload<unknown>, root: string, context: string): JsonRecord {
  const value = dataObject(payload, context)[root];
  if (!isRecord(value)) {
    throw new Error(`${context} expected data.${root}: ${JSON.stringify(payload)}`);
  }
  return value;
}

function assertNoUserErrors(payload: ConformanceGraphqlPayload<unknown>, root: string, context: string): void {
  const result = payloadObject(payload, root, context);
  const errors = Array.isArray(result['userErrors']) ? result['userErrors'] : [];
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function stringField(value: unknown, field: string, context: string): string {
  if (!isRecord(value) || typeof value[field] !== 'string') {
    throw new Error(`${context} expected ${field}: ${JSON.stringify(value)}`);
  }
  return value[field];
}

function shopLocaleRows(payload: ConformanceGraphqlPayload<unknown>, root = 'shopLocales'): JsonRecord[] {
  const rows = dataObject(payload, root)[root];
  if (!Array.isArray(rows)) {
    throw new Error(`Expected ${root} array: ${JSON.stringify(payload)}`);
  }
  return rows.filter(isRecord);
}

function shopLocaleSnapshot(rows: JsonRecord[], locale: string): JsonRecord | null {
  return rows.find((row) => row['locale'] === locale) ?? null;
}

function marketWebPresenceIds(locale: JsonRecord | null): string[] {
  if (!locale || !Array.isArray(locale['marketWebPresences'])) return [];
  return locale['marketWebPresences']
    .filter(isRecord)
    .map((presence) => presence['id'])
    .filter((id): id is string => typeof id === 'string');
}

function productTitleDigest(payload: ConformanceGraphqlPayload<unknown>): string {
  const resource = dataObject(payload, 'translation prerequisites')['translatableResource'];
  if (!isRecord(resource) || !Array.isArray(resource['translatableContent'])) {
    throw new Error(`Expected translatable Product content: ${JSON.stringify(payload)}`);
  }
  const title = resource['translatableContent'].find(
    (entry) => isRecord(entry) && entry['key'] === 'title' && typeof entry['digest'] === 'string',
  );
  return stringField(title, 'digest', 'Product title translatable content');
}

function resourceIdsFromMixedRead(payload: ConformanceGraphqlPayload<unknown>): string[] {
  const connection = dataObject(payload, 'mixed ByIds read')['translatableResourcesByIds'];
  if (!isRecord(connection) || !Array.isArray(connection['nodes'])) {
    throw new Error(`Expected mixed ByIds nodes: ${JSON.stringify(payload)}`);
  }
  return connection['nodes'].map((node) => stringField(node, 'resourceId', 'mixed ByIds node'));
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
if (apiVersion !== '2026-04') {
  throw new Error(`Expected SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`);
}
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function capture(query: string, variables: JsonRecord = {}): Promise<CapturedOperation> {
  return {
    request: { query, variables },
    response: await runGraphql(query, variables),
  };
}

async function restoreLocale(locale: string, snapshot: JsonRecord | null): Promise<CapturedOperation[]> {
  const operations: CapturedOperation[] = [];
  const current = await capture(shopLocalesSetupQuery);
  const currentSnapshot = shopLocaleSnapshot(shopLocaleRows(current.response), locale);
  if (!snapshot) {
    if (currentSnapshot) {
      const disabled = await capture(shopLocaleDisableMutation, { locale });
      assertNoUserErrors(disabled.response, 'shopLocaleDisable', `cleanup disable ${locale}`);
      operations.push(disabled);
    }
    return operations;
  }

  const ids = marketWebPresenceIds(snapshot);
  if (!currentSnapshot) {
    const enabled = await capture(shopLocaleEnableMutation, {
      locale,
      marketWebPresenceIds: ids,
    });
    assertNoUserErrors(enabled.response, 'shopLocaleEnable', `cleanup enable ${locale}`);
    operations.push(enabled);
  }
  const restored = await capture(shopLocaleUpdateMutation, {
    locale,
    shopLocale: {
      published: snapshot['published'] === true,
      marketWebPresenceIds: ids,
    },
  });
  assertNoUserErrors(restored.response, 'shopLocaleUpdate', `cleanup restore ${locale}`);
  operations.push(restored);
  return operations;
}

async function main(): Promise<void> {
  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
  const outputPath = path.join(outputDir, `${scenarioId}.json`);
  const unique = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
  const slug = unique.toLowerCase();
  const missingProductId = `gid://shopify/Product/999${unique}`;

  let productId: string | null = null;
  let collectionId: string | null = null;
  let initialTranslationLocale: JsonRecord | null = null;
  let initialDisabledLocale: JsonRecord | null = null;
  const setup: JsonRecord = {};
  const cleanup: JsonRecord = {};

  try {
    const initialLocales = await capture(shopLocalesSetupQuery);
    const initialLocaleRows = shopLocaleRows(initialLocales.response);
    initialTranslationLocale = shopLocaleSnapshot(initialLocaleRows, translationLocale);
    initialDisabledLocale = shopLocaleSnapshot(initialLocaleRows, disabledLocale);
    setup['initialLocales'] = initialLocales;

    if (!initialTranslationLocale) {
      const enabled = await capture(shopLocaleEnableMutation, {
        locale: translationLocale,
        marketWebPresenceIds: [],
      });
      assertNoUserErrors(enabled.response, 'shopLocaleEnable', `enable ${translationLocale}`);
      setup['translationLocaleEnable'] = enabled;
    }
    if (!initialDisabledLocale) {
      const enabled = await capture(shopLocaleEnableMutation, {
        locale: disabledLocale,
        marketWebPresenceIds: [],
      });
      assertNoUserErrors(enabled.response, 'shopLocaleEnable', `enable ${disabledLocale}`);
      setup['disabledLocaleEnable'] = enabled;
    }

    const productCreate = await capture(productCreateMutation, {
      product: {
        title: `Localization scoped hydration ${unique}`,
        handle: `localization-scoped-hydration-${slug}`,
        status: 'DRAFT',
      },
    });
    assertNoUserErrors(productCreate.response, 'productCreate', 'productCreate setup');
    const product = payloadObject(productCreate.response, 'productCreate', 'productCreate setup')['product'];
    productId = stringField(product, 'id', 'created Product');
    setup['productCreate'] = productCreate;

    const collectionCreate = await capture(collectionCreateMutation, {
      input: {
        title: `Localization scoped collection ${unique}`,
        handle: `localization-scoped-collection-${slug}`,
      },
    });
    assertNoUserErrors(collectionCreate.response, 'collectionCreate', 'collectionCreate setup');
    const collection = payloadObject(collectionCreate.response, 'collectionCreate', 'collectionCreate setup')[
      'collection'
    ];
    collectionId = stringField(collection, 'id', 'created Collection');
    setup['collectionCreate'] = collectionCreate;

    const localePrerequisites = await capture(runtimeLocalePrerequisitesQuery);
    const localeRows = shopLocaleRows(localePrerequisites.response);
    const translationLocaleBaseline = shopLocaleSnapshot(localeRows, translationLocale);
    const disabledLocaleBaseline = shopLocaleSnapshot(localeRows, disabledLocale);
    if (!translationLocaleBaseline || !disabledLocaleBaseline) {
      throw new Error(
        `Expected enabled ${translationLocale} and ${disabledLocale} locales: ${JSON.stringify(localeRows)}`,
      );
    }

    const localeUpdateVariables = {
      locale: translationLocale,
      shopLocale: { published: translationLocaleBaseline['published'] !== true },
    };
    const localeUpdate = await capture(shopLocaleUpdateMutation, localeUpdateVariables);
    assertNoUserErrors(localeUpdate.response, 'shopLocaleUpdate', 'mutation-first shopLocaleUpdate');

    const localeDisableVariables = { locale: disabledLocale };
    const localeDisable = await capture(shopLocaleDisableMutation, localeDisableVariables);
    assertNoUserErrors(localeDisable.response, 'shopLocaleDisable', 'mutation-first shopLocaleDisable');

    const coldTranslationPrerequisiteVariables = {
      resourceId: productId,
      locale0: translationLocale,
    };
    const coldTranslationPrerequisites = await capture(
      runtimeColdTranslationPrerequisitesQuery,
      coldTranslationPrerequisiteVariables,
    );
    const digest = productTitleDigest(coldTranslationPrerequisites.response);
    const translationValue = `Afrikaanse titel ${unique}`;
    const registerVariables = {
      resourceId: productId,
      translations: [
        {
          locale: translationLocale,
          key: 'title',
          value: translationValue,
          translatableContentDigest: digest,
        },
      ],
    };
    const register = await capture(translationsRegisterMutation, registerVariables);
    assertNoUserErrors(register.response, 'translationsRegister', 'mutation-first translationsRegister');

    const mixedByIdsProductFirstVariables = {
      first: 4,
      resourceIds: [productId, collectionId, productId, missingProductId],
    };
    const mixedByIdsProductFirst = await capture(mixedByIdsQuery, mixedByIdsProductFirstVariables);
    const mixedProductFirstIds = resourceIdsFromMixedRead(mixedByIdsProductFirst.response);
    const mixedByIdsCollectionFirstVariables = {
      first: 4,
      resourceIds: [collectionId, productId, collectionId, missingProductId],
    };
    const mixedByIdsCollectionFirst = await capture(mixedByIdsQuery, mixedByIdsCollectionFirstVariables);
    const mixedCollectionFirstIds = resourceIdsFromMixedRead(mixedByIdsCollectionFirst.response);
    if (
      mixedProductFirstIds.length !== 2 ||
      mixedProductFirstIds[0] !== collectionId ||
      mixedProductFirstIds[1] !== productId ||
      mixedCollectionFirstIds.length !== 2 ||
      mixedCollectionFirstIds[0] !== collectionId ||
      mixedCollectionFirstIds[1] !== productId ||
      mixedProductFirstIds.includes(missingProductId) ||
      mixedCollectionFirstIds.includes(missingProductId)
    ) {
      throw new Error(
        `Unexpected Shopify mixed ByIds canonical order/dedup/miss semantics: ${JSON.stringify({
          productFirst: {
            requested: mixedByIdsProductFirstVariables.resourceIds,
            returned: mixedProductFirstIds,
          },
          collectionFirst: {
            requested: mixedByIdsCollectionFirstVariables.resourceIds,
            returned: mixedCollectionFirstIds,
          },
        })}`,
      );
    }

    const warmTranslationPrerequisiteVariables = {
      resourceId: productId,
      locale0: translationLocale,
    };
    const warmTranslationPrerequisites = await capture(
      runtimeWarmTranslationPrerequisitesQuery,
      warmTranslationPrerequisiteVariables,
    );
    const removeVariables = {
      resourceId: productId,
      translationKeys: ['title'],
      locales: [translationLocale],
    };
    const remove = await capture(translationsRemoveMutation, removeVariables);
    assertNoUserErrors(remove.response, 'translationsRemove', 'mutation-first translationsRemove');
    const postRemoveReadVariables = { resourceId: productId };
    const postRemoveRead = await capture(postRemoveReadQuery, postRemoveReadVariables);

    cleanup['productDelete'] = await capture(productDeleteMutation, { input: { id: productId } });
    assertNoUserErrors(
      (cleanup['productDelete'] as CapturedOperation).response,
      'productDelete',
      'productDelete cleanup',
    );
    productId = null;
    cleanup['collectionDelete'] = await capture(collectionDeleteMutation, { input: { id: collectionId } });
    assertNoUserErrors(
      (cleanup['collectionDelete'] as CapturedOperation).response,
      'collectionDelete',
      'collectionDelete cleanup',
    );
    collectionId = null;
    cleanup['translationLocale'] = await restoreLocale(translationLocale, initialTranslationLocale);
    cleanup['disabledLocale'] = await restoreLocale(disabledLocale, initialDisabledLocale);

    const captureArtifact = {
      scenarioId,
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup,
      localePrerequisites,
      localeUpdate,
      localeDisable,
      coldTranslationPrerequisites,
      register,
      mixedByIdsProductFirst,
      mixedByIdsCollectionFirst,
      warmTranslationPrerequisites,
      remove,
      postRemoveRead,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'LocalizationMutationPrerequisites',
          variables: localePrerequisites.request.variables,
          query: localePrerequisites.request.query,
          response: { status: 200, body: localePrerequisites.response },
        },
        {
          operationName: 'LocalizationMutationPrerequisites',
          variables: coldTranslationPrerequisites.request.variables,
          query: coldTranslationPrerequisites.request.query,
          response: { status: 200, body: coldTranslationPrerequisites.response },
        },
        {
          operationName: 'LocalizationScopedHydrationMixedByIds',
          variables: mixedByIdsProductFirst.request.variables,
          query: mixedByIdsProductFirst.request.query,
          response: { status: 200, body: mixedByIdsProductFirst.response },
        },
        {
          operationName: 'LocalizationScopedHydrationMixedByIds',
          variables: mixedByIdsCollectionFirst.request.variables,
          query: mixedByIdsCollectionFirst.request.query,
          response: { status: 200, body: mixedByIdsCollectionFirst.response },
        },
        {
          operationName: 'LocalizationMutationPrerequisites',
          variables: warmTranslationPrerequisites.request.variables,
          query: warmTranslationPrerequisites.request.query,
          response: { status: 200, body: warmTranslationPrerequisites.response },
        },
        {
          operationName: 'LocalizationScopedHydrationPostRemoveRead',
          variables: postRemoveRead.request.variables,
          query: postRemoveRead.request.query,
          response: { status: 200, body: postRemoveRead.response },
        },
      ],
    };

    await mkdir(outputDir, { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(captureArtifact, null, 2)}\n`, 'utf8');
    console.log(
      JSON.stringify(
        {
          ok: true,
          outputPath,
          storeDomain,
          apiVersion,
          mixedProductFirstIds,
          mixedCollectionFirstIds,
        },
        null,
        2,
      ),
    );
  } catch (error) {
    if (productId) {
      try {
        cleanup['productDeleteAfterFailure'] = await capture(productDeleteMutation, { input: { id: productId } });
      } catch (cleanupError) {
        cleanup['productDeleteAfterFailureError'] = String(cleanupError);
      }
    }
    if (collectionId) {
      try {
        cleanup['collectionDeleteAfterFailure'] = await capture(collectionDeleteMutation, {
          input: { id: collectionId },
        });
      } catch (cleanupError) {
        cleanup['collectionDeleteAfterFailureError'] = String(cleanupError);
      }
    }
    try {
      cleanup['translationLocaleAfterFailure'] = await restoreLocale(translationLocale, initialTranslationLocale);
    } catch (cleanupError) {
      cleanup['translationLocaleAfterFailureError'] = String(cleanupError);
    }
    try {
      cleanup['disabledLocaleAfterFailure'] = await restoreLocale(disabledLocale, initialDisabledLocale);
    } catch (cleanupError) {
      cleanup['disabledLocaleAfterFailureError'] = String(cleanupError);
    }
    console.error(JSON.stringify({ cleanupAfterFailure: cleanup }, null, 2));
    throw error;
  }
}

await main();
