/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-payload-shapes';

const setupReadQuery = `#graphql
  query LocalizationLocaleTranslationRead($first: Int!, $resourceType: TranslatableResourceType!, $ids: [ID!]!) {
    availableLocalesExcerpt: availableLocales {
      isoCode
      name
    }
    allShopLocales: shopLocales {
      locale
      name
      primary
      published
      marketWebPresences {
        id
        subfolderSuffix
      }
    }
    publishedShopLocales: shopLocales(published: true) {
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
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    byIds: translatableResourcesByIds(first: $first, resourceIds: $ids) {
      nodes {
        resourceId
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
    missing: translatableResource(resourceId: "gid://shopify/Product/999999999999999") {
      resourceId
    }
  }
`;

const enableWithMarketWebPresenceMutation = `#graphql
  mutation LocalizationPayloadShapesShopLocaleEnable($locale: String!, $marketWebPresenceIds: [ID!]) {
    shopLocaleEnable(locale: $locale, marketWebPresenceIds: $marketWebPresenceIds) {
      shopLocale {
        locale
        published
        marketWebPresences {
          id
          __typename
          defaultLocale {
            locale
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const shopLocalesReadQuery = `#graphql
  query LocalizationPayloadShapesShopLocalesRead {
    shopLocales {
      locale
      marketWebPresences {
        id
        __typename
        defaultLocale {
          locale
        }
      }
    }
  }
`;

const disableMutation = `#graphql
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
  mutation LocalizationPayloadShapesTranslationsRemove(
    $resourceId: ID!
    $keys: [String!]!
    $locales: [String!]!
  ) {
    translationsRemove(resourceId: $resourceId, translationKeys: $keys, locales: $locales) {
      translations {
        key
      }
      userErrors {
        field
        message
        code
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

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
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

function arrayField(value: unknown, context: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`Expected ${context} array: ${JSON.stringify(value)}`);
  }
  return value;
}

function stringField(value: unknown, fieldName: string, context: string): string {
  if (!isRecord(value) || typeof value[fieldName] !== 'string') {
    throw new Error(`Expected ${context}.${fieldName} string: ${JSON.stringify(value)}`);
  }
  return value[fieldName];
}

function shopLocales(payload: JsonRecord): JsonRecord[] {
  return arrayField(dataObject(payload)['allShopLocales'], 'allShopLocales').filter(isRecord);
}

function productTitleContent(payload: JsonRecord): { resourceId: string; digest: string } {
  const nodes = arrayField(
    isRecord(dataObject(payload)['resources']) ? dataObject(payload)['resources']['nodes'] : null,
    'resources.nodes',
  );
  for (const node of nodes) {
    if (!isRecord(node)) {
      continue;
    }
    const resourceId = stringField(node, 'resourceId', 'resource node');
    const content = arrayField(node['translatableContent'], 'translatableContent');
    const title = content.find(
      (entry) => isRecord(entry) && entry['key'] === 'title' && typeof entry['digest'] === 'string',
    );
    if (isRecord(title)) {
      return { resourceId, digest: stringField(title, 'digest', 'title content') };
    }
  }
  throw new Error('Could not find product title translatable content digest in setup read.');
}

function firstMarketWebPresenceId(locales: JsonRecord[]): string {
  for (const locale of locales) {
    const presences = Array.isArray(locale['marketWebPresences']) ? locale['marketWebPresences'] : [];
    const presence = presences.find((entry) => isRecord(entry) && typeof entry['id'] === 'string');
    if (isRecord(presence)) {
      return stringField(presence, 'id', 'marketWebPresence');
    }
  }
  throw new Error('Could not find a MarketWebPresence id from shopLocales.');
}

function localeSnapshot(locales: JsonRecord[], locale: string): JsonRecord | null {
  return locales.find((entry) => entry['locale'] === locale) ?? null;
}

const setupVariables = {
  first: 3,
  resourceType: 'PRODUCT',
  ids: ['gid://shopify/Product/999999999999999'],
};

const cleanup: JsonRecord = {};
let setupRead = (await runGraphql(setupReadQuery, setupVariables)) as JsonRecord;
let initialLocales = shopLocales(setupRead);
if (localeSnapshot(initialLocales, 'fr')) {
  cleanup['preCaptureShopLocaleDisable'] = await runGraphql(disableMutation, { locale: 'fr' });
  setupRead = (await runGraphql(setupReadQuery, setupVariables)) as JsonRecord;
  initialLocales = shopLocales(setupRead);
}
const primaryLocale = initialLocales.find((entry) => entry['primary'] === true);
if (!primaryLocale) {
  throw new Error(`Could not find primary shop locale: ${JSON.stringify(initialLocales)}`);
}
const primaryLocaleCode = stringField(primaryLocale, 'locale', 'primary shop locale');
const marketWebPresenceId = firstMarketWebPresenceId(initialLocales);
const product = productTitleContent(setupRead);

const enableVariables = {
  locale: 'fr',
  marketWebPresenceIds: [marketWebPresenceId],
};
const registerVariables = {
  resourceId: product.resourceId,
  translations: [
    {
      locale: 'fr',
      key: 'title',
      value: `HAR-711 payload shape ${Date.now()}`,
      translatableContentDigest: product.digest,
    },
    {
      locale: 'fr',
      key: 'title',
      value: 'Invalid digest row',
      translatableContentDigest: `invalid-${product.digest}`,
    },
  ],
};

try {
  const enable = (await runGraphql(enableWithMarketWebPresenceMutation, enableVariables)) as JsonRecord;
  assertNoUserErrors(payloadField(enable, 'shopLocaleEnable'), 'shopLocaleEnable');
  const readAfterEnable = (await runGraphql(shopLocalesReadQuery, {})) as JsonRecord;
  const disablePrimaryVariables = { locale: primaryLocaleCode };
  const disablePrimary = (await runGraphql(disableMutation, disablePrimaryVariables)) as JsonRecord;
  const registerMixed = (await runGraphql(registerMutation, registerVariables)) as JsonRecord;

  cleanup['translationsRemove'] = await runGraphql(removeMutation, {
    resourceId: product.resourceId,
    keys: ['title'],
    locales: ['fr'],
  });
  cleanup['shopLocaleDisable'] = await runGraphql(disableMutation, { locale: 'fr' });

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId,
    setup: {
      request: { variables: setupVariables },
      response: setupRead,
    },
    enable: {
      request: { variables: enableVariables },
      response: enable,
    },
    readAfterEnable: {
      request: { variables: {} },
      response: readAfterEnable,
    },
    disablePrimary: {
      request: { variables: disablePrimaryVariables },
      response: disablePrimary,
    },
    registerMixed: {
      request: { variables: registerVariables },
      response: registerMixed,
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'LocalizationLocaleTranslationRead',
        variables: setupVariables,
        query: 'sha:hand-synthesized-from-setup-response',
        response: {
          status: 200,
          body: setupRead,
        },
      },
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`wrote ${outputPath}`);
} catch (error) {
  cleanup['error'] = error instanceof Error ? error.message : String(error);
  throw error;
}
