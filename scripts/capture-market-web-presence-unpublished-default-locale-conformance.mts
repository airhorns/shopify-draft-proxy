/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type ShopLocaleSnapshot = {
  locale: string;
  published: boolean;
  marketWebPresenceIds: string[];
};

const fixtureStoreDomain = 'harry-test-heelo.myshopify.com';
const locale = 'it';
const scenarioId = 'web-presence-create-unpublished-default-locale';
const createQueryPath = 'config/parity-requests/markets/web-presence-lifecycle-create.graphql';
const deleteQueryPath = 'config/parity-requests/markets/web-presence-delete.graphql';

const config = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
  exitOnMissing: true,
});

if (config.storeDomain !== fixtureStoreDomain) {
  throw new Error(
    `This recorder writes checked-in ${fixtureStoreDomain} fixtures; got SHOPIFY_CONFORMANCE_STORE_DOMAIN=${config.storeDomain}.`,
  );
}

const adminAccessToken = await getValidConformanceAccessToken({
  adminOrigin: config.adminOrigin,
  apiVersion: config.apiVersion,
});

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin: config.adminOrigin,
  apiVersion: config.apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const shopLocalesQuery = `#graphql
  query MarketWebPresenceUnpublishedDefaultLocaleShopLocales {
    shopLocales {
      locale
      published
      marketWebPresences {
        id
      }
    }
  }
`;

const enableLocaleMutation = `#graphql
  mutation MarketWebPresenceUnpublishedDefaultLocaleEnable($locale: String!) {
    shopLocaleEnable(locale: $locale) {
      shopLocale {
        locale
        published
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const updateLocaleMutation = `#graphql
  mutation MarketWebPresenceUnpublishedDefaultLocaleUpdate($locale: String!, $shopLocale: ShopLocaleInput!) {
    shopLocaleUpdate(locale: $locale, shopLocale: $shopLocale) {
      shopLocale {
        locale
        published
        marketWebPresences {
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

const disableLocaleMutation = `#graphql
  mutation MarketWebPresenceUnpublishedDefaultLocaleDisable($locale: String!) {
    shopLocaleDisable(locale: $locale) {
      locale
      userErrors {
        field
        message
      }
    }
  }
`;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function dataObject(result: ConformanceGraphqlResult): JsonRecord {
  const data = result.payload.data;
  if (!isRecord(data)) {
    throw new Error(`Expected GraphQL data object: ${JSON.stringify(result, null, 2)}`);
  }
  return data;
}

function rootPayload(result: ConformanceGraphqlResult, root: string): JsonRecord {
  const value = dataObject(result)[root];
  if (!isRecord(value)) {
    throw new Error(`Expected data.${root} object: ${JSON.stringify(result, null, 2)}`);
  }
  return value;
}

function userErrors(result: ConformanceGraphqlResult, root: string): JsonRecord[] {
  const errors = rootPayload(result, root)['userErrors'];
  return Array.isArray(errors) ? errors.filter(isRecord) : [];
}

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(label: string, result: ConformanceGraphqlResult, root: string): void {
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertUnpublishedLanguageError(result: ConformanceGraphqlResult): void {
  assertGraphqlOk('webPresenceCreate unpublished default locale', result);
  const payload = rootPayload(result, 'webPresenceCreate');
  const errors = userErrors(result, 'webPresenceCreate');
  if (payload['webPresence'] !== null) {
    throw new Error(`Expected null webPresence: ${JSON.stringify(result, null, 2)}`);
  }
  const firstError = errors[0];
  if (
    !firstError ||
    JSON.stringify(firstError['field']) !== JSON.stringify(['input', 'defaultLocale']) ||
    firstError['message'] !== "Default locale The default language isn't published to the store: Italian" ||
    firstError['code'] !== 'UNPUBLISHED_LANGUAGE'
  ) {
    throw new Error(`Expected UNPUBLISHED_LANGUAGE default-locale error: ${JSON.stringify(errors)}`);
  }
}

function shopLocaleSnapshots(result: ConformanceGraphqlResult): ShopLocaleSnapshot[] {
  const locales = dataObject(result)['shopLocales'];
  if (!Array.isArray(locales)) {
    throw new Error(`Expected shopLocales array: ${JSON.stringify(result, null, 2)}`);
  }
  return locales.filter(isRecord).map((entry) => {
    const marketWebPresences = entry['marketWebPresences'];
    return {
      locale: String(entry['locale'] ?? ''),
      published: entry['published'] === true,
      marketWebPresenceIds: Array.isArray(marketWebPresences)
        ? marketWebPresences.filter(isRecord).map((presence) => String(presence['id'] ?? ''))
        : [],
    };
  });
}

function randomLetters(length: number): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz';
  return Array.from({ length }, () => alphabet[Math.floor(Math.random() * alphabet.length)] ?? 'a').join('');
}

function webPresenceId(result: ConformanceGraphqlResult): string | null {
  const value = rootPayload(result, 'webPresenceCreate')['webPresence'];
  if (!isRecord(value) || typeof value['id'] !== 'string') {
    return null;
  }
  return value['id'];
}

async function readDocument(relativePath: string): Promise<string> {
  return await readFile(relativePath, 'utf8');
}

async function writeFixture(capture: JsonRecord): Promise<string> {
  const outputDir = path.join('fixtures', 'conformance', fixtureStoreDomain, config.apiVersion, 'markets');
  const outputPath = path.join(outputDir, 'market-web-presence-unpublished-default-locale.json');
  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  return outputPath;
}

const createQuery = await readDocument(createQueryPath);
const deleteQuery = await readDocument(deleteQueryPath);
const baselineShopLocales = await runGraphqlRequest(shopLocalesQuery);
assertGraphqlOk('baseline shopLocales', baselineShopLocales);
const originalItalianLocale = shopLocaleSnapshots(baselineShopLocales).find((entry) => entry.locale === locale) ?? null;
let setupResponse: ConformanceGraphqlResult | null = null;
let forceUnpublishedResponse: ConformanceGraphqlResult | null = null;
let enabledItalianForCapture = false;
let unpublishedItalianForCapture = false;
let createVariables: JsonRecord | null = null;
let createResponse: ConformanceGraphqlResult | null = null;
let createdId: string | null = null;
let unexpectedWebPresenceCleanupResponse: ConformanceGraphqlResult | null = null;
let localeCleanupResponse: ConformanceGraphqlResult | null = null;
let finalShopLocales: ConformanceGraphqlResult | null = null;

try {
  if (!originalItalianLocale) {
    setupResponse = await runGraphqlRequest(enableLocaleMutation, { locale });
    assertGraphqlOk('shopLocaleEnable Italian', setupResponse);
    assertNoUserErrors('shopLocaleEnable Italian', setupResponse, 'shopLocaleEnable');
    enabledItalianForCapture = true;
  } else if (originalItalianLocale.published) {
    forceUnpublishedResponse = await runGraphqlRequest(updateLocaleMutation, {
      locale,
      shopLocale: {
        published: false,
        marketWebPresenceIds: originalItalianLocale.marketWebPresenceIds,
      },
    });
    assertGraphqlOk('shopLocaleUpdate Italian unpublished setup', forceUnpublishedResponse);
    assertNoUserErrors('shopLocaleUpdate Italian unpublished setup', forceUnpublishedResponse, 'shopLocaleUpdate');
    unpublishedItalianForCapture = true;
  }

  createVariables = {
    input: {
      defaultLocale: locale,
      alternateLocales: [],
      subfolderSuffix: `har${randomLetters(10)}`,
    },
  };
  createResponse = await runGraphqlRequest(createQuery, createVariables);
  createdId = webPresenceId(createResponse);
  assertUnpublishedLanguageError(createResponse);
} finally {
  if (createdId) {
    unexpectedWebPresenceCleanupResponse = await runGraphqlRequest(deleteQuery, { id: createdId });
  }

  if (enabledItalianForCapture) {
    localeCleanupResponse = await runGraphqlRequest(disableLocaleMutation, { locale });
    assertGraphqlOk('shopLocaleDisable Italian cleanup', localeCleanupResponse);
    assertNoUserErrors('shopLocaleDisable Italian cleanup', localeCleanupResponse, 'shopLocaleDisable');
  } else if (unpublishedItalianForCapture && originalItalianLocale) {
    localeCleanupResponse = await runGraphqlRequest(updateLocaleMutation, {
      locale,
      shopLocale: {
        published: originalItalianLocale.published,
        marketWebPresenceIds: originalItalianLocale.marketWebPresenceIds,
      },
    });
    assertGraphqlOk('shopLocaleUpdate Italian cleanup', localeCleanupResponse);
    assertNoUserErrors('shopLocaleUpdate Italian cleanup', localeCleanupResponse, 'shopLocaleUpdate');
  }

  finalShopLocales = await runGraphqlRequest(shopLocalesQuery);
}

if (!createResponse) {
  throw new Error('Missing webPresenceCreate response.');
}
if (!createVariables) {
  throw new Error('Missing webPresenceCreate variables.');
}

const outputPath = await writeFixture({
  capturedAt: new Date().toISOString(),
  storeDomain: config.storeDomain,
  apiVersion: config.apiVersion,
  scenarioId,
  setup: {
    baselineShopLocales,
    originalItalianLocale,
    enableItalianUnpublished: setupResponse,
    forceItalianUnpublished: forceUnpublishedResponse,
  },
  cases: {
    unpublishedItalianDefault: {
      name: 'webPresenceCreateUnpublishedItalianDefaultLocale',
      query: createQuery,
      variables: createVariables,
      response: createResponse,
    },
  },
  cleanup: {
    unexpectedWebPresenceCleanupResponse,
    localeCleanupResponse,
    finalShopLocales,
  },
  upstreamCalls: [],
});

console.log(`Wrote ${outputPath}`);
