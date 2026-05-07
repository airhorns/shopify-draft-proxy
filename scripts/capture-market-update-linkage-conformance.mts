/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = {
  field?: string[] | null;
  message?: string;
  code?: string | null;
};

type CapturedCase<TData> = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<TData>;
};

type MarketCreateData = {
  marketCreate?: {
    market?: { id?: string | null } | null;
    userErrors?: UserError[];
  } | null;
};

type CatalogCreateData = {
  catalogCreate?: {
    catalog?: { id?: string | null } | null;
    userErrors?: UserError[];
  } | null;
};

type MarketUpdateData = {
  marketUpdate?: {
    market?: { id?: string | null } | null;
    userErrors?: UserError[];
  } | null;
};

type CatalogDeleteData = {
  catalogDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  } | null;
};

type MarketDeleteData = {
  marketDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'market-update-linkage.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketCreateMutation = `#graphql
mutation MarketUpdateLinkageMarketCreate($input: MarketCreateInput!) {
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

const catalogCreateMutation = `#graphql
mutation MarketUpdateLinkageCatalogCreate($input: CatalogCreateInput!) {
  catalogCreate(input: $input) {
    catalog {
      id
      ... on MarketCatalog {
        markets(first: 5) {
          nodes {
            id
          }
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

const marketUpdateMutation = `#graphql
mutation MarketUpdateLinkageUpdate($id: ID!, $input: MarketUpdateInput!) {
  marketUpdate(id: $id, input: $input) {
    market {
      id
      catalogs(first: 5) {
        nodes {
          id
          ... on MarketCatalog {
            markets(first: 5) {
              nodes {
                id
              }
            }
          }
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

const marketReadQuery = `#graphql
query MarketUpdateLinkageMarketRead($id: ID!) {
  market(id: $id) {
    id
    catalogs(first: 5) {
      nodes {
        id
        ... on MarketCatalog {
          markets(first: 5) {
            nodes {
              id
            }
          }
        }
      }
    }
  }
}
`;

const catalogReadQuery = `#graphql
query MarketUpdateLinkageCatalogRead($id: ID!) {
  catalog(id: $id) {
    id
    ... on MarketCatalog {
      markets(first: 5) {
        nodes {
          id
        }
      }
    }
  }
}
`;

const marketDeleteMutation = `#graphql
mutation MarketUpdateLinkageMarketDelete($id: ID!) {
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

const catalogDeleteMutation = `#graphql
mutation MarketUpdateLinkageCatalogDelete($id: ID!) {
  catalogDelete(id: $id) {
    deletedId
    userErrors {
      field
      message
      code
    }
  }
}
`;

function userErrors<TData>(result: ConformanceGraphqlResult<TData>, root: keyof TData): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = data[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const maybeErrors = (payload as { userErrors?: UserError[] }).userErrors;
  return Array.isArray(maybeErrors) ? maybeErrors : [];
}

function assertNoUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: keyof TData, label: string): void {
  const errors = userErrors(result, root);
  if (result.status !== 200 || result.payload.errors || errors.length > 0) {
    throw new Error(
      `${label} failed: status=${result.status} userErrors=${JSON.stringify(errors)} errors=${JSON.stringify(
        result.payload.errors ?? null,
      )}`,
    );
  }
}

function assertExpectedUserError<TData>(
  result: ConformanceGraphqlResult<TData>,
  root: keyof TData,
  expectedCode: string,
  expectedField: string[],
  label: string,
): void {
  const errors = userErrors(result, root);
  const hasExpectedError = errors.some(
    (error) => error.code === expectedCode && JSON.stringify(error.field ?? null) === JSON.stringify(expectedField),
  );
  if (result.status !== 200 || result.payload.errors || !hasExpectedError) {
    throw new Error(
      `${label} did not return expected userError ${expectedCode}: status=${result.status} userErrors=${JSON.stringify(
        errors,
      )} errors=${JSON.stringify(result.payload.errors ?? null)}`,
    );
  }
}

function marketId(result: ConformanceGraphqlResult<MarketCreateData>, label: string): string {
  const id = result.payload.data?.marketCreate?.market?.id;
  if (typeof id !== 'string') {
    throw new Error(`${label} did not return a market id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function catalogId(result: ConformanceGraphqlResult<CatalogCreateData>): string {
  const id = result.payload.data?.catalogCreate?.catalog?.id;
  if (typeof id !== 'string') {
    throw new Error(`catalogCreate did not return a catalog id: ${JSON.stringify(result.payload)}`);
  }
  return id;
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

const unique = Date.now().toString(36);
const cases: Array<CapturedCase<unknown>> = [];
const cleanup: Array<{
  type: 'market' | 'catalog' | 'marketLink';
  id: string;
  response: ConformanceGraphqlResult<unknown>;
}> = [];
const createdMarketIds: string[] = [];
let createdCatalogId: string | null = null;
let targetMarketId: string | null = null;

try {
  const targetMarketCreate = await captureCase<MarketCreateData>('target market create', marketCreateMutation, {
    input: { name: `Draft Proxy Link Target ${unique}` },
  });
  assertNoUserErrors(targetMarketCreate.response, 'marketCreate', 'target market create');
  targetMarketId = marketId(targetMarketCreate.response, 'target market create');
  createdMarketIds.push(targetMarketId);
  cases.push(targetMarketCreate);

  const sourceMarketCreate = await captureCase<MarketCreateData>('source market create', marketCreateMutation, {
    input: { name: `Draft Proxy Link Source ${unique}` },
  });
  assertNoUserErrors(sourceMarketCreate.response, 'marketCreate', 'source market create');
  const sourceMarketId = marketId(sourceMarketCreate.response, 'source market create');
  createdMarketIds.push(sourceMarketId);
  cases.push(sourceMarketCreate);

  const catalogCreate = await captureCase<CatalogCreateData>(
    'catalog create with source market',
    catalogCreateMutation,
    {
      input: {
        title: `Draft Proxy Link Catalog ${unique}`,
        status: 'ACTIVE',
        context: { marketIds: [sourceMarketId] },
      },
    },
  );
  assertNoUserErrors(catalogCreate.response, 'catalogCreate', 'catalog create with source market');
  createdCatalogId = catalogId(catalogCreate.response);
  cases.push(catalogCreate);

  const addCatalog = await captureCase<MarketUpdateData>('marketUpdate catalogsToAdd', marketUpdateMutation, {
    id: targetMarketId,
    input: { catalogsToAdd: [createdCatalogId] },
  });
  assertNoUserErrors(addCatalog.response, 'marketUpdate', 'marketUpdate catalogsToAdd');
  cases.push(addCatalog);

  cases.push(await captureCase('market read after catalogsToAdd', marketReadQuery, { id: targetMarketId }));
  cases.push(await captureCase('catalog read after catalogsToAdd', catalogReadQuery, { id: createdCatalogId }));

  const unknownCatalogAdd = await captureCase<MarketUpdateData>(
    'marketUpdate unknown catalogsToAdd validation',
    marketUpdateMutation,
    {
      id: targetMarketId,
      input: { catalogsToAdd: ['gid://shopify/MarketCatalog/9999999999'] },
    },
  );
  assertExpectedUserError(
    unknownCatalogAdd.response,
    'marketUpdate',
    'CUSTOMIZATIONS_NOT_FOUND',
    ['input', 'catalogsToAdd'],
    'marketUpdate unknown catalogsToAdd validation',
  );
  cases.push(unknownCatalogAdd);

  const unknownWebPresenceAdd = await captureCase<MarketUpdateData>(
    'marketUpdate unknown webPresencesToAdd validation',
    marketUpdateMutation,
    {
      id: targetMarketId,
      input: { webPresencesToAdd: ['gid://shopify/MarketWebPresence/9999999999'] },
    },
  );
  assertExpectedUserError(
    unknownWebPresenceAdd.response,
    'marketUpdate',
    'CUSTOMIZATIONS_NOT_FOUND',
    ['input', 'webPresencesToAdd'],
    'marketUpdate unknown webPresencesToAdd validation',
  );
  cases.push(unknownWebPresenceAdd);
} finally {
  if (targetMarketId && createdCatalogId) {
    cleanup.push({
      type: 'marketLink',
      id: targetMarketId,
      response: await runGraphqlRequest<MarketUpdateData>(marketUpdateMutation, {
        id: targetMarketId,
        input: { catalogsToDelete: [createdCatalogId] },
      }),
    });
  }
  if (createdCatalogId) {
    cleanup.push({
      type: 'catalog',
      id: createdCatalogId,
      response: await runGraphqlRequest<CatalogDeleteData>(catalogDeleteMutation, { id: createdCatalogId }),
    });
  }
  for (const id of createdMarketIds.toReversed()) {
    cleanup.push({
      type: 'market',
      id,
      response: await runGraphqlRequest<MarketDeleteData>(marketDeleteMutation, { id }),
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
      scope: 'marketUpdate linkage add/delete validation',
      cases,
      cleanup,
      upstreamCalls: [],
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
      cases: cases.map((capture) => ({ name: capture.name, status: capture.response.status })),
      cleanup: cleanup.map((entry) => ({ type: entry.type, id: entry.id, status: entry.response.status })),
    },
    null,
    2,
  ),
);
