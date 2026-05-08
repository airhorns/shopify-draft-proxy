/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const webPresenceFields = `#graphql
  fragment MarketWebPresenceLifecycleFields on MarketWebPresence {
    id
    subfolderSuffix
    domain {
      id
      host
      url
      sslEnabled
    }
    rootUrls {
      locale
      url
    }
    defaultLocale {
      locale
      name
      primary
      published
    }
    alternateLocales {
      locale
      name
      primary
      published
    }
    markets(first: 5) {
      nodes {
        id
        name
        handle
        status
        type
      }
    }
  }
`;

const webPresencesReadQuery = `#graphql
  ${webPresenceFields}
  query MarketWebPresenceLifecycleRead($first: Int!) {
    webPresences(first: $first) {
      nodes {
        ...MarketWebPresenceLifecycleFields
      }
    }
  }
`;

const primaryWebPresenceSetupQuery = `#graphql
  ${webPresenceFields}
  query WebPresenceDeletePrimarySetupRead($first: Int!) {
    webPresences(first: $first) {
      nodes {
        ...MarketWebPresenceLifecycleFields
      }
    }
    shop {
      id
      myshopifyDomain
      primaryDomain {
        host
        url
      }
    }
  }
`;

const createMutation = `#graphql
  ${webPresenceFields}
  mutation MarketWebPresenceLifecycleCreate($input: WebPresenceCreateInput!) {
    webPresenceCreate(input: $input) {
      webPresence {
        ...MarketWebPresenceLifecycleFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const updateMutation = `#graphql
  ${webPresenceFields}
  mutation MarketWebPresenceLifecycleUpdate($id: ID!, $input: WebPresenceUpdateInput!) {
    webPresenceUpdate(id: $id, input: $input) {
      webPresence {
        ...MarketWebPresenceLifecycleFields
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation MarketWebPresenceLifecycleDelete($id: ID!) {
    webPresenceDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const marketCreateMutation = `#graphql
  mutation MarketWebPresenceSuffixMarketCreate($input: MarketCreateInput!) {
    marketCreate(input: $input) {
      market {
        id
        name
        handle
        status
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const marketUpdateMutation = `#graphql
  mutation MarketWebPresenceSuffixMarketUpdate($id: ID!, $input: MarketUpdateInput!) {
    marketUpdate(id: $id, input: $input) {
      market {
        id
        webPresences(first: 5) {
          nodes {
            id
            subfolderSuffix
          }
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

const marketDeleteMutation = `#graphql
  mutation MarketWebPresenceSuffixMarketDelete($id: ID!) {
    marketDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const shopLocalesQuery = `#graphql
  query MarketWebPresenceLocaleSetupRead {
    shopLocales {
      locale
      published
    }
  }
`;

const enableLocaleMutation = `#graphql
  mutation MarketWebPresenceLocaleSetupEnable($locale: String!) {
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

const disableLocaleMutation = `#graphql
  mutation MarketWebPresenceLocaleSetupDisable($locale: String!) {
    shopLocaleDisable(locale: $locale) {
      locale
      userErrors {
        field
        message
      }
    }
  }
`;

const updateLocaleMutation = `#graphql
  mutation MarketWebPresenceLocaleSetupUpdate($locale: String!, $shopLocale: ShopLocaleInput!) {
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

type LocaleRestoreAction = { locale: string; kind: 'disable' } | { locale: string; kind: 'update'; published: boolean };

function randomLetters(length: number): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz';
  return Array.from({ length }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join('');
}

function readUserErrors(payload: unknown, root: string): unknown[] {
  if (
    typeof payload !== 'object' ||
    payload === null ||
    !('data' in payload) ||
    typeof payload.data !== 'object' ||
    payload.data === null ||
    !(root in payload.data) ||
    typeof payload.data[root as keyof typeof payload.data] !== 'object' ||
    payload.data[root as keyof typeof payload.data] === null
  ) {
    return [];
  }
  const rootPayload = payload.data[root as keyof typeof payload.data] as { userErrors?: unknown };
  return Array.isArray(rootPayload.userErrors) ? rootPayload.userErrors : [];
}

function readMarketCreateId(payload: unknown, label: string): string {
  const data = typeof payload === 'object' && payload !== null ? (payload as { data?: unknown }).data : undefined;
  const marketCreate =
    typeof data === 'object' && data !== null ? (data as { marketCreate?: unknown }).marketCreate : undefined;
  const market =
    typeof marketCreate === 'object' && marketCreate !== null
      ? (marketCreate as { market?: unknown }).market
      : undefined;
  const id = typeof market === 'object' && market !== null ? (market as { id?: unknown }).id : undefined;
  if (typeof id !== 'string') {
    throw new Error(`${label} did not return a market id: ${JSON.stringify(payload)}`);
  }
  return id;
}

function assertNoUserErrors(payload: unknown, root: string, label: string): void {
  const userErrors = readUserErrors(payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function normalizeHost(value: string): string {
  let host = value.trim().toLowerCase();
  if (host.startsWith('http://') || host.startsWith('https://')) {
    try {
      host = new URL(host).host;
    } catch {
      host = host.replace(/^https?:\/\//, '').split('/')[0] ?? host;
    }
  } else {
    host = host.split('/')[0] ?? host;
  }
  return host.replace(/\.$/, '');
}

function isPrimaryRootUrl(value: unknown, primaryHost: string): boolean {
  if (typeof value !== 'string') return false;
  try {
    const url = new URL(value);
    return normalizeHost(url.host) === primaryHost && (url.pathname === '' || url.pathname === '/');
  } catch {
    return false;
  }
}

function findPrimaryWebPresence(payload: unknown): { id: string; node: unknown } {
  if (typeof payload !== 'object' || payload === null || !('data' in payload)) {
    throw new Error(`Expected primary setup GraphQL data: ${JSON.stringify(payload)}`);
  }
  const data = (payload as { data?: unknown }).data;
  if (typeof data !== 'object' || data === null) {
    throw new Error(`Expected primary setup data object: ${JSON.stringify(payload)}`);
  }
  const shop = (data as { shop?: unknown }).shop;
  if (typeof shop !== 'object' || shop === null) {
    throw new Error(`Expected shop in primary setup response: ${JSON.stringify(payload)}`);
  }
  const primaryDomain = (shop as { primaryDomain?: unknown }).primaryDomain;
  const primaryHost =
    typeof primaryDomain === 'object' &&
    primaryDomain !== null &&
    typeof (primaryDomain as { host?: unknown }).host === 'string'
      ? normalizeHost((primaryDomain as { host: string }).host)
      : undefined;
  if (!primaryHost) {
    throw new Error(`Expected shop.primaryDomain.host in primary setup response: ${JSON.stringify(payload)}`);
  }
  const webPresences = (data as { webPresences?: unknown }).webPresences;
  const nodes =
    typeof webPresences === 'object' &&
    webPresences !== null &&
    Array.isArray((webPresences as { nodes?: unknown }).nodes)
      ? (webPresences as { nodes: unknown[] }).nodes
      : [];

  for (const node of nodes) {
    if (typeof node !== 'object' || node === null || typeof (node as { id?: unknown }).id !== 'string') continue;
    const domain = (node as { domain?: unknown }).domain;
    const domainHost =
      typeof domain === 'object' && domain !== null && typeof (domain as { host?: unknown }).host === 'string'
        ? normalizeHost((domain as { host: string }).host)
        : undefined;
    const rootUrls = Array.isArray((node as { rootUrls?: unknown }).rootUrls)
      ? (node as { rootUrls: unknown[] }).rootUrls
      : [];
    const hasPrimaryRootUrl = rootUrls.some((rootUrl) => {
      if (typeof rootUrl !== 'object' || rootUrl === null) return false;
      return isPrimaryRootUrl((rootUrl as { url?: unknown }).url, primaryHost);
    });
    if (domainHost === primaryHost || hasPrimaryRootUrl) {
      return { id: (node as { id: string }).id, node };
    }
  }

  throw new Error(`Could not find primary-host web presence for ${primaryHost}: ${JSON.stringify(nodes)}`);
}

function isAlreadyAbsentLocaleCleanup(action: LocaleRestoreAction, userErrors: unknown[]): boolean {
  return (
    action.kind === 'disable' &&
    userErrors.length === 1 &&
    typeof userErrors[0] === 'object' &&
    userErrors[0] !== null &&
    'message' in userErrors[0] &&
    userErrors[0].message === "The locale doesn't exist."
  );
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createSuffix = `har${randomLetters(10)}`;
const updateSuffix = `har${randomLetters(10)}`;
const multiLocaleSuffix = 'intl';
const frenchCanadianSuffix = 'fr';
const partialUpdateSuffix = `har${randomLetters(10)}`;
const regionalLocaleSuffix = `har${randomLetters(10)}`;
const caseInsensitiveSuffix = `har${randomLetters(10)}`;
const duplicateCreateSuffix = `har${randomLetters(10)}`;
const updateCollisionSourceSuffix = `har${randomLetters(10)}`;
const updateCollisionTakenSuffix = `har${randomLetters(10)}`;
const duplicateLanguageCreateDefaultSuffix = `har${randomLetters(10)}`;
const duplicateLanguageCreateAlternateSuffix = `har${randomLetters(10)}`;
const duplicateLanguageUpdateDefaultSuffix = `har${randomLetters(10)}`;
const duplicateLanguageUpdateAlternateSuffix = `har${randomLetters(10)}`;
const nonLetterUpdateSourceSuffix = `har${randomLetters(10)}`;
const unique = Date.now().toString(36);
let createdWebPresenceId: string | null = null;
let multiLocaleWebPresenceId: string | null = null;
let frenchCanadianWebPresenceId: string | null = null;
let partialUpdateWebPresenceId: string | null = null;
let regionalLocaleWebPresenceId: string | null = null;
let caseInsensitiveWebPresenceId: string | null = null;
let duplicateCreateWebPresenceId: string | null = null;
let duplicateCreateUnexpectedWebPresenceId: string | null = null;
let duplicateCreateMarketId: string | null = null;
let updateCollisionSourceWebPresenceId: string | null = null;
let updateCollisionTakenWebPresenceId: string | null = null;
let updateCollisionMarketId: string | null = null;
let duplicateLanguageCreateDefaultUnexpectedWebPresenceId: string | null = null;
let duplicateLanguageCreateAlternateUnexpectedWebPresenceId: string | null = null;
let duplicateLanguageUpdateDefaultWebPresenceId: string | null = null;
let duplicateLanguageUpdateAlternateWebPresenceId: string | null = null;
let nonLetterUpdateSourceWebPresenceId: string | null = null;
let nonLetterCreateValidUsWebPresenceId: string | null = null;
let cleanupResponse: unknown = null;
let multiLocaleCleanupResponse: unknown = null;
let frenchCanadianCleanupResponse: unknown = null;
let partialUpdateCleanupResponse: unknown = null;
let regionalLocaleCleanupResponse: unknown = null;
let caseInsensitiveCleanupResponse: unknown = null;
let duplicateCreateMarketCleanupResponse: unknown = null;
let duplicateCreateCleanupResponse: unknown = null;
let duplicateCreateUnexpectedCleanupResponse: unknown = null;
let updateCollisionMarketCleanupResponse: unknown = null;
let updateCollisionSourceCleanupResponse: unknown = null;
let updateCollisionTakenCleanupResponse: unknown = null;
let duplicateLanguageCreateDefaultUnexpectedCleanupResponse: unknown = null;
let duplicateLanguageCreateAlternateUnexpectedCleanupResponse: unknown = null;
let duplicateLanguageUpdateDefaultCleanupResponse: unknown = null;
let duplicateLanguageUpdateAlternateCleanupResponse: unknown = null;
let nonLetterUpdateSourceCleanupResponse: unknown = null;
let nonLetterCreateValidUsCleanupResponse: unknown = null;
const localeRestoreActions: LocaleRestoreAction[] = [];
let localeCleanupResponses: Record<string, unknown> = {};

async function ensureLocalesEnabled(locales: string[]): Promise<void> {
  const payload = await runGraphql(shopLocalesQuery, {});
  const existingLocales = new Map<string, { published: boolean }>(
    (payload.data?.shopLocales ?? [])
      .filter(
        (locale: { locale?: unknown; published?: unknown }) =>
          typeof locale.locale === 'string' && typeof locale.published === 'boolean',
      )
      .map((locale: { locale: string; published: boolean }) => [locale.locale, { published: locale.published }]),
  );

  for (const locale of locales) {
    const existing = existingLocales.get(locale);
    if (existing === undefined) {
      localeRestoreActions.push({ locale, kind: 'disable' });
      const enablePayload = await runGraphql(enableLocaleMutation, { locale });
      const userErrors = readUserErrors(enablePayload, 'shopLocaleEnable');
      if (userErrors.length > 0) {
        throw new Error(`shopLocaleEnable(${locale}) failed: ${JSON.stringify(userErrors)}`);
      }
    } else if (!existing.published) {
      localeRestoreActions.push({ locale, kind: 'update', published: existing.published });
    } else {
      continue;
    }

    const publishPayload = await runGraphql(updateLocaleMutation, {
      locale,
      shopLocale: { published: true },
    });
    const userErrors = readUserErrors(publishPayload, 'shopLocaleUpdate');
    if (userErrors.length > 0) {
      throw new Error(`shopLocaleUpdate(${locale}, published: true) failed: ${JSON.stringify(userErrors)}`);
    }
  }
}

async function restoreEnabledLocales(): Promise<Record<string, unknown>> {
  const responses: Record<string, unknown> = {};
  for (const action of localeRestoreActions.toReversed()) {
    const payload =
      action.kind === 'disable'
        ? await runGraphql(disableLocaleMutation, { locale: action.locale })
        : await runGraphql(updateLocaleMutation, {
            locale: action.locale,
            shopLocale: { published: action.published },
          });
    const root = action.kind === 'disable' ? 'shopLocaleDisable' : 'shopLocaleUpdate';
    const userErrors = readUserErrors(payload, root);
    if (userErrors.length > 0 && !isAlreadyAbsentLocaleCleanup(action, userErrors)) {
      throw new Error(`locale cleanup failed for ${action.locale}: ${JSON.stringify(userErrors)}`);
    }
    responses[action.locale] = payload;
  }
  return responses;
}

try {
  const primarySetupVariables = { first: 20 };
  const primarySetupRead = await runGraphql(primaryWebPresenceSetupQuery, primarySetupVariables);
  const primaryWebPresence = findPrimaryWebPresence(primarySetupRead);
  const primaryDeleteVariables = { id: primaryWebPresence.id };
  const primaryDeleteResponse = await runGraphql(deleteMutation, primaryDeleteVariables);
  const primaryReadAfterDelete = await runGraphql(webPresencesReadQuery, { first: 20 });

  const duplicateCreateMarketVariables = {
    input: {
      name: `Web Presence Duplicate Source ${unique}`,
    },
  };
  const duplicateCreateMarketResponse = await runGraphql(marketCreateMutation, duplicateCreateMarketVariables);
  assertNoUserErrors(duplicateCreateMarketResponse, 'marketCreate', 'duplicate-source marketCreate');
  duplicateCreateMarketId = readMarketCreateId(duplicateCreateMarketResponse, 'duplicate-source marketCreate');
  const duplicateCreateSourceVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: duplicateCreateSuffix,
    },
  };
  const duplicateCreateSourceResponse = await runGraphql(createMutation, duplicateCreateSourceVariables);
  duplicateCreateWebPresenceId = duplicateCreateSourceResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!duplicateCreateWebPresenceId) {
    throw new Error(
      `duplicate-source webPresenceCreate did not return a disposable web presence id: ${JSON.stringify(
        duplicateCreateSourceResponse,
        null,
        2,
      )}`,
    );
  }
  const duplicateCreateMarketLinkVariables = {
    id: duplicateCreateMarketId,
    input: {
      webPresencesToAdd: [duplicateCreateWebPresenceId],
    },
  };
  const duplicateCreateMarketLinkResponse = await runGraphql(marketUpdateMutation, duplicateCreateMarketLinkVariables);
  assertNoUserErrors(duplicateCreateMarketLinkResponse, 'marketUpdate', 'duplicate-source marketUpdate');
  const duplicateCreateTakenVariables = duplicateCreateSourceVariables;
  const duplicateCreateTakenResponse = await runGraphql(createMutation, duplicateCreateTakenVariables);
  duplicateCreateUnexpectedWebPresenceId =
    duplicateCreateTakenResponse.data?.webPresenceCreate?.webPresence?.id ?? null;

  const updateCollisionMarketVariables = {
    input: {
      name: `Web Presence Update Collision ${unique}`,
    },
  };
  const updateCollisionMarketResponse = await runGraphql(marketCreateMutation, updateCollisionMarketVariables);
  assertNoUserErrors(updateCollisionMarketResponse, 'marketCreate', 'update-collision marketCreate');
  updateCollisionMarketId = readMarketCreateId(updateCollisionMarketResponse, 'update-collision marketCreate');
  const updateCollisionSourceCreateVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: updateCollisionSourceSuffix,
    },
  };
  const updateCollisionSourceCreateResponse = await runGraphql(createMutation, updateCollisionSourceCreateVariables);
  updateCollisionSourceWebPresenceId =
    updateCollisionSourceCreateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!updateCollisionSourceWebPresenceId) {
    throw new Error(
      `update-collision source webPresenceCreate did not return a disposable web presence id: ${JSON.stringify(
        updateCollisionSourceCreateResponse,
        null,
        2,
      )}`,
    );
  }
  const updateCollisionTakenCreateVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: updateCollisionTakenSuffix,
    },
  };
  const updateCollisionTakenCreateResponse = await runGraphql(createMutation, updateCollisionTakenCreateVariables);
  updateCollisionTakenWebPresenceId =
    updateCollisionTakenCreateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!updateCollisionTakenWebPresenceId) {
    throw new Error(
      `update-collision taken webPresenceCreate did not return a disposable web presence id: ${JSON.stringify(
        updateCollisionTakenCreateResponse,
        null,
        2,
      )}`,
    );
  }
  const updateCollisionMarketLinkVariables = {
    id: updateCollisionMarketId,
    input: {
      webPresencesToAdd: [updateCollisionTakenWebPresenceId],
    },
  };
  const updateCollisionMarketLinkResponse = await runGraphql(marketUpdateMutation, updateCollisionMarketLinkVariables);
  assertNoUserErrors(updateCollisionMarketLinkResponse, 'marketUpdate', 'update-collision marketUpdate');
  const updateCollisionVariables = {
    id: updateCollisionSourceWebPresenceId,
    input: {
      subfolderSuffix: updateCollisionTakenSuffix,
    },
  };
  const updateCollisionResponse = await runGraphql(updateMutation, updateCollisionVariables);
  updateCollisionSourceCleanupResponse = await runGraphql(deleteMutation, { id: updateCollisionSourceWebPresenceId });
  updateCollisionTakenCleanupResponse = await runGraphql(deleteMutation, { id: updateCollisionTakenWebPresenceId });
  updateCollisionMarketCleanupResponse = await runGraphql(marketDeleteMutation, { id: updateCollisionMarketId });
  duplicateCreateCleanupResponse = await runGraphql(deleteMutation, { id: duplicateCreateWebPresenceId });
  duplicateCreateMarketCleanupResponse = await runGraphql(marketDeleteMutation, { id: duplicateCreateMarketId });
  if (
    duplicateCreateUnexpectedWebPresenceId &&
    duplicateCreateUnexpectedWebPresenceId !== duplicateCreateWebPresenceId
  ) {
    duplicateCreateUnexpectedCleanupResponse = await runGraphql(deleteMutation, {
      id: duplicateCreateUnexpectedWebPresenceId,
    });
  }

  const baselineRead = await runGraphql(webPresencesReadQuery, { first: 20 });
  const createVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: createSuffix,
    },
  };
  const createResponse = await runGraphql(createMutation, createVariables);
  createdWebPresenceId = createResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!createdWebPresenceId) {
    throw new Error('webPresenceCreate did not return a disposable web presence id.');
  }

  const updateVariables = {
    id: createdWebPresenceId,
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: updateSuffix,
    },
  };
  const updateResponse = await runGraphql(updateMutation, updateVariables);
  const readAfterUpdate = await runGraphql(webPresencesReadQuery, { first: 20 });
  const deleteResponse = await runGraphql(deleteMutation, { id: createdWebPresenceId });
  cleanupResponse = deleteResponse;
  const readAfterDelete = await runGraphql(webPresencesReadQuery, { first: 20 });

  await ensureLocalesEnabled(['fr', 'de']);
  const multiLocaleCreateVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: ['fr', 'de'],
      subfolderSuffix: multiLocaleSuffix,
    },
  };
  const multiLocaleCreateResponse = await runGraphql(createMutation, multiLocaleCreateVariables);
  multiLocaleWebPresenceId = multiLocaleCreateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!multiLocaleWebPresenceId) {
    throw new Error(
      `multi-locale webPresenceCreate did not return a disposable web presence id: ${JSON.stringify(
        multiLocaleCreateResponse,
        null,
        2,
      )}`,
    );
  }
  multiLocaleCleanupResponse = await runGraphql(deleteMutation, { id: multiLocaleWebPresenceId });

  const frenchCanadianCreateVariables = {
    input: {
      defaultLocale: 'fr-CA',
      alternateLocales: [],
      subfolderSuffix: frenchCanadianSuffix,
    },
  };
  const frenchCanadianCreateResponse = await runGraphql(createMutation, frenchCanadianCreateVariables);
  frenchCanadianWebPresenceId = frenchCanadianCreateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!frenchCanadianWebPresenceId) {
    throw new Error(
      `fr-CA webPresenceCreate did not return a disposable web presence id: ${JSON.stringify(
        frenchCanadianCreateResponse,
        null,
        2,
      )}`,
    );
  }
  frenchCanadianCleanupResponse = await runGraphql(deleteMutation, { id: frenchCanadianWebPresenceId });

  await ensureLocalesEnabled(['pt-BR']);
  const regionalLocaleCreateVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: ['pt-BR'],
      subfolderSuffix: regionalLocaleSuffix,
    },
  };
  const regionalLocaleCreateResponse = await runGraphql(createMutation, regionalLocaleCreateVariables);
  regionalLocaleWebPresenceId = regionalLocaleCreateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!regionalLocaleWebPresenceId) {
    throw new Error(
      `regional-locale webPresenceCreate did not return a disposable web presence id: ${JSON.stringify(
        regionalLocaleCreateResponse,
        null,
        2,
      )}`,
    );
  }
  regionalLocaleCleanupResponse = await runGraphql(deleteMutation, { id: regionalLocaleWebPresenceId });

  const caseInsensitiveCreateVariables = {
    input: {
      defaultLocale: 'en-us',
      alternateLocales: [],
      subfolderSuffix: caseInsensitiveSuffix,
    },
  };
  const caseInsensitiveCreateResponse = await runGraphql(createMutation, caseInsensitiveCreateVariables);
  caseInsensitiveWebPresenceId = caseInsensitiveCreateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!caseInsensitiveWebPresenceId) {
    throw new Error(
      `case-insensitive webPresenceCreate did not return a disposable web presence id: ${JSON.stringify(
        caseInsensitiveCreateResponse,
        null,
        2,
      )}`,
    );
  }
  caseInsensitiveCleanupResponse = await runGraphql(deleteMutation, { id: caseInsensitiveWebPresenceId });

  const invalidAlternatesCreateVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: ['zz', 'yy'],
      subfolderSuffix: `har${randomLetters(10)}`,
    },
  };
  const invalidAlternatesCreateResponse = await runGraphql(createMutation, invalidAlternatesCreateVariables);

  const partialUpdateCreateVariables = {
    input: {
      defaultLocale: 'fr',
      alternateLocales: [],
      subfolderSuffix: partialUpdateSuffix,
    },
  };
  const partialUpdateCreateResponse = await runGraphql(createMutation, partialUpdateCreateVariables);
  partialUpdateWebPresenceId = partialUpdateCreateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!partialUpdateWebPresenceId) {
    throw new Error(
      `partial-update webPresenceCreate did not return a disposable web presence id: ${JSON.stringify(
        partialUpdateCreateResponse,
        null,
        2,
      )}`,
    );
  }
  const partialUpdateVariables = {
    id: partialUpdateWebPresenceId,
    input: {
      alternateLocales: ['de'],
    },
  };
  const partialUpdateResponse = await runGraphql(updateMutation, partialUpdateVariables);
  partialUpdateCleanupResponse = await runGraphql(deleteMutation, { id: partialUpdateWebPresenceId });

  const nonLetterCreateUs2Variables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: 'us2',
    },
  };
  const nonLetterCreateUs2Response = await runGraphql(createMutation, nonLetterCreateUs2Variables);
  const nonLetterCreateEn1Variables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: 'en1',
    },
  };
  const nonLetterCreateEn1Response = await runGraphql(createMutation, nonLetterCreateEn1Variables);
  const nonLetterCreateUsEastVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: 'us-east',
    },
  };
  const nonLetterCreateUsEastResponse = await runGraphql(createMutation, nonLetterCreateUsEastVariables);
  const nonLetterCreateValidUsVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: 'us',
    },
  };
  const nonLetterCreateValidUsResponse = await runGraphql(createMutation, nonLetterCreateValidUsVariables);
  nonLetterCreateValidUsWebPresenceId = nonLetterCreateValidUsResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!nonLetterCreateValidUsWebPresenceId) {
    throw new Error(
      `non-letter valid-us webPresenceCreate did not return a disposable web presence id: ${JSON.stringify(
        nonLetterCreateValidUsResponse,
        null,
        2,
      )}`,
    );
  }
  nonLetterCreateValidUsCleanupResponse = await runGraphql(deleteMutation, { id: nonLetterCreateValidUsWebPresenceId });

  const nonLetterUpdateSourceCreateVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: nonLetterUpdateSourceSuffix,
    },
  };
  const nonLetterUpdateSourceCreateResponse = await runGraphql(createMutation, nonLetterUpdateSourceCreateVariables);
  nonLetterUpdateSourceWebPresenceId =
    nonLetterUpdateSourceCreateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!nonLetterUpdateSourceWebPresenceId) {
    throw new Error(
      `non-letter update source webPresenceCreate did not return a disposable web presence id: ${JSON.stringify(
        nonLetterUpdateSourceCreateResponse,
        null,
        2,
      )}`,
    );
  }
  const nonLetterUpdateUs2Variables = {
    id: nonLetterUpdateSourceWebPresenceId,
    input: {
      subfolderSuffix: 'us2',
    },
  };
  const nonLetterUpdateUs2Response = await runGraphql(updateMutation, nonLetterUpdateUs2Variables);
  const nonLetterUpdateEn1Variables = {
    id: nonLetterUpdateSourceWebPresenceId,
    input: {
      subfolderSuffix: 'en1',
    },
  };
  const nonLetterUpdateEn1Response = await runGraphql(updateMutation, nonLetterUpdateEn1Variables);
  const nonLetterUpdateUsEastVariables = {
    id: nonLetterUpdateSourceWebPresenceId,
    input: {
      subfolderSuffix: 'us-east',
    },
  };
  const nonLetterUpdateUsEastResponse = await runGraphql(updateMutation, nonLetterUpdateUsEastVariables);
  const nonLetterUpdateValidUsVariables = {
    id: nonLetterUpdateSourceWebPresenceId,
    input: {
      subfolderSuffix: 'us',
    },
  };
  const nonLetterUpdateValidUsResponse = await runGraphql(updateMutation, nonLetterUpdateValidUsVariables);
  nonLetterUpdateSourceCleanupResponse = await runGraphql(deleteMutation, { id: nonLetterUpdateSourceWebPresenceId });

  const duplicateLanguageCreateDefaultVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: ['en', 'fr'],
      subfolderSuffix: duplicateLanguageCreateDefaultSuffix,
    },
  };
  const duplicateLanguageCreateDefaultResponse = await runGraphql(
    createMutation,
    duplicateLanguageCreateDefaultVariables,
  );
  duplicateLanguageCreateDefaultUnexpectedWebPresenceId =
    duplicateLanguageCreateDefaultResponse.data?.webPresenceCreate?.webPresence?.id ?? null;

  const duplicateLanguageCreateAlternateVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: ['en', 'en'],
      subfolderSuffix: duplicateLanguageCreateAlternateSuffix,
    },
  };
  const duplicateLanguageCreateAlternateResponse = await runGraphql(
    createMutation,
    duplicateLanguageCreateAlternateVariables,
  );
  duplicateLanguageCreateAlternateUnexpectedWebPresenceId =
    duplicateLanguageCreateAlternateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;

  const duplicateLanguageUpdateDefaultCreateVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: ['fr'],
      subfolderSuffix: duplicateLanguageUpdateDefaultSuffix,
    },
  };
  const duplicateLanguageUpdateDefaultCreateResponse = await runGraphql(
    createMutation,
    duplicateLanguageUpdateDefaultCreateVariables,
  );
  duplicateLanguageUpdateDefaultWebPresenceId =
    duplicateLanguageUpdateDefaultCreateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!duplicateLanguageUpdateDefaultWebPresenceId) {
    throw new Error(
      `duplicate-language update-default setup did not return a disposable web presence id: ${JSON.stringify(
        duplicateLanguageUpdateDefaultCreateResponse,
        null,
        2,
      )}`,
    );
  }
  const duplicateLanguageUpdateDefaultVariables = {
    id: duplicateLanguageUpdateDefaultWebPresenceId,
    input: {
      defaultLocale: 'fr',
    },
  };
  const duplicateLanguageUpdateDefaultResponse = await runGraphql(
    updateMutation,
    duplicateLanguageUpdateDefaultVariables,
  );
  duplicateLanguageUpdateDefaultCleanupResponse = await runGraphql(deleteMutation, {
    id: duplicateLanguageUpdateDefaultWebPresenceId,
  });

  const duplicateLanguageUpdateAlternateCreateVariables = {
    input: {
      defaultLocale: 'en',
      alternateLocales: [],
      subfolderSuffix: duplicateLanguageUpdateAlternateSuffix,
    },
  };
  const duplicateLanguageUpdateAlternateCreateResponse = await runGraphql(
    createMutation,
    duplicateLanguageUpdateAlternateCreateVariables,
  );
  duplicateLanguageUpdateAlternateWebPresenceId =
    duplicateLanguageUpdateAlternateCreateResponse.data?.webPresenceCreate?.webPresence?.id ?? null;
  if (!duplicateLanguageUpdateAlternateWebPresenceId) {
    throw new Error(
      `duplicate-language update-alternate setup did not return a disposable web presence id: ${JSON.stringify(
        duplicateLanguageUpdateAlternateCreateResponse,
        null,
        2,
      )}`,
    );
  }
  const duplicateLanguageUpdateAlternateVariables = {
    id: duplicateLanguageUpdateAlternateWebPresenceId,
    input: {
      alternateLocales: ['en', 'en'],
    },
  };
  const duplicateLanguageUpdateAlternateResponse = await runGraphql(
    updateMutation,
    duplicateLanguageUpdateAlternateVariables,
  );
  duplicateLanguageUpdateAlternateCleanupResponse = await runGraphql(deleteMutation, {
    id: duplicateLanguageUpdateAlternateWebPresenceId,
  });

  if (duplicateLanguageCreateDefaultUnexpectedWebPresenceId) {
    duplicateLanguageCreateDefaultUnexpectedCleanupResponse = await runGraphql(deleteMutation, {
      id: duplicateLanguageCreateDefaultUnexpectedWebPresenceId,
    });
  }
  if (duplicateLanguageCreateAlternateUnexpectedWebPresenceId) {
    duplicateLanguageCreateAlternateUnexpectedCleanupResponse = await runGraphql(deleteMutation, {
      id: duplicateLanguageCreateAlternateUnexpectedWebPresenceId,
    });
  }

  localeCleanupResponses = await restoreEnabledLocales();

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    disposableSubfolderSuffixes: {
      created: createSuffix,
      updated: updateSuffix,
      multiLocale: multiLocaleSuffix,
      frenchCanadian: frenchCanadianSuffix,
      partialUpdate: partialUpdateSuffix,
      regionalLocale: regionalLocaleSuffix,
      caseInsensitive: caseInsensitiveSuffix,
      duplicateCreate: duplicateCreateSuffix,
      updateCollisionSource: updateCollisionSourceSuffix,
      updateCollisionTaken: updateCollisionTakenSuffix,
      duplicateLanguageCreateDefault: duplicateLanguageCreateDefaultSuffix,
      duplicateLanguageCreateAlternate: duplicateLanguageCreateAlternateSuffix,
      duplicateLanguageUpdateDefault: duplicateLanguageUpdateDefaultSuffix,
      duplicateLanguageUpdateAlternate: duplicateLanguageUpdateAlternateSuffix,
      nonLetterUpdateSource: nonLetterUpdateSourceSuffix,
    },
    scope:
      'HAR-448 market web presence create/update/delete lifecycle parity plus HAR-613 multi-locale rootUrls parity, HAR-611 fr-CA default locale parity, web-presence locale catalog/error-shape parity, primary-domain delete guard parity, duplicate subfolder suffix validation parity, duplicate-language validation parity, and non-letter subfolder suffix validation parity',
    data: {
      shop: primarySetupRead.data?.shop,
      webPresences: baselineRead.data?.webPresences,
    },
    har613Expected: {
      webPresenceCreateMultiLocaleRootUrls: {
        data: {
          webPresenceCreate: {
            webPresence: {
              id: multiLocaleWebPresenceId,
              subfolderSuffix: multiLocaleSuffix,
              domain: null,
              rootUrls: [
                {
                  locale: 'en',
                  url: `https://${storeDomain}/${multiLocaleSuffix}/`,
                },
                {
                  locale: 'fr',
                  url: `https://${storeDomain}/${multiLocaleSuffix}/fr/`,
                },
                {
                  locale: 'de',
                  url: `https://${storeDomain}/${multiLocaleSuffix}/de/`,
                },
              ],
              defaultLocale: {
                locale: 'en',
                name: 'English',
                primary: true,
                published: true,
              },
              alternateLocales: [
                {
                  locale: 'fr',
                  name: 'French',
                  primary: false,
                  published: true,
                },
                {
                  locale: 'de',
                  name: 'German',
                  primary: false,
                  published: true,
                },
              ],
              markets: {
                nodes: [],
              },
            },
            userErrors: [],
          },
        },
      },
    },
    cases: [
      {
        name: 'webPresenceCreateSuccess',
        query: createMutation,
        variables: createVariables,
        response: {
          status: 200,
          payload: createResponse,
        },
      },
      {
        name: 'webPresenceUpdateSuccess',
        query: updateMutation,
        variables: updateVariables,
        response: {
          status: 200,
          payload: updateResponse,
        },
      },
      {
        name: 'webPresenceReadAfterUpdate',
        query: webPresencesReadQuery,
        variables: { first: 20 },
        response: {
          status: 200,
          payload: readAfterUpdate,
        },
      },
      {
        name: 'webPresenceDeleteSuccess',
        query: deleteMutation,
        variables: { id: createdWebPresenceId },
        response: {
          status: 200,
          payload: deleteResponse,
        },
      },
      {
        name: 'webPresenceReadAfterDelete',
        query: webPresencesReadQuery,
        variables: { first: 20 },
        response: {
          status: 200,
          payload: readAfterDelete,
        },
      },
      {
        name: 'webPresenceCreateMultiLocaleRootUrls',
        query: createMutation,
        variables: multiLocaleCreateVariables,
        response: {
          status: 200,
          payload: multiLocaleCreateResponse,
        },
      },
      {
        name: 'webPresenceCreateFrenchCanadianDefaultLocale',
        query: createMutation,
        variables: frenchCanadianCreateVariables,
        response: {
          status: 200,
          payload: frenchCanadianCreateResponse,
        },
      },
      {
        name: 'webPresenceCreateRegionalAlternateLocales',
        query: createMutation,
        variables: regionalLocaleCreateVariables,
        response: {
          status: 200,
          payload: regionalLocaleCreateResponse,
        },
      },
      {
        name: 'webPresenceCreateCaseInsensitiveLocales',
        query: createMutation,
        variables: caseInsensitiveCreateVariables,
        response: {
          status: 200,
          payload: caseInsensitiveCreateResponse,
        },
      },
      {
        name: 'webPresenceCreateInvalidAlternatesCombined',
        query: createMutation,
        variables: invalidAlternatesCreateVariables,
        response: {
          status: 200,
          payload: invalidAlternatesCreateResponse,
        },
      },
      {
        name: 'webPresencePartialUpdateCreate',
        query: createMutation,
        variables: partialUpdateCreateVariables,
        response: {
          status: 200,
          payload: partialUpdateCreateResponse,
        },
      },
      {
        name: 'webPresencePartialUpdateAlternateLocalesOnly',
        query: updateMutation,
        variables: partialUpdateVariables,
        response: {
          status: 200,
          payload: partialUpdateResponse,
        },
      },
      {
        name: 'webPresenceDeletePrimarySetupRead',
        query: primaryWebPresenceSetupQuery,
        variables: primarySetupVariables,
        response: {
          status: 200,
          payload: primarySetupRead,
        },
      },
      {
        name: 'webPresenceDeletePrimaryBlocked',
        query: deleteMutation,
        variables: primaryDeleteVariables,
        response: {
          status: 200,
          payload: primaryDeleteResponse,
        },
      },
      {
        name: 'webPresenceReadAfterPrimaryBlockedDelete',
        query: webPresencesReadQuery,
        variables: { first: 20 },
        response: {
          status: 200,
          payload: primaryReadAfterDelete,
        },
      },
      {
        name: 'webPresenceDuplicateSuffixMarketCreate',
        query: marketCreateMutation,
        variables: duplicateCreateMarketVariables,
        response: {
          status: 200,
          payload: duplicateCreateMarketResponse,
        },
      },
      {
        name: 'webPresenceDuplicateSuffixSourceCreate',
        query: createMutation,
        variables: duplicateCreateSourceVariables,
        response: {
          status: 200,
          payload: duplicateCreateSourceResponse,
        },
      },
      {
        name: 'webPresenceDuplicateSuffixMarketLink',
        query: marketUpdateMutation,
        variables: duplicateCreateMarketLinkVariables,
        response: {
          status: 200,
          payload: duplicateCreateMarketLinkResponse,
        },
      },
      {
        name: 'webPresenceDuplicateSuffixCreateTaken',
        query: createMutation,
        variables: duplicateCreateTakenVariables,
        response: {
          status: 200,
          payload: duplicateCreateTakenResponse,
        },
      },
      {
        name: 'webPresenceUpdateCollisionMarketCreate',
        query: marketCreateMutation,
        variables: updateCollisionMarketVariables,
        response: {
          status: 200,
          payload: updateCollisionMarketResponse,
        },
      },
      {
        name: 'webPresenceUpdateCollisionSourceCreate',
        query: createMutation,
        variables: updateCollisionSourceCreateVariables,
        response: {
          status: 200,
          payload: updateCollisionSourceCreateResponse,
        },
      },
      {
        name: 'webPresenceUpdateCollisionTakenCreate',
        query: createMutation,
        variables: updateCollisionTakenCreateVariables,
        response: {
          status: 200,
          payload: updateCollisionTakenCreateResponse,
        },
      },
      {
        name: 'webPresenceUpdateCollisionMarketLink',
        query: marketUpdateMutation,
        variables: updateCollisionMarketLinkVariables,
        response: {
          status: 200,
          payload: updateCollisionMarketLinkResponse,
        },
      },
      {
        name: 'webPresenceUpdateSubfolderSuffixTaken',
        query: updateMutation,
        variables: updateCollisionVariables,
        response: {
          status: 200,
          payload: updateCollisionResponse,
        },
      },
      {
        name: 'webPresenceCreateNonLetterUs2',
        query: createMutation,
        variables: nonLetterCreateUs2Variables,
        response: {
          status: 200,
          payload: nonLetterCreateUs2Response,
        },
      },
      {
        name: 'webPresenceCreateNonLetterEn1',
        query: createMutation,
        variables: nonLetterCreateEn1Variables,
        response: {
          status: 200,
          payload: nonLetterCreateEn1Response,
        },
      },
      {
        name: 'webPresenceCreateNonLetterUsEast',
        query: createMutation,
        variables: nonLetterCreateUsEastVariables,
        response: {
          status: 200,
          payload: nonLetterCreateUsEastResponse,
        },
      },
      {
        name: 'webPresenceCreateValidUs',
        query: createMutation,
        variables: nonLetterCreateValidUsVariables,
        response: {
          status: 200,
          payload: nonLetterCreateValidUsResponse,
        },
      },
      {
        name: 'webPresenceUpdateNonLetterSourceCreate',
        query: createMutation,
        variables: nonLetterUpdateSourceCreateVariables,
        response: {
          status: 200,
          payload: nonLetterUpdateSourceCreateResponse,
        },
      },
      {
        name: 'webPresenceUpdateNonLetterUs2',
        query: updateMutation,
        variables: nonLetterUpdateUs2Variables,
        response: {
          status: 200,
          payload: nonLetterUpdateUs2Response,
        },
      },
      {
        name: 'webPresenceUpdateNonLetterEn1',
        query: updateMutation,
        variables: nonLetterUpdateEn1Variables,
        response: {
          status: 200,
          payload: nonLetterUpdateEn1Response,
        },
      },
      {
        name: 'webPresenceUpdateNonLetterUsEast',
        query: updateMutation,
        variables: nonLetterUpdateUsEastVariables,
        response: {
          status: 200,
          payload: nonLetterUpdateUsEastResponse,
        },
      },
      {
        name: 'webPresenceUpdateValidUs',
        query: updateMutation,
        variables: nonLetterUpdateValidUsVariables,
        response: {
          status: 200,
          payload: nonLetterUpdateValidUsResponse,
        },
      },
      {
        name: 'webPresenceCreateDuplicateDefaultLocaleInAlternateLocales',
        query: createMutation,
        variables: duplicateLanguageCreateDefaultVariables,
        response: {
          status: 200,
          payload: duplicateLanguageCreateDefaultResponse,
        },
      },
      {
        name: 'webPresenceCreateDuplicateDefaultInAlternateLocales',
        query: createMutation,
        variables: duplicateLanguageCreateAlternateVariables,
        response: {
          status: 200,
          payload: duplicateLanguageCreateAlternateResponse,
        },
      },
      {
        name: 'webPresenceDuplicateLanguageUpdateDefaultSetupCreate',
        query: createMutation,
        variables: duplicateLanguageUpdateDefaultCreateVariables,
        response: {
          status: 200,
          payload: duplicateLanguageUpdateDefaultCreateResponse,
        },
      },
      {
        name: 'webPresenceUpdateDefaultLocaleDuplicateLanguage',
        query: updateMutation,
        variables: duplicateLanguageUpdateDefaultVariables,
        response: {
          status: 200,
          payload: duplicateLanguageUpdateDefaultResponse,
        },
      },
      {
        name: 'webPresenceDuplicateLanguageUpdateAlternateSetupCreate',
        query: createMutation,
        variables: duplicateLanguageUpdateAlternateCreateVariables,
        response: {
          status: 200,
          payload: duplicateLanguageUpdateAlternateCreateResponse,
        },
      },
      {
        name: 'webPresenceUpdateAlternateLocalesDuplicateLanguage',
        query: updateMutation,
        variables: duplicateLanguageUpdateAlternateVariables,
        response: {
          status: 200,
          payload: duplicateLanguageUpdateAlternateResponse,
        },
      },
    ],
    cleanup: {
      webPresenceDelete: {
        query: deleteMutation,
        variables: { id: createdWebPresenceId },
        response: {
          status: 200,
          payload: cleanupResponse,
        },
      },
      multiLocaleWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: multiLocaleWebPresenceId },
        response: {
          status: 200,
          payload: multiLocaleCleanupResponse,
        },
      },
      frenchCanadianWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: frenchCanadianWebPresenceId },
        response: {
          status: 200,
          payload: frenchCanadianCleanupResponse,
        },
      },
      partialUpdateWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: partialUpdateWebPresenceId },
        response: {
          status: 200,
          payload: partialUpdateCleanupResponse,
        },
      },
      regionalLocaleWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: regionalLocaleWebPresenceId },
        response: {
          status: 200,
          payload: regionalLocaleCleanupResponse,
        },
      },
      caseInsensitiveWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: caseInsensitiveWebPresenceId },
        response: {
          status: 200,
          payload: caseInsensitiveCleanupResponse,
        },
      },
      duplicateCreateWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: duplicateCreateWebPresenceId },
        response: {
          status: 200,
          payload: duplicateCreateCleanupResponse,
        },
      },
      duplicateCreateMarketDelete: {
        query: marketDeleteMutation,
        variables: { id: duplicateCreateMarketId },
        response: {
          status: 200,
          payload: duplicateCreateMarketCleanupResponse,
        },
      },
      duplicateCreateUnexpectedWebPresenceDelete: duplicateCreateUnexpectedWebPresenceId
        ? {
            query: deleteMutation,
            variables: { id: duplicateCreateUnexpectedWebPresenceId },
            response: {
              status: 200,
              payload: duplicateCreateUnexpectedCleanupResponse,
            },
          }
        : null,
      updateCollisionMarketDelete: {
        query: marketDeleteMutation,
        variables: { id: updateCollisionMarketId },
        response: {
          status: 200,
          payload: updateCollisionMarketCleanupResponse,
        },
      },
      updateCollisionSourceWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: updateCollisionSourceWebPresenceId },
        response: {
          status: 200,
          payload: updateCollisionSourceCleanupResponse,
        },
      },
      updateCollisionTakenWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: updateCollisionTakenWebPresenceId },
        response: {
          status: 200,
          payload: updateCollisionTakenCleanupResponse,
        },
      },
      duplicateLanguageCreateDefaultUnexpectedWebPresenceDelete: duplicateLanguageCreateDefaultUnexpectedWebPresenceId
        ? {
            query: deleteMutation,
            variables: { id: duplicateLanguageCreateDefaultUnexpectedWebPresenceId },
            response: {
              status: 200,
              payload: duplicateLanguageCreateDefaultUnexpectedCleanupResponse,
            },
          }
        : null,
      duplicateLanguageCreateAlternateUnexpectedWebPresenceDelete:
        duplicateLanguageCreateAlternateUnexpectedWebPresenceId
          ? {
              query: deleteMutation,
              variables: { id: duplicateLanguageCreateAlternateUnexpectedWebPresenceId },
              response: {
                status: 200,
                payload: duplicateLanguageCreateAlternateUnexpectedCleanupResponse,
              },
            }
          : null,
      duplicateLanguageUpdateDefaultWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: duplicateLanguageUpdateDefaultWebPresenceId },
        response: {
          status: 200,
          payload: duplicateLanguageUpdateDefaultCleanupResponse,
        },
      },
      duplicateLanguageUpdateAlternateWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: duplicateLanguageUpdateAlternateWebPresenceId },
        response: {
          status: 200,
          payload: duplicateLanguageUpdateAlternateCleanupResponse,
        },
      },
      nonLetterUpdateSourceWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: nonLetterUpdateSourceWebPresenceId },
        response: {
          status: 200,
          payload: nonLetterUpdateSourceCleanupResponse,
        },
      },
      nonLetterCreateValidUsWebPresenceDelete: {
        query: deleteMutation,
        variables: { id: nonLetterCreateValidUsWebPresenceId },
        response: {
          status: 200,
          payload: nonLetterCreateValidUsCleanupResponse,
        },
      },
      enabledLocaleCleanup: localeCleanupResponses,
    },
    upstreamCalls: [
      {
        operationName: 'WebPresenceDeletePrimarySetupRead',
        variables: primarySetupVariables,
        query: primaryWebPresenceSetupQuery,
        response: {
          status: 200,
          body: primarySetupRead,
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: createVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: updateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: multiLocaleCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: frenchCanadianCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: partialUpdateCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: partialUpdateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: regionalLocaleCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: caseInsensitiveCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: invalidAlternatesCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: duplicateCreateSourceVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: duplicateCreateTakenVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: updateCollisionSourceCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: updateCollisionTakenCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: updateCollisionVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: nonLetterCreateUs2Variables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: nonLetterCreateEn1Variables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: nonLetterCreateUsEastVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: nonLetterCreateValidUsVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: nonLetterUpdateSourceCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: nonLetterUpdateUs2Variables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: nonLetterUpdateEn1Variables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: nonLetterUpdateUsEastVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: nonLetterUpdateValidUsVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: duplicateLanguageCreateDefaultVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: duplicateLanguageCreateAlternateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: duplicateLanguageUpdateDefaultCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: duplicateLanguageUpdateDefaultVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: duplicateLanguageUpdateAlternateCreateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
      {
        operationName: 'MarketsMutationPreflightHydrate',
        variables: duplicateLanguageUpdateAlternateVariables,
        query: 'hand-synthesized from checked-in capture',
        response: {
          status: 200,
          body: {
            data: {
              webPresences: baselineRead.data?.webPresences,
            },
          },
        },
      },
    ],
  };

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
  await mkdir(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'market-web-presence-lifecycle-parity.json');
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  if (createdWebPresenceId && !cleanupResponse) {
    cleanupResponse = await runGraphql(deleteMutation, { id: createdWebPresenceId });
    console.error(JSON.stringify({ cleanupAfterFailure: cleanupResponse }, null, 2));
  }
  if (multiLocaleWebPresenceId && !multiLocaleCleanupResponse) {
    multiLocaleCleanupResponse = await runGraphql(deleteMutation, { id: multiLocaleWebPresenceId });
    console.error(JSON.stringify({ multiLocaleCleanupAfterFailure: multiLocaleCleanupResponse }, null, 2));
  }
  if (frenchCanadianWebPresenceId && !frenchCanadianCleanupResponse) {
    frenchCanadianCleanupResponse = await runGraphql(deleteMutation, { id: frenchCanadianWebPresenceId });
    console.error(JSON.stringify({ frenchCanadianCleanupAfterFailure: frenchCanadianCleanupResponse }, null, 2));
  }
  if (partialUpdateWebPresenceId && !partialUpdateCleanupResponse) {
    partialUpdateCleanupResponse = await runGraphql(deleteMutation, { id: partialUpdateWebPresenceId });
    console.error(JSON.stringify({ partialUpdateCleanupAfterFailure: partialUpdateCleanupResponse }, null, 2));
  }
  if (regionalLocaleWebPresenceId && !regionalLocaleCleanupResponse) {
    regionalLocaleCleanupResponse = await runGraphql(deleteMutation, { id: regionalLocaleWebPresenceId });
    console.error(JSON.stringify({ regionalLocaleCleanupAfterFailure: regionalLocaleCleanupResponse }, null, 2));
  }
  if (caseInsensitiveWebPresenceId && !caseInsensitiveCleanupResponse) {
    caseInsensitiveCleanupResponse = await runGraphql(deleteMutation, { id: caseInsensitiveWebPresenceId });
    console.error(JSON.stringify({ caseInsensitiveCleanupAfterFailure: caseInsensitiveCleanupResponse }, null, 2));
  }
  if (
    duplicateCreateUnexpectedWebPresenceId &&
    duplicateCreateUnexpectedWebPresenceId !== duplicateCreateWebPresenceId &&
    !duplicateCreateUnexpectedCleanupResponse
  ) {
    duplicateCreateUnexpectedCleanupResponse = await runGraphql(deleteMutation, {
      id: duplicateCreateUnexpectedWebPresenceId,
    });
    console.error(
      JSON.stringify(
        { duplicateCreateUnexpectedCleanupAfterFailure: duplicateCreateUnexpectedCleanupResponse },
        null,
        2,
      ),
    );
  }
  if (duplicateCreateWebPresenceId && !duplicateCreateCleanupResponse) {
    duplicateCreateCleanupResponse = await runGraphql(deleteMutation, { id: duplicateCreateWebPresenceId });
    console.error(JSON.stringify({ duplicateCreateCleanupAfterFailure: duplicateCreateCleanupResponse }, null, 2));
  }
  if (duplicateCreateMarketId && !duplicateCreateMarketCleanupResponse) {
    duplicateCreateMarketCleanupResponse = await runGraphql(marketDeleteMutation, { id: duplicateCreateMarketId });
    console.error(
      JSON.stringify({ duplicateCreateMarketCleanupAfterFailure: duplicateCreateMarketCleanupResponse }, null, 2),
    );
  }
  if (updateCollisionSourceWebPresenceId && !updateCollisionSourceCleanupResponse) {
    updateCollisionSourceCleanupResponse = await runGraphql(deleteMutation, {
      id: updateCollisionSourceWebPresenceId,
    });
    console.error(
      JSON.stringify({ updateCollisionSourceCleanupAfterFailure: updateCollisionSourceCleanupResponse }, null, 2),
    );
  }
  if (updateCollisionTakenWebPresenceId && !updateCollisionTakenCleanupResponse) {
    updateCollisionTakenCleanupResponse = await runGraphql(deleteMutation, {
      id: updateCollisionTakenWebPresenceId,
    });
    console.error(
      JSON.stringify({ updateCollisionTakenCleanupAfterFailure: updateCollisionTakenCleanupResponse }, null, 2),
    );
  }
  if (updateCollisionMarketId && !updateCollisionMarketCleanupResponse) {
    updateCollisionMarketCleanupResponse = await runGraphql(marketDeleteMutation, { id: updateCollisionMarketId });
    console.error(
      JSON.stringify({ updateCollisionMarketCleanupAfterFailure: updateCollisionMarketCleanupResponse }, null, 2),
    );
  }
  if (
    duplicateLanguageCreateDefaultUnexpectedWebPresenceId &&
    !duplicateLanguageCreateDefaultUnexpectedCleanupResponse
  ) {
    duplicateLanguageCreateDefaultUnexpectedCleanupResponse = await runGraphql(deleteMutation, {
      id: duplicateLanguageCreateDefaultUnexpectedWebPresenceId,
    });
    console.error(
      JSON.stringify(
        {
          duplicateLanguageCreateDefaultUnexpectedCleanupAfterFailure:
            duplicateLanguageCreateDefaultUnexpectedCleanupResponse,
        },
        null,
        2,
      ),
    );
  }
  if (
    duplicateLanguageCreateAlternateUnexpectedWebPresenceId &&
    !duplicateLanguageCreateAlternateUnexpectedCleanupResponse
  ) {
    duplicateLanguageCreateAlternateUnexpectedCleanupResponse = await runGraphql(deleteMutation, {
      id: duplicateLanguageCreateAlternateUnexpectedWebPresenceId,
    });
    console.error(
      JSON.stringify(
        {
          duplicateLanguageCreateAlternateUnexpectedCleanupAfterFailure:
            duplicateLanguageCreateAlternateUnexpectedCleanupResponse,
        },
        null,
        2,
      ),
    );
  }
  if (duplicateLanguageUpdateDefaultWebPresenceId && !duplicateLanguageUpdateDefaultCleanupResponse) {
    duplicateLanguageUpdateDefaultCleanupResponse = await runGraphql(deleteMutation, {
      id: duplicateLanguageUpdateDefaultWebPresenceId,
    });
    console.error(
      JSON.stringify(
        { duplicateLanguageUpdateDefaultCleanupAfterFailure: duplicateLanguageUpdateDefaultCleanupResponse },
        null,
        2,
      ),
    );
  }
  if (duplicateLanguageUpdateAlternateWebPresenceId && !duplicateLanguageUpdateAlternateCleanupResponse) {
    duplicateLanguageUpdateAlternateCleanupResponse = await runGraphql(deleteMutation, {
      id: duplicateLanguageUpdateAlternateWebPresenceId,
    });
    console.error(
      JSON.stringify(
        { duplicateLanguageUpdateAlternateCleanupAfterFailure: duplicateLanguageUpdateAlternateCleanupResponse },
        null,
        2,
      ),
    );
  }
  if (localeRestoreActions.length > 0 && Object.keys(localeCleanupResponses).length === 0) {
    localeCleanupResponses = await restoreEnabledLocales();
    console.error(JSON.stringify({ localeCleanupAfterFailure: localeCleanupResponses }, null, 2));
  }
  throw error;
}
