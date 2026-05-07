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
  published: boolean;
  marketWebPresenceIds: string[];
};

const locale = 'fr';
const webPresencesFirst = 100;
const readQueryPath = 'config/parity-requests/localization/localization-shop-locale-web-presence-sync-read.graphql';
const enableQueryPath = 'config/parity-requests/localization/localization-shop-locale-web-presence-sync-enable.graphql';
const updateQueryPath = 'config/parity-requests/localization/localization-shop-locale-web-presence-sync-update.graphql';
const disableQueryPath = 'config/parity-requests/localization/localization-shop-locale-disable.graphql';
const createQueryPath = 'config/parity-requests/markets/web-presence-lifecycle-create.graphql';
const deleteQueryPath = 'config/parity-requests/markets/web-presence-delete.graphql';

const shopLocalesQuery = `#graphql
  query LocalizationShopLocaleWebPresenceSyncLocaleSnapshot {
    shopLocales {
      locale
      published
      marketWebPresences {
        id
      }
    }
  }
`;

const restoreLocaleUpdateMutation = `#graphql
  mutation LocalizationShopLocaleWebPresenceSyncRestoreLocale($locale: String!, $shopLocale: ShopLocaleInput!) {
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
    published: boolField(shopLocale, 'published', 'shopLocale'),
    marketWebPresenceIds: arrayField(shopLocale['marketWebPresences'], 'shopLocale.marketWebPresences')
      .filter(isRecord)
      .map((presence) => stringField(presence, 'id', 'MarketWebPresence')),
  }));
}

function webPresenceNodes(payload: unknown): JsonRecord[] {
  const webPresences = dataObject(payload)['webPresences'];
  if (!isRecord(webPresences)) {
    throw new Error(`Expected data.webPresences object: ${JSON.stringify(payload)}`);
  }
  return arrayField(webPresences['nodes'], 'webPresences.nodes').filter(isRecord);
}

function webPresenceNode(payload: unknown, id: string): JsonRecord {
  const found = webPresenceNodes(payload).find((node) => node['id'] === id);
  if (!found) {
    throw new Error(`Could not find WebPresence ${id}: ${JSON.stringify(payload)}`);
  }
  return found;
}

function localeCodes(value: unknown, context: string): string[] {
  return arrayField(value, context)
    .filter(isRecord)
    .map((entry) => stringField(entry, 'locale', context));
}

function assertWebPresenceHasLocale(payload: unknown, id: string, expectedLocale: string, context: string): void {
  const node = webPresenceNode(payload, id);
  const alternateLocales = localeCodes(node['alternateLocales'], `${context}.alternateLocales`);
  const rootUrlLocales = localeCodes(node['rootUrls'], `${context}.rootUrls`);
  if (!alternateLocales.includes(expectedLocale)) {
    throw new Error(`${context} missing alternate locale ${expectedLocale}: ${JSON.stringify(node)}`);
  }
  if (!rootUrlLocales.includes(expectedLocale)) {
    throw new Error(`${context} missing rootUrl locale ${expectedLocale}: ${JSON.stringify(node)}`);
  }
}

function assertWebPresenceLacksLocale(payload: unknown, id: string, unexpectedLocale: string, context: string): void {
  const node = webPresenceNode(payload, id);
  const alternateLocales = localeCodes(node['alternateLocales'], `${context}.alternateLocales`);
  if (alternateLocales.includes(unexpectedLocale)) {
    throw new Error(`${context} still has alternate locale ${unexpectedLocale}: ${JSON.stringify(node)}`);
  }
}

function randomLetters(length: number): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz';
  return Array.from({ length }, () => alphabet[Math.floor(Math.random() * alphabet.length)] ?? 'a').join('');
}

async function createWebPresence(createQuery: string, suffix: string): Promise<{ id: string; response: JsonRecord }> {
  const variables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: suffix,
    },
  };
  const response = (await runGraphql(createQuery, variables)) as JsonRecord;
  assertNoUserErrors(response, 'webPresenceCreate', `webPresenceCreate ${suffix}`);
  const root = payloadField(response, 'webPresenceCreate');
  const webPresence = root['webPresence'];
  return {
    id: stringField(webPresence, 'id', 'webPresence'),
    response,
  };
}

async function deleteWebPresence(deleteQuery: string, id: string): Promise<JsonRecord> {
  const response = (await runGraphql(deleteQuery, { id })) as JsonRecord;
  assertNoUserErrors(response, 'webPresenceDelete', `webPresenceDelete ${id}`);
  return response;
}

async function disableLocale(disableQuery: string): Promise<JsonRecord> {
  return (await runGraphql(disableQuery, { locale })) as JsonRecord;
}

function localeAbsentUserErrors(payload: unknown): boolean {
  const errors = readUserErrors(payloadField(payload, 'shopLocaleDisable'));
  return errors.length === 1 && isRecord(errors[0]) && errors[0]['message'] === "The locale doesn't exist.";
}

async function ensureLocaleAbsent(disableQuery: string): Promise<JsonRecord> {
  const response = await disableLocale(disableQuery);
  const errors = readUserErrors(payloadField(response, 'shopLocaleDisable'));
  if (errors.length > 0 && !localeAbsentUserErrors(response)) {
    throw new Error(`shopLocaleDisable(${locale}) failed: ${JSON.stringify(errors)}`);
  }
  return response;
}

async function restoreLocaleIfNeeded(
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

function upstreamReadCall(query: string, variables: JsonRecord, response: JsonRecord): JsonRecord {
  return {
    operationName: 'LocalizationShopLocaleWebPresenceSyncRead',
    variables,
    query,
    response: {
      status: 200,
      body: response,
    },
  };
}

const readQuery = await readText(readQueryPath);
const enableQuery = await readText(enableQueryPath);
const updateQuery = await readText(updateQueryPath);
const disableQuery = await readText(disableQueryPath);
const createQuery = await readText(createQueryPath);
const deleteQuery = await readText(deleteQueryPath);

const initialLocales = await runGraphql(shopLocalesQuery, {});
const initialFrenchLocale = localeSnapshots(initialLocales).find((snapshot) => snapshot.locale === locale) ?? null;
const restoreResponses: JsonRecord = {};

let enableWebPresenceId: string | null = null;
let updateSourceWebPresenceId: string | null = null;
let updateTargetWebPresenceId: string | null = null;

try {
  const preCaptureDisable = await ensureLocaleAbsent(disableQuery);
  const enableSuffix = `loc${randomLetters(10)}`;
  const enableCreate = await createWebPresence(createQuery, enableSuffix);
  enableWebPresenceId = enableCreate.id;
  const setupVariables = { first: webPresencesFirst };
  const setupRead = (await runGraphql(readQuery, setupVariables)) as JsonRecord;
  webPresenceNode(setupRead, enableWebPresenceId);

  const enableVariables = {
    locale,
    marketWebPresenceIds: [enableWebPresenceId],
  };
  const enable = (await runGraphql(enableQuery, enableVariables)) as JsonRecord;
  assertNoUserErrors(enable, 'shopLocaleEnable', 'shopLocaleEnable WebPresence sync');

  const readAfterEnable = (await runGraphql(readQuery, setupVariables)) as JsonRecord;
  assertWebPresenceHasLocale(readAfterEnable, enableWebPresenceId, locale, 'shopLocaleEnable read-after-write');

  const cleanupDisable = await disableLocale(disableQuery);
  const cleanupDelete = await deleteWebPresence(deleteQuery, enableWebPresenceId);
  enableWebPresenceId = null;

  const enableCapture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'localization-shop-locale-enable-web-presence-sync',
    createdWebPresenceIds: [enableCreate.id],
    setup: {
      request: { variables: setupVariables },
      response: setupRead,
    },
    enable: {
      request: { variables: enableVariables },
      response: enable,
    },
    readAfterEnable: {
      request: { variables: setupVariables },
      response: readAfterEnable,
    },
    cleanup: {
      preCaptureDisable,
      shopLocaleDisable: cleanupDisable,
      webPresenceDelete: cleanupDelete,
    },
    upstreamCalls: [upstreamReadCall(readQuery, setupVariables, setupRead)],
  };

  await ensureLocaleAbsent(disableQuery);
  const sourceCreate = await createWebPresence(createQuery, `loc${randomLetters(10)}`);
  updateSourceWebPresenceId = sourceCreate.id;
  const targetCreate = await createWebPresence(createQuery, `loc${randomLetters(10)}`);
  updateTargetWebPresenceId = targetCreate.id;
  const updateSetupRead = (await runGraphql(readQuery, setupVariables)) as JsonRecord;
  webPresenceNode(updateSetupRead, updateSourceWebPresenceId);
  webPresenceNode(updateSetupRead, updateTargetWebPresenceId);

  const enableSourceVariables = {
    locale,
    marketWebPresenceIds: [updateSourceWebPresenceId],
  };
  const enableSource = (await runGraphql(enableQuery, enableSourceVariables)) as JsonRecord;
  assertNoUserErrors(enableSource, 'shopLocaleEnable', 'shopLocaleEnable update-source setup');

  const updateSwapVariables = {
    locale,
    shopLocale: {
      marketWebPresenceIds: [updateTargetWebPresenceId],
    },
  };
  const updateSwap = (await runGraphql(updateQuery, updateSwapVariables)) as JsonRecord;
  assertNoUserErrors(updateSwap, 'shopLocaleUpdate', 'shopLocaleUpdate WebPresence sync');

  const readAfterUpdate = (await runGraphql(readQuery, setupVariables)) as JsonRecord;
  assertWebPresenceLacksLocale(
    readAfterUpdate,
    updateSourceWebPresenceId,
    locale,
    'shopLocaleUpdate source read-after-write',
  );
  assertWebPresenceHasLocale(
    readAfterUpdate,
    updateTargetWebPresenceId,
    locale,
    'shopLocaleUpdate target read-after-write',
  );

  const updateCleanupDisable = await disableLocale(disableQuery);
  const sourceCleanupDelete = await deleteWebPresence(deleteQuery, updateSourceWebPresenceId);
  updateSourceWebPresenceId = null;
  const targetCleanupDelete = await deleteWebPresence(deleteQuery, updateTargetWebPresenceId);
  updateTargetWebPresenceId = null;

  const updateCapture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'localization-shop-locale-update-web-presence-sync',
    createdWebPresenceIds: [sourceCreate.id, targetCreate.id],
    setup: {
      request: { variables: setupVariables },
      response: updateSetupRead,
    },
    enableSource: {
      request: { variables: enableSourceVariables },
      response: enableSource,
    },
    updateSwap: {
      request: { variables: updateSwapVariables },
      response: updateSwap,
    },
    readAfterUpdate: {
      request: { variables: setupVariables },
      response: readAfterUpdate,
    },
    cleanup: {
      shopLocaleDisable: updateCleanupDisable,
      sourceWebPresenceDelete: sourceCleanupDelete,
      targetWebPresenceDelete: targetCleanupDelete,
    },
    upstreamCalls: [upstreamReadCall(readQuery, setupVariables, updateSetupRead)],
  };

  const restoreResponse = await restoreLocaleIfNeeded(enableQuery, restoreLocaleUpdateMutation, initialFrenchLocale);
  if (restoreResponse !== null) {
    restoreResponses['finalLocaleRestore'] = restoreResponse;
  }

  await mkdir(outputDir, { recursive: true });
  const enableOutputPath = path.join(outputDir, 'localization-shop-locale-enable-web-presence-sync.json');
  const updateOutputPath = path.join(outputDir, 'localization-shop-locale-update-web-presence-sync.json');
  await writeFile(enableOutputPath, `${JSON.stringify({ ...enableCapture, restore: restoreResponses }, null, 2)}\n`);
  await writeFile(updateOutputPath, `${JSON.stringify({ ...updateCapture, restore: restoreResponses }, null, 2)}\n`);
  console.log(`wrote ${enableOutputPath}`);
  console.log(`wrote ${updateOutputPath}`);
} catch (error) {
  const cleanupErrors: JsonRecord = {};
  try {
    if (enableWebPresenceId !== null || updateSourceWebPresenceId !== null || updateTargetWebPresenceId !== null) {
      cleanupErrors['localeDisableAfterFailure'] = await ensureLocaleAbsent(disableQuery);
    }
    if (enableWebPresenceId !== null) {
      cleanupErrors['enableWebPresenceDeleteAfterFailure'] = await deleteWebPresence(deleteQuery, enableWebPresenceId);
    }
    if (updateSourceWebPresenceId !== null) {
      cleanupErrors['updateSourceWebPresenceDeleteAfterFailure'] = await deleteWebPresence(
        deleteQuery,
        updateSourceWebPresenceId,
      );
    }
    if (updateTargetWebPresenceId !== null) {
      cleanupErrors['updateTargetWebPresenceDeleteAfterFailure'] = await deleteWebPresence(
        deleteQuery,
        updateTargetWebPresenceId,
      );
    }
    const restoreResponse = await restoreLocaleIfNeeded(enableQuery, restoreLocaleUpdateMutation, initialFrenchLocale);
    if (restoreResponse !== null) {
      cleanupErrors['localeRestoreAfterFailure'] = restoreResponse;
    }
  } catch (cleanupError) {
    cleanupErrors['cleanupError'] = cleanupError instanceof Error ? cleanupError.message : String(cleanupError);
  }
  console.error(JSON.stringify({ cleanupAfterFailure: cleanupErrors }, null, 2));
  throw error;
}
