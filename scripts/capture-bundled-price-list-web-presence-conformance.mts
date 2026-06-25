/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = {
  field?: string[] | null;
  message?: string;
  code?: string | null;
};

type WebPresencesReadData = {
  webPresences?: {
    nodes?: Array<Record<string, unknown> | null> | null;
  } | null;
};

type BundledCreateData = {
  priceListCreate?: {
    priceList?: { id?: string | null } | null;
    userErrors?: UserError[];
  } | null;
  webPresenceCreate?: {
    webPresence?: { id?: string | null } | null;
    userErrors?: UserError[];
  } | null;
};

type PriceListDeleteData = {
  priceListDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  } | null;
};

type WebPresenceDeleteData = {
  webPresenceDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  } | null;
};

type CapturedCase<TData> = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<TData>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'bundled-price-list-web-presence.json');
const bundledDocumentPath = path.join(
  'config',
  'parity-requests',
  'markets',
  'bundled-price-list-web-presence-create.graphql',
);
const bundledMutation = await readFile(bundledDocumentPath, 'utf8');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const webPresenceFields = `#graphql
fragment BundledPriceListWebPresenceFields on MarketWebPresence {
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
}
`;

const webPresencesReadQuery = `#graphql
${webPresenceFields}
query BundledPriceListWebPresenceBaselineRead($first: Int!) {
  webPresences(first: $first) {
    nodes {
      ...BundledPriceListWebPresenceFields
    }
  }
}
`;

const webPresenceDeleteMutation = `#graphql
mutation BundledPriceListWebPresenceCleanupDelete($id: ID!) {
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

const priceListDeleteMutation = `#graphql
mutation BundledPriceListCleanupDelete($id: ID!) {
  priceListDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

function randomLetters(length: number): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz';
  return Array.from({ length }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join('');
}

function userErrors<TData>(result: ConformanceGraphqlResult<TData>, root: keyof TData): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = data[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const maybeErrors = (payload as { userErrors?: UserError[] }).userErrors;
  return Array.isArray(maybeErrors) ? maybeErrors : [];
}

function assertNoUserErrors<TData>(
  result: ConformanceGraphqlResult<TData>,
  roots: Array<keyof TData>,
  label: string,
): void {
  const errors = roots.flatMap((root) => userErrors(result, root));
  if (result.status !== 200 || result.payload.errors || errors.length > 0) {
    throw new Error(
      `${label} failed: status=${result.status} userErrors=${JSON.stringify(errors)} errors=${JSON.stringify(
        result.payload.errors ?? null,
      )}`,
    );
  }
}

async function captureCase<TData>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase<TData>> {
  return {
    name,
    query,
    variables,
    response: await runGraphqlRequest<TData>(query, variables),
  };
}

function readCreatedIds(result: ConformanceGraphqlResult<BundledCreateData>): {
  priceListId: string;
  webPresenceId: string;
} {
  const priceListId = result.payload.data?.priceListCreate?.priceList?.id;
  const webPresenceId = result.payload.data?.webPresenceCreate?.webPresence?.id;
  if (typeof priceListId !== 'string' || typeof webPresenceId !== 'string') {
    throw new Error(`bundled create did not return expected ids: ${JSON.stringify(result.payload)}`);
  }
  return { priceListId, webPresenceId };
}

const unique = Date.now().toString(36);
const bundledVariables = {
  priceListInput: {
    name: `Bundled web presence DKK ${unique}`,
    currency: 'DKK',
    parent: {
      adjustment: {
        type: 'PERCENTAGE_DECREASE',
        value: 10,
      },
    },
  },
  webPresenceInput: {
    defaultLocale: 'en',
    alternateLocales: [],
    subfolderSuffix: `har${randomLetters(10)}`,
  },
};
const baselineReadVariables = { first: 20 };
const cases: Array<CapturedCase<unknown>> = [];
const cleanup: Array<{
  type: 'webPresence' | 'priceList';
  id: string;
  response: ConformanceGraphqlResult<unknown>;
}> = [];
let createdWebPresenceId: string | null = null;
let createdPriceListId: string | null = null;

const baselineRead = await captureCase<WebPresencesReadData>(
  'webPresences baseline for bundled preflight',
  webPresencesReadQuery,
  baselineReadVariables,
);
if (baselineRead.response.status !== 200 || baselineRead.response.payload.errors) {
  throw new Error(`webPresences baseline read failed: ${JSON.stringify(baselineRead.response.payload)}`);
}
cases.push(baselineRead);

try {
  const bundledCreate = await captureCase<BundledCreateData>(
    'bundled priceListCreate and webPresenceCreate',
    bundledMutation,
    bundledVariables,
  );
  assertNoUserErrors(bundledCreate.response, ['priceListCreate', 'webPresenceCreate'], 'bundled create');
  const ids = readCreatedIds(bundledCreate.response);
  createdPriceListId = ids.priceListId;
  createdWebPresenceId = ids.webPresenceId;
  cases.push(bundledCreate);
} finally {
  if (createdWebPresenceId) {
    cleanup.push({
      type: 'webPresence',
      id: createdWebPresenceId,
      response: await runGraphqlRequest<WebPresenceDeleteData>(webPresenceDeleteMutation, { id: createdWebPresenceId }),
    });
  }
  if (createdPriceListId) {
    cleanup.push({
      type: 'priceList',
      id: createdPriceListId,
      response: await runGraphqlRequest<PriceListDeleteData>(priceListDeleteMutation, { id: createdPriceListId }),
    });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      scope: 'bundled price-list and web-presence local-dispatch parity',
      cases,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'MarketsMutationPreflightHydrate',
          variables: bundledVariables,
          query: 'hand-synthesized from checked-in capture',
          response: {
            status: baselineRead.response.status,
            body: {
              data: {
                webPresences: baselineRead.response.payload.data?.webPresences ?? null,
              },
            },
          },
        },
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain,
      apiVersion,
      createdPriceListId,
      createdWebPresenceId,
      cleanup: cleanup.map((entry) => ({ type: entry.type, id: entry.id, status: entry.response.status })),
    },
    null,
    2,
  ),
);
