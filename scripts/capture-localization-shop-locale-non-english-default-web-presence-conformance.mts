/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type LocaleSnapshot = {
  locale: string;
  primary: boolean;
  published: boolean;
  marketWebPresenceIds: string[];
};

const scenarioId = 'localization-shop-locale-non-english-default-web-presence';
const defaultLocale = 'it';
const associatedLocale = 'fr';
const webPresencesFirst = 100;

const setupQueryPath =
  'config/parity-requests/localization/localization-shop-locale-non-english-default-web-presence-setup.graphql';
const enableQueryPath =
  'config/parity-requests/localization/localization-shop-locale-non-english-default-web-presence-enable.graphql';
const readQueryPath =
  'config/parity-requests/localization/localization-shop-locale-non-english-default-web-presence-read.graphql';
const createQueryPath = 'config/parity-requests/markets/web-presence-lifecycle-create.graphql';
const deleteQueryPath = 'config/parity-requests/markets/web-presence-delete.graphql';
const disableQueryPath = 'config/parity-requests/localization/localization-shop-locale-disable.graphql';

const shopLocalesSnapshotQuery = `#graphql
  query LocalizationShopLocaleNonEnglishDefaultSnapshot {
    shopLocales {
      locale
      primary
      published
      marketWebPresences {
        id
      }
    }
  }
`;

const restoreLocaleUpdateMutation = `#graphql
  mutation LocalizationShopLocaleNonEnglishDefaultRestore($locale: String!, $shopLocale: ShopLocaleInput!) {
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
const outputPath = path.join(outputDir, `${scenarioId}.json`);

const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function readText(filePath: string): Promise<string> {
  return await readFile(filePath, 'utf8');
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function dataObject(payload: unknown): JsonRecord {
  if (!isRecord(payload) || !isRecord(payload['data'])) {
    throw new Error(`Expected GraphQL data object: ${JSON.stringify(payload)}`);
  }
  return payload['data'];
}

function payloadField(payload: unknown, fieldName: string): JsonRecord {
  const field = dataObject(payload)[fieldName];
  if (!isRecord(field)) {
    throw new Error(`Expected data.${fieldName} object: ${JSON.stringify(payload)}`);
  }
  return field;
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

function readUserErrors(field: JsonRecord): unknown[] {
  return arrayField(field['userErrors'], 'userErrors');
}

function assertNoUserErrors(payload: unknown, fieldName: string, context: string): void {
  const errors = readUserErrors(payloadField(payload, fieldName));
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function localeSnapshots(payload: unknown): LocaleSnapshot[] {
  const locales = arrayField(dataObject(payload)['shopLocales'], 'shopLocales').filter(isRecord);
  return locales.map((shopLocale) => ({
    locale: stringField(shopLocale, 'locale', 'shopLocale'),
    primary: boolField(shopLocale, 'primary', 'shopLocale'),
    published: boolField(shopLocale, 'published', 'shopLocale'),
    marketWebPresenceIds: arrayField(shopLocale['marketWebPresences'], 'shopLocale.marketWebPresences')
      .filter(isRecord)
      .map((presence) => stringField(presence, 'id', 'MarketWebPresence')),
  }));
}

async function readLocaleSnapshot(locale: string): Promise<LocaleSnapshot | null> {
  const payload = (await runGraphql(shopLocalesSnapshotQuery, {})) as JsonRecord;
  return localeSnapshots(payload).find((snapshot) => snapshot.locale === locale) ?? null;
}

function localeAbsentUserErrors(payload: unknown, fieldName: string): boolean {
  const errors = readUserErrors(payloadField(payload, fieldName));
  return errors.length === 1 && isRecord(errors[0]) && errors[0]['message'] === "The locale doesn't exist.";
}

async function ensureLocaleAbsent(disableQuery: string, locale: string): Promise<JsonRecord> {
  const response = (await runGraphql(disableQuery, { locale })) as JsonRecord;
  const errors = readUserErrors(payloadField(response, 'shopLocaleDisable'));
  if (errors.length > 0 && !localeAbsentUserErrors(response, 'shopLocaleDisable')) {
    throw new Error(`shopLocaleDisable(${locale}) failed: ${JSON.stringify(errors)}`);
  }
  return response;
}

async function restoreDisabledLocale(
  enableQuery: string,
  updateQuery: string,
  snapshot: LocaleSnapshot | null,
): Promise<JsonRecord | null> {
  if (snapshot === null) {
    return null;
  }
  const enableResponse = (await runGraphql(enableQuery, {
    locale: snapshot.locale,
    marketWebPresenceIds: snapshot.marketWebPresenceIds,
  })) as JsonRecord;
  assertNoUserErrors(enableResponse, 'shopLocaleEnable', `restore shopLocaleEnable ${snapshot.locale}`);

  const updateResponse = (await runGraphql(updateQuery, {
    locale: snapshot.locale,
    shopLocale: {
      published: snapshot.published,
      marketWebPresenceIds: snapshot.marketWebPresenceIds,
    },
  })) as JsonRecord;
  assertNoUserErrors(updateResponse, 'shopLocaleUpdate', `restore shopLocaleUpdate ${snapshot.locale}`);
  return updateResponse;
}

async function restoreExistingLocale(
  updateQuery: string,
  snapshot: LocaleSnapshot | null,
): Promise<JsonRecord | null> {
  if (snapshot === null || snapshot.primary) {
    return null;
  }
  const updateResponse = (await runGraphql(updateQuery, {
    locale: snapshot.locale,
    shopLocale: {
      published: snapshot.published,
      marketWebPresenceIds: snapshot.marketWebPresenceIds,
    },
  })) as JsonRecord;
  assertNoUserErrors(updateResponse, 'shopLocaleUpdate', `restore shopLocaleUpdate ${snapshot.locale}`);
  return updateResponse;
}

async function createWebPresence(
  createQuery: string,
  suffix: string,
): Promise<{ id: string; variables: JsonRecord; response: JsonRecord }> {
  const variables = {
    input: {
      defaultLocale,
      alternateLocales: [],
      subfolderSuffix: suffix,
    },
  };
  const response = (await runGraphql(createQuery, variables)) as JsonRecord;
  assertNoUserErrors(response, 'webPresenceCreate', `webPresenceCreate ${suffix}`);
  const webPresence = payloadField(response, 'webPresenceCreate')['webPresence'];
  return {
    id: stringField(webPresence, 'id', 'webPresence'),
    variables,
    response,
  };
}

async function deleteWebPresence(deleteQuery: string, id: string): Promise<JsonRecord> {
  const response = (await runGraphql(deleteQuery, { id })) as JsonRecord;
  assertNoUserErrors(response, 'webPresenceDelete', `webPresenceDelete ${id}`);
  return response;
}

function randomLetters(length: number): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz';
  return Array.from({ length }, () => alphabet[Math.floor(Math.random() * alphabet.length)] ?? 'a').join('');
}

function shopLocaleFromEnable(payload: unknown): JsonRecord {
  const shopLocale = payloadField(payload, 'shopLocaleEnable')['shopLocale'];
  if (!isRecord(shopLocale)) {
    throw new Error(`Expected shopLocaleEnable.shopLocale object: ${JSON.stringify(payload)}`);
  }
  return shopLocale;
}

function shopLocaleFromRead(payload: unknown, locale: string): JsonRecord {
  const locales = arrayField(dataObject(payload)['shopLocales'], 'shopLocales').filter(isRecord);
  const shopLocale = locales.find((entry) => entry['locale'] === locale);
  if (!shopLocale) {
    throw new Error(`Expected shopLocales entry ${locale}: ${JSON.stringify(payload)}`);
  }
  return shopLocale;
}

function marketWebPresenceFromLocale(shopLocale: JsonRecord, id: string): JsonRecord {
  const presences = arrayField(shopLocale['marketWebPresences'], 'shopLocale.marketWebPresences').filter(isRecord);
  const presence = presences.find((entry) => entry['id'] === id);
  if (!presence) {
    throw new Error(`Expected MarketWebPresence ${id}: ${JSON.stringify(shopLocale)}`);
  }
  return presence;
}

function assertDefaultLocaleRecord(shopLocale: JsonRecord, id: string, context: string): void {
  const presence = marketWebPresenceFromLocale(shopLocale, id);
  const localeRecord = presence['defaultLocale'];
  if (!isRecord(localeRecord)) {
    throw new Error(`${context} expected defaultLocale object: ${JSON.stringify(presence)}`);
  }
  const actual = {
    locale: localeRecord['locale'],
    primary: localeRecord['primary'],
    published: localeRecord['published'],
  };
  const expected = { locale: defaultLocale, primary: true, published: true };
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${context} defaultLocale mismatch: ${JSON.stringify({ actual, expected, presence })}`);
  }
}

function upstreamReadCall(query: string, variables: JsonRecord, response: JsonRecord): JsonRecord {
  return {
    operationName: 'LocalizationShopLocaleNonEnglishDefaultWebPresenceSetup',
    variables,
    query,
    response: {
      status: 200,
      body: response,
    },
  };
}

const setupQuery = await readText(setupQueryPath);
const enableQuery = await readText(enableQueryPath);
const readQuery = await readText(readQueryPath);
const createQuery = await readText(createQueryPath);
const deleteQuery = await readText(deleteQueryPath);
const disableQuery = await readText(disableQueryPath);

const initialAssociatedLocale = await readLocaleSnapshot(associatedLocale);
const initialDefaultLocale = await readLocaleSnapshot(defaultLocale);
const cleanup: JsonRecord = {};
let createdWebPresenceId: string | null = null;
let capture: JsonRecord | null = null;

try {
  cleanup['preCaptureAssociatedLocaleDisable'] = await ensureLocaleAbsent(disableQuery, associatedLocale);

  const setupResponses: JsonRecord = {};
  if (initialDefaultLocale === null) {
    const defaultLocaleEnable = (await runGraphql(enableQuery, {
      locale: defaultLocale,
      marketWebPresenceIds: [],
    })) as JsonRecord;
    assertNoUserErrors(defaultLocaleEnable, 'shopLocaleEnable', `setup shopLocaleEnable ${defaultLocale}`);
    setupResponses['defaultLocaleEnable'] = defaultLocaleEnable;
  }

  const create = await createWebPresence(createQuery, `loc${randomLetters(10)}`);
  createdWebPresenceId = create.id;

  const setupVariables = { first: webPresencesFirst };
  const setupRead = (await runGraphql(setupQuery, setupVariables)) as JsonRecord;
  assertDefaultLocaleRecord(shopLocaleFromRead(setupRead, defaultLocale), create.id, 'setup shopLocales');

  const enableVariables = {
    locale: associatedLocale,
    marketWebPresenceIds: [create.id],
  };
  const enable = (await runGraphql(enableQuery, enableVariables)) as JsonRecord;
  assertNoUserErrors(enable, 'shopLocaleEnable', 'shopLocaleEnable associated locale');
  assertDefaultLocaleRecord(shopLocaleFromEnable(enable), create.id, 'shopLocaleEnable payload');

  const readAfterEnable = (await runGraphql(readQuery, {})) as JsonRecord;
  assertDefaultLocaleRecord(shopLocaleFromRead(readAfterEnable, associatedLocale), create.id, 'read after enable');

  cleanup['associatedLocaleDisable'] = await ensureLocaleAbsent(disableQuery, associatedLocale);
  cleanup['webPresenceDelete'] = await deleteWebPresence(deleteQuery, create.id);
  createdWebPresenceId = null;

  if (initialDefaultLocale === null) {
    cleanup['defaultLocaleDisable'] = await ensureLocaleAbsent(disableQuery, defaultLocale);
  } else {
    const restoreDefaultLocale = await restoreExistingLocale(restoreLocaleUpdateMutation, initialDefaultLocale);
    if (restoreDefaultLocale !== null) {
      cleanup['defaultLocaleRestore'] = restoreDefaultLocale;
    }
  }

  const restoreAssociatedLocale = await restoreDisabledLocale(enableQuery, restoreLocaleUpdateMutation, initialAssociatedLocale);
  if (restoreAssociatedLocale !== null) {
    cleanup['associatedLocaleRestore'] = restoreAssociatedLocale;
  }

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId,
    defaultLocale,
    associatedLocale,
    createdWebPresenceIds: [create.id],
    initial: {
      associatedLocale: initialAssociatedLocale,
      defaultLocale: initialDefaultLocale,
    },
    setupResponses,
    create: {
      request: { variables: create.variables },
      response: create.response,
    },
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
    cleanup,
    upstreamCalls: [upstreamReadCall(setupQuery, setupVariables, setupRead)],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(`wrote ${outputPath}`);
} catch (error) {
  cleanup['error'] = error instanceof Error ? error.message : String(error);
  try {
    cleanup['bestEffortAssociatedLocaleDisable'] = await ensureLocaleAbsent(disableQuery, associatedLocale);
  } catch (cleanupError) {
    cleanup['bestEffortAssociatedLocaleDisableError'] =
      cleanupError instanceof Error ? cleanupError.message : String(cleanupError);
  }
  try {
    if (createdWebPresenceId !== null) {
      cleanup['bestEffortWebPresenceDelete'] = await deleteWebPresence(deleteQuery, createdWebPresenceId);
    }
  } catch (cleanupError) {
    cleanup['bestEffortWebPresenceDeleteError'] =
      cleanupError instanceof Error ? cleanupError.message : String(cleanupError);
  }
  try {
    if (initialDefaultLocale === null) {
      cleanup['bestEffortDefaultLocaleDisable'] = await ensureLocaleAbsent(disableQuery, defaultLocale);
    } else {
      const restoreDefaultLocale = await restoreExistingLocale(restoreLocaleUpdateMutation, initialDefaultLocale);
      if (restoreDefaultLocale !== null) {
        cleanup['bestEffortDefaultLocaleRestore'] = restoreDefaultLocale;
      }
    }
  } catch (cleanupError) {
    cleanup['bestEffortDefaultLocaleCleanupError'] =
      cleanupError instanceof Error ? cleanupError.message : String(cleanupError);
  }
  try {
    const restoreAssociatedLocale = await restoreDisabledLocale(
      enableQuery,
      restoreLocaleUpdateMutation,
      initialAssociatedLocale,
    );
    if (restoreAssociatedLocale !== null) {
      cleanup['bestEffortAssociatedLocaleRestore'] = restoreAssociatedLocale;
    }
  } catch (cleanupError) {
    cleanup['bestEffortAssociatedLocaleRestoreError'] =
      cleanupError instanceof Error ? cleanupError.message : String(cleanupError);
  }
  console.error(JSON.stringify({ cleanupAfterFailure: cleanup }, null, 2));
  throw error;
} finally {
  if (capture === null) {
    console.error(JSON.stringify({ cleanup }, null, 2));
  }
}
