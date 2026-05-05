import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
const disableCleanupOutputPath = path.join(outputDir, 'localization-disable-clears-translations.json');
const primaryGuardsOutputPath = path.join(outputDir, 'localization-shop-locale-primary-guards.json');

const readQuery = `#graphql
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

const enableMutation = `#graphql
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

const enableUserErrorsMutation = `#graphql
  mutation LocalizationShopLocaleEnableUserErrors($locale: String!) {
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

const updateUserErrorsMutation = `#graphql
  mutation LocalizationShopLocaleUpdateUserErrors($locale: String!, $shopLocale: ShopLocaleInput!) {
    shopLocaleUpdate(locale: $locale, shopLocale: $shopLocale) {
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

const registerMutation = `#graphql
  mutation LocalizationTranslationsRegister($resourceId: ID!, $translations: [TranslationInput!]!) {
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

const disableUserErrorsMutation = `#graphql
  mutation LocalizationShopLocaleDisableUserErrors($locale: String!) {
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
        updatedAt
        market {
          id
        }
      }
    }
  }
`;

type JsonRecord = Record<string, unknown>;

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphql(query: string, variables: JsonRecord = {}): Promise<JsonRecord> {
  const { status, payload } = await runGraphqlRequest(query, variables);
  if (status < 200 || status >= 300) {
    throw new Error(`Shopify GraphQL request failed with HTTP ${status}: ${JSON.stringify(payload)}`);
  }
  if (!isRecord(payload)) {
    throw new Error(`Shopify GraphQL response was not an object: ${JSON.stringify(payload)}`);
  }
  if ('errors' in payload) {
    throw new Error(`Shopify GraphQL returned top-level errors: ${JSON.stringify(payload['errors'])}`);
  }
  return payload;
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readData(payload: JsonRecord): JsonRecord {
  const data = payload['data'];
  if (!isRecord(data)) {
    throw new Error(`Expected GraphQL payload data object: ${JSON.stringify(payload)}`);
  }
  return data;
}

function readPayloadField(payload: JsonRecord, fieldName: string): JsonRecord {
  const field = readData(payload)[fieldName];
  if (!isRecord(field)) {
    throw new Error(`Expected response data.${fieldName} object: ${JSON.stringify(payload)}`);
  }
  return field;
}

function findProductTitleDigest(readCapturePayload: JsonRecord): { resourceId: string; digest: string } {
  const resources = readData(readCapturePayload)['resources'];
  if (!isRecord(resources) || !Array.isArray(resources['nodes'])) {
    throw new Error('Localization read did not return translatable product resources.');
  }

  for (const node of resources['nodes']) {
    if (!isRecord(node) || typeof node['resourceId'] !== 'string') {
      continue;
    }
    const content = Array.isArray(node['translatableContent']) ? node['translatableContent'] : [];
    for (const item of content) {
      if (
        isRecord(item) &&
        item['key'] === 'title' &&
        typeof item['digest'] === 'string' &&
        item['digest'].length > 0
      ) {
        return {
          resourceId: node['resourceId'],
          digest: item['digest'],
        };
      }
    }
  }

  throw new Error('Could not find a product title digest in the localization read capture.');
}

async function disableFrenchLocaleIfEnabled(): Promise<void> {
  const payload = await runGraphql(disableMutation, { locale: 'fr' });
  const result = readPayloadField(payload, 'shopLocaleDisable');
  const userErrors = Array.isArray(result['userErrors']) ? result['userErrors'] : [];
  const onlyInvalidLocale =
    userErrors.length === 1 &&
    isRecord(userErrors[0]) &&
    (userErrors[0]['message'] === 'Locale not found' || userErrors[0]['message'] === "The locale doesn't exist.");
  if (userErrors.length > 0 && !onlyInvalidLocale) {
    throw new Error(`Pre-capture locale cleanup failed: ${JSON.stringify(result)}`);
  }
}

const readVariables = {
  first: 3,
  resourceType: 'PRODUCT',
  ids: ['gid://shopify/Product/999999999999999'],
};

await disableFrenchLocaleIfEnabled();

const readCapture = await runGraphql(readQuery, readVariables);
const primaryGuardVariables = {
  locale: 'en',
};
const primaryUnpublishVariables = {
  locale: 'en',
  shopLocale: { published: false },
};
const missingLocaleUpdateVariables = {
  locale: 'zz',
  shopLocale: { published: false },
};
const primaryGuards = {
  primaryLocale: 'en',
  enablePrimary: {
    request: { variables: primaryGuardVariables },
    response: await runGraphql(enableUserErrorsMutation, primaryGuardVariables),
  },
  updatePrimaryUnpublish: {
    request: { variables: primaryUnpublishVariables },
    response: await runGraphql(updateUserErrorsMutation, primaryUnpublishVariables),
  },
  disablePrimary: {
    request: { variables: primaryGuardVariables },
    response: await runGraphql(disableUserErrorsMutation, primaryGuardVariables),
  },
  updateMissingLocale: {
    request: { variables: missingLocaleUpdateVariables },
    response: await runGraphql(updateUserErrorsMutation, missingLocaleUpdateVariables),
  },
};
const { resourceId, digest } = findProductTitleDigest(readCapture);
const translationValue = `Titre HAR-449 disable cleanup ${Date.now()}`;
const registerVariables = {
  resourceId,
  translations: [
    {
      locale: 'fr',
      key: 'title',
      value: translationValue,
      translatableContentDigest: digest,
    },
  ],
};

let disablePayload: JsonRecord | null = null;
try {
  const enablePayload = await runGraphql(enableMutation, { locale: 'fr' });
  const registerPayload = await runGraphql(registerMutation, registerVariables);
  const downstreamRegisteredPayload = await runGraphql(translationsReadQuery, { resourceId });
  disablePayload = await runGraphql(disableMutation, { locale: 'fr' });
  const downstreamAfterDisablePayload = await runGraphql(translationsReadQuery, { resourceId });

  const capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    rootAvailability: {
      queries: [
        'availableLocales',
        'shopLocales',
        'translatableResource',
        'translatableResources',
        'translatableResourcesByIds',
      ],
      mutations: ['shopLocaleDisable', 'shopLocaleEnable', 'translationsRegister'],
    },
    readCapture: {
      request: { variables: readVariables },
      response: readCapture,
    },
    disableCleanupLifecycle: {
      resourceId,
      locale: 'fr',
      titleDigest: digest,
      translationValue,
      registerRequest: { variables: registerVariables },
      enable: readPayloadField(enablePayload, 'shopLocaleEnable'),
      register: readPayloadField(registerPayload, 'translationsRegister'),
      downstreamRegistered: readPayloadField(downstreamRegisteredPayload, 'translatableResource'),
      disable: readPayloadField(disablePayload, 'shopLocaleDisable'),
      downstreamAfterDisable: readPayloadField(downstreamAfterDisablePayload, 'translatableResource'),
    },
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(disableCleanupOutputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  await writeFile(
    primaryGuardsOutputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        rootAvailability: {
          mutations: ['shopLocaleDisable', 'shopLocaleEnable', 'shopLocaleUpdate'],
        },
        primaryGuards,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );
  // oxlint-disable-next-line no-console -- CLI capture output is intentionally written to stdout.
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPaths: [disableCleanupOutputPath, primaryGuardsOutputPath],
        storeDomain,
        apiVersion,
        resourceId,
      },
      null,
      2,
    ),
  );
} finally {
  if (disablePayload === null) {
    await disableFrenchLocaleIfEnabled();
  }
}
