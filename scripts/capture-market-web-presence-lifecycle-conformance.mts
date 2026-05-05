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
let createdWebPresenceId: string | null = null;
let multiLocaleWebPresenceId: string | null = null;
let cleanupResponse: unknown = null;
let multiLocaleCleanupResponse: unknown = null;
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
    if (userErrors.length > 0) {
      throw new Error(`locale cleanup failed for ${action.locale}: ${JSON.stringify(userErrors)}`);
    }
    responses[action.locale] = payload;
  }
  return responses;
}

try {
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
  localeCleanupResponses = await restoreEnabledLocales();

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    disposableSubfolderSuffixes: {
      created: createSuffix,
      updated: updateSuffix,
      multiLocale: multiLocaleSuffix,
    },
    scope:
      'HAR-448 market web presence create/update/delete lifecycle parity plus HAR-613 multi-locale rootUrls parity',
    data: {
      webPresences: baselineRead.data?.webPresences,
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
      enabledLocaleCleanup: localeCleanupResponses,
    },
    upstreamCalls: [
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
  if (localeRestoreActions.length > 0 && Object.keys(localeCleanupResponses).length === 0) {
    localeCleanupResponses = await restoreEnabledLocales();
    console.error(JSON.stringify({ localeCleanupAfterFailure: localeCleanupResponses }, null, 2));
  }
  throw error;
}
