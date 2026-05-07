/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-shop-locale-market-web-presence-filter';
const fabricatedWebPresenceId = 'gid://shopify/MarketWebPresence/9999999999';

const setupQueryPath =
  'config/parity-requests/localization/localization-shop-locale-market-web-presence-filter-setup.graphql';
const enableQueryPath = 'config/parity-requests/localization/localization-payload-shapes-shop-locale-enable.graphql';
const updateQueryPath =
  'config/parity-requests/localization/localization-shop-locale-market-web-presence-filter-update.graphql';
const readQueryPath =
  'config/parity-requests/localization/localization-shop-locale-market-web-presence-filter-read.graphql';
const disableQueryPath = 'config/parity-requests/localization/localization-shop-locale-disable.graphql';

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

function payloadField(payload: JsonRecord, fieldName: string): JsonRecord {
  const field = dataObject(payload)[fieldName];
  if (!isRecord(field)) {
    throw new Error(`Expected data.${fieldName} object: ${JSON.stringify(payload)}`);
  }
  return field;
}

function stringField(value: unknown, fieldName: string, context: string): string {
  if (!isRecord(value) || typeof value[fieldName] !== 'string') {
    throw new Error(`Expected ${context}.${fieldName} string: ${JSON.stringify(value)}`);
  }
  return value[fieldName];
}

function userErrors(field: JsonRecord): unknown[] {
  return arrayField(field['userErrors'], 'userErrors');
}

function assertNoUserErrors(field: JsonRecord, context: string): void {
  const errors = userErrors(field);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function shopLocales(payload: JsonRecord): JsonRecord[] {
  return arrayField(dataObject(payload)['allShopLocales'], 'allShopLocales').filter(isRecord);
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

function idsFromPayload(field: JsonRecord): string[] {
  const shopLocale = field['shopLocale'];
  if (!isRecord(shopLocale)) {
    throw new Error(`Expected shopLocale payload: ${JSON.stringify(field)}`);
  }
  return arrayField(shopLocale['marketWebPresences'], 'shopLocale.marketWebPresences')
    .filter(isRecord)
    .map((presence) => stringField(presence, 'id', 'MarketWebPresence'));
}

function assertOnlyValidPresence(field: JsonRecord, validId: string, context: string): void {
  const ids = idsFromPayload(field);
  if (JSON.stringify(ids) !== JSON.stringify([validId])) {
    throw new Error(`${context} expected only ${validId}, got ${JSON.stringify(ids)}`);
  }
}

const setupQuery = await readText(setupQueryPath);
const enableQuery = await readText(enableQueryPath);
const updateQuery = await readText(updateQueryPath);
const readQuery = await readText(readQueryPath);
const disableQuery = await readText(disableQueryPath);

const cleanup: JsonRecord = {};
let capture: JsonRecord | null = null;

try {
  let setupRead = (await runGraphql(setupQuery, {})) as JsonRecord;
  let initialLocales = shopLocales(setupRead);
  const hadFrenchLocale = Boolean(localeSnapshot(initialLocales, 'fr'));
  if (hadFrenchLocale) {
    cleanup['preCaptureShopLocaleDisable'] = await runGraphql(disableQuery, { locale: 'fr' });
    setupRead = (await runGraphql(setupQuery, {})) as JsonRecord;
    initialLocales = shopLocales(setupRead);
  }
  const validWebPresenceId = firstMarketWebPresenceId(initialLocales);
  const mixedWebPresenceIds = [validWebPresenceId, fabricatedWebPresenceId];

  const enableVariables = {
    locale: 'fr',
    marketWebPresenceIds: mixedWebPresenceIds,
  };
  const enable = (await runGraphql(enableQuery, enableVariables)) as JsonRecord;
  const enablePayload = payloadField(enable, 'shopLocaleEnable');
  assertNoUserErrors(enablePayload, 'shopLocaleEnable');
  assertOnlyValidPresence(enablePayload, validWebPresenceId, 'shopLocaleEnable');

  const readAfterEnable = (await runGraphql(readQuery, {})) as JsonRecord;

  const updateVariables = {
    locale: 'fr',
    shopLocale: {
      marketWebPresenceIds: mixedWebPresenceIds,
    },
  };
  const update = (await runGraphql(updateQuery, updateVariables)) as JsonRecord;
  const updatePayload = payloadField(update, 'shopLocaleUpdate');
  assertNoUserErrors(updatePayload, 'shopLocaleUpdate');
  assertOnlyValidPresence(updatePayload, validWebPresenceId, 'shopLocaleUpdate');

  const readAfterUpdate = (await runGraphql(readQuery, {})) as JsonRecord;

  cleanup['shopLocaleDisable'] = await runGraphql(disableQuery, { locale: 'fr' });

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId,
    fabricatedWebPresenceId,
    setup: {
      request: { variables: {} },
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
    update: {
      request: { variables: updateVariables },
      response: update,
    },
    readAfterUpdate: {
      request: { variables: {} },
      response: readAfterUpdate,
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'LocalizationShopLocaleMarketWebPresenceFilterSetup',
        variables: {},
        query: setupQuery,
        response: {
          status: 200,
          body: setupRead,
        },
      },
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(`wrote ${outputPath}`);
} catch (error) {
  cleanup['error'] = error instanceof Error ? error.message : String(error);
  throw error;
} finally {
  if (capture === null) {
    try {
      cleanup['bestEffortShopLocaleDisable'] = await runGraphql(disableQuery, { locale: 'fr' });
    } catch (error) {
      cleanup['bestEffortShopLocaleDisableError'] = error instanceof Error ? error.message : String(error);
    }
  }
}
