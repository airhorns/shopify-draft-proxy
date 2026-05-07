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
let createdWebPresenceId: string | null = null;
let multiLocaleWebPresenceId: string | null = null;
let frenchCanadianWebPresenceId: string | null = null;
let partialUpdateWebPresenceId: string | null = null;
let regionalLocaleWebPresenceId: string | null = null;
let caseInsensitiveWebPresenceId: string | null = null;
let cleanupResponse: unknown = null;
let multiLocaleCleanupResponse: unknown = null;
let frenchCanadianCleanupResponse: unknown = null;
let partialUpdateCleanupResponse: unknown = null;
let regionalLocaleCleanupResponse: unknown = null;
let caseInsensitiveCleanupResponse: unknown = null;
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
    },
    scope:
      'HAR-448 market web presence create/update/delete lifecycle parity plus HAR-613 multi-locale rootUrls parity, HAR-611 fr-CA default locale parity, and web-presence locale catalog/error-shape parity',
    data: {
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
  if (localeRestoreActions.length > 0 && Object.keys(localeCleanupResponses).length === 0) {
    localeCleanupResponses = await restoreEnabledLocales();
    console.error(JSON.stringify({ localeCleanupAfterFailure: localeCleanupResponses }, null, 2));
  }
  throw error;
}
