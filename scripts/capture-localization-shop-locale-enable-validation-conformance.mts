/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-shop-locale-enable-validation';

const shopLocalesQuery = `#graphql
  query LocalizationShopLocaleEnableValidationShopLocales {
    shopLocales {
      locale
      name
      primary
      published
      marketWebPresences {
        id
      }
    }
  }
`;

const availableLocalesQuery = `#graphql
  query LocalizationShopLocaleEnableValidationAvailableLocales {
    availableLocales {
      isoCode
      name
    }
  }
`;

const webPresencesQuery = `#graphql
  query LocalizationShopLocaleEnableValidationWebPresences($first: Int!) {
    webPresences(first: $first) {
      nodes {
        id
      }
    }
  }
`;

const shopLocaleEnableMutation = `#graphql
  mutation LocalizationShopLocaleEnableValidationEnable($locale: String!) {
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

const shopLocaleUpdateMutation = `#graphql
  mutation LocalizationShopLocaleEnableValidationUpdate($locale: String!, $shopLocale: ShopLocaleInput!) {
    shopLocaleUpdate(locale: $locale, shopLocale: $shopLocale) {
      shopLocale {
        locale
        name
        primary
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

const shopLocaleDisableMutation = `#graphql
  mutation LocalizationShopLocaleEnableValidationDisable($locale: String!) {
    shopLocaleDisable(locale: $locale) {
      locale
      userErrors {
        field
        message
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

function boolField(value: unknown, fieldName: string, context: string): boolean {
  if (!isRecord(value) || typeof value[fieldName] !== 'boolean') {
    throw new Error(`Expected ${context}.${fieldName} boolean: ${JSON.stringify(value)}`);
  }
  return value[fieldName];
}

function shopLocales(payload: JsonRecord): JsonRecord[] {
  return arrayField(dataObject(payload)['shopLocales'], 'shopLocales').filter(isRecord);
}

function availableLocaleCodes(payload: JsonRecord): string[] {
  return arrayField(dataObject(payload)['availableLocales'], 'availableLocales')
    .filter(isRecord)
    .map((locale) => stringField(locale, 'isoCode', 'availableLocale'));
}

function marketWebPresenceIds(locale: JsonRecord): string[] {
  return arrayField(locale['marketWebPresences'], 'marketWebPresences')
    .filter(isRecord)
    .map((presence) => stringField(presence, 'id', 'marketWebPresence'));
}

function firstWebPresenceId(payload: JsonRecord): string {
  const webPresences = dataObject(payload)['webPresences'];
  if (!isRecord(webPresences)) {
    throw new Error(`Expected webPresences object: ${JSON.stringify(payload)}`);
  }
  const first = arrayField(webPresences['nodes'], 'webPresences.nodes').find(isRecord);
  if (!first) {
    throw new Error(`Expected at least one MarketWebPresence: ${JSON.stringify(payload)}`);
  }
  return stringField(first, 'id', 'webPresence');
}

function userErrors(payload: JsonRecord, root: string): unknown[] {
  const field = dataObject(payload)[root];
  if (!isRecord(field)) {
    throw new Error(`Expected data.${root} object: ${JSON.stringify(payload)}`);
  }
  return arrayField(field['userErrors'], `${root}.userErrors`);
}

function assertNoUserErrors(payload: JsonRecord, root: string, context: string): void {
  const errors = userErrors(payload, root);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

async function captureStep(query: string, variables: JsonRecord): Promise<JsonRecord> {
  return {
    request: { variables },
    response: (await runGraphql(query, variables)) as JsonRecord,
  };
}

async function disableLocale(locale: string): Promise<JsonRecord> {
  return (await runGraphql(shopLocaleDisableMutation, { locale })) as JsonRecord;
}

async function restoreLocale(locale: JsonRecord): Promise<JsonRecord> {
  const code = stringField(locale, 'locale', 'shopLocale');
  const enable = await runGraphql(shopLocaleEnableMutation, { locale: code });
  const update = await runGraphql(shopLocaleUpdateMutation, {
    locale: code,
    shopLocale: {
      published: boolField(locale, 'published', 'shopLocale'),
      marketWebPresenceIds: marketWebPresenceIds(locale),
    },
  });
  return { enable, update };
}

const cleanup: JsonRecord = {};
const enabledByCapture = new Set<string>();
let initialAlternateLocalesForRestore: JsonRecord[] = [];
let capture: JsonRecord | null = null;

try {
  const initialShopLocales = (await runGraphql(shopLocalesQuery)) as JsonRecord;
  const initialLocales = shopLocales(initialShopLocales);
  const primaryLocale = initialLocales.find((locale) => locale['primary'] === true);
  if (!primaryLocale) {
    throw new Error(`Could not find primary shop locale: ${JSON.stringify(initialLocales)}`);
  }
  const primaryLocaleCode = stringField(primaryLocale, 'locale', 'primary shop locale');
  const initialAlternateLocales = initialLocales.filter((locale) => locale['primary'] !== true);
  initialAlternateLocalesForRestore = initialAlternateLocales;

  cleanup['preCaptureDisable'] = [];
  for (const locale of initialAlternateLocales) {
    const code = stringField(locale, 'locale', 'shopLocale');
    (cleanup['preCaptureDisable'] as unknown[]).push({ locale: code, response: await disableLocale(code) });
  }

  const availableLocales = (await runGraphql(availableLocalesQuery)) as JsonRecord;
  const webPresences = (await runGraphql(webPresencesQuery, { first: 5 })) as JsonRecord;
  const capturedWebPresenceId = firstWebPresenceId(webPresences);
  const availableCodes = availableLocaleCodes(availableLocales).filter(
    (code) => code !== primaryLocaleCode && code !== 'fr' && code !== 'tr',
  );
  if (availableCodes.length < 20) {
    throw new Error(`Expected at least 20 alternate locale candidates: ${JSON.stringify(availableCodes)}`);
  }

  const unsupportedLocale = await captureStep(shopLocaleEnableMutation, { locale: 'tlh' });
  const firstEnable = await captureStep(shopLocaleEnableMutation, { locale: 'fr' });
  assertNoUserErrors(firstEnable['response'] as JsonRecord, 'shopLocaleEnable', 'first French enable');
  enabledByCapture.add('fr');
  const duplicateEnable = await captureStep(shopLocaleEnableMutation, { locale: 'fr' });

  const maxSetupLocales = availableCodes.slice(0, 19);
  const maxSetup: JsonRecord[] = [];
  for (const locale of maxSetupLocales) {
    const step = await captureStep(shopLocaleEnableMutation, { locale });
    assertNoUserErrors(step['response'] as JsonRecord, 'shopLocaleEnable', `max setup enable ${locale}`);
    maxSetup.push(step);
    enabledByCapture.add(locale);
  }
  const limitReached = await captureStep(shopLocaleEnableMutation, { locale: availableCodes[19] });

  const partialUpdate = await captureStep(shopLocaleUpdateMutation, {
    locale: 'tr',
    shopLocale: {
      marketWebPresenceIds: ['gid://shopify/MarketWebPresence/1'],
    },
  });
  enabledByCapture.add('tr');

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    initialShopLocales,
    availableLocales,
    webPresences,
    capturedWebPresenceId,
    unsupportedLocale,
    duplicateLocale: {
      firstEnable,
      duplicateEnable,
    },
    maximumLocales: {
      setup: maxSetup,
      limitReached,
    },
    partialUpdate,
    cleanup,
    upstreamCalls: [],
  };
} finally {
  cleanup['disableCapturedLocales'] = [];
  for (const locale of Array.from(enabledByCapture).reverse()) {
    try {
      (cleanup['disableCapturedLocales'] as unknown[]).push({ locale, response: await disableLocale(locale) });
    } catch (error) {
      (cleanup['disableCapturedLocales'] as unknown[]).push({ locale, error: String(error) });
    }
  }

  cleanup['restoreInitialLocales'] = [];
  for (const locale of initialAlternateLocalesForRestore) {
    try {
      (cleanup['restoreInitialLocales'] as unknown[]).push({
        locale: stringField(locale, 'locale', 'shopLocale'),
        response: await restoreLocale(locale),
      });
    } catch (error) {
      (cleanup['restoreInitialLocales'] as unknown[]).push({
        locale: stringField(locale, 'locale', 'shopLocale'),
        error: String(error),
      });
    }
  }
  if (capture) {
    capture['cleanup'] = cleanup;
    await mkdir(outputDir, { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${outputPath}`);
  }
}
