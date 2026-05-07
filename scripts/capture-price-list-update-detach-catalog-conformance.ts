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

type CapturedCase<TData = unknown> = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<TData>;
};

type MarketsReadData = {
  markets?: {
    nodes?: Array<{ id?: string } | null> | null;
  } | null;
};

type PriceListCreateData = {
  priceListCreate?: {
    priceList?: { id?: string } | null;
    userErrors?: UserError[];
  } | null;
};

type PriceListUpdateData = {
  priceListUpdate?: {
    priceList?: { id?: string; catalog?: { id?: string } | null } | null;
    userErrors?: UserError[];
  } | null;
};

type CatalogCreateData = {
  catalogCreate?: {
    catalog?: { id?: string } | null;
    userErrors?: UserError[];
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'price-list-update-detach-catalog.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketsReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-relation-markets-read.graphql'),
  'utf8',
);
const priceListCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-create-catalog-validation.graphql'),
  'utf8',
);
const catalogCreateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'catalog-create-relation-validation.graphql'),
  'utf8',
);
const priceListUpdateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-update-input-validation.graphql'),
  'utf8',
);
const detachReadDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-update-detach-catalog-read.graphql'),
  'utf8',
);

const catalogDeleteDocument = `#graphql
mutation PriceListUpdateDetachCatalogCleanup($id: ID!) {
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

const priceListDeleteDocument = `#graphql
mutation PriceListUpdateDetachPriceListCleanup($id: ID!) {
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

function userErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = (data as Record<string, unknown>)[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const maybeErrors = (payload as { userErrors?: UserError[] }).userErrors;
  return Array.isArray(maybeErrors) ? maybeErrors : [];
}

function assertNoGraphqlErrors<TData>(result: ConformanceGraphqlResult<TData>, label: string): void {
  if (result.status !== 200 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string, label: string): void {
  assertNoGraphqlErrors(result, label);
  const errors = userErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertInvalidCatalogIdVariable<TData>(result: ConformanceGraphqlResult<TData>, label: string): void {
  const errors = result.payload.errors;
  const first = Array.isArray(errors) ? errors[0] : undefined;
  const extensions =
    typeof first === 'object' && first !== null
      ? (first as { extensions?: { code?: string; problems?: Array<{ path?: string[] }> } }).extensions
      : undefined;
  const problemPath = extensions?.problems?.[0]?.path;
  if (
    result.status !== 200 ||
    !first ||
    extensions?.code !== 'INVALID_VARIABLE' ||
    JSON.stringify(problemPath ?? null) !== JSON.stringify(['catalogId'])
  ) {
    throw new Error(`${label} did not return INVALID_VARIABLE for catalogId: ${JSON.stringify(result.payload)}`);
  }
}

function firstMarketId(result: ConformanceGraphqlResult<MarketsReadData>): string {
  const id = result.payload.data?.markets?.nodes?.[0]?.id;
  if (typeof id !== 'string') {
    throw new Error(`markets(first: 1) did not return a market id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function priceListId(result: ConformanceGraphqlResult<PriceListCreateData>): string {
  const id = result.payload.data?.priceListCreate?.priceList?.id;
  if (typeof id !== 'string') {
    throw new Error(`priceListCreate did not return an id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function catalogId(result: ConformanceGraphqlResult<CatalogCreateData>): string {
  const id = result.payload.data?.catalogCreate?.catalog?.id;
  if (typeof id !== 'string') {
    throw new Error(`catalogCreate did not return an id: ${JSON.stringify(result.payload)}`);
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

function priceListCreateVariables(label: string): Record<string, unknown> {
  return {
    input: {
      name: label,
      currency: 'DKK',
      parent: {
        adjustment: {
          type: 'PERCENTAGE_DECREASE',
          value: 10,
        },
      },
    },
  };
}

function catalogCreateVariables(title: string, marketId: string, priceListId?: string): Record<string, unknown> {
  return {
    input: {
      title,
      status: 'ACTIVE',
      context: {
        marketIds: [marketId],
      },
      ...(priceListId === undefined ? {} : { priceListId }),
    },
  };
}

function assertDetached<TData extends PriceListUpdateData>(
  result: ConformanceGraphqlResult<TData>,
  label: string,
): void {
  assertNoUserErrors(result, 'priceListUpdate', label);
  const catalog = result.payload.data?.priceListUpdate?.priceList?.catalog;
  if (catalog !== null) {
    throw new Error(`${label} did not detach catalog: ${JSON.stringify(result.payload)}`);
  }
}

const unique = Date.now().toString(36);
const createdCatalogIds: string[] = [];
const createdPriceListIds: string[] = [];
const cases: Array<CapturedCase<unknown>> = [];
const cleanup: Array<{
  type: 'catalog' | 'priceList';
  id: string;
  response: ConformanceGraphqlResult<unknown>;
}> = [];

const marketsReadVariables = { first: 1 };
const marketsRead = await captureCase<MarketsReadData>(
  'markets read for price-list update detach catalog',
  marketsReadDocument,
  marketsReadVariables,
);
assertNoGraphqlErrors(marketsRead.response, 'markets read');
const marketId = firstMarketId(marketsRead.response);
cases.push(marketsRead);

try {
  const priceListSetup = await captureCase<PriceListCreateData>(
    'priceListCreate for detach catalog setup',
    priceListCreateDocument,
    priceListCreateVariables(`Price list detach ${unique}`),
  );
  assertNoUserErrors(priceListSetup.response, 'priceListCreate', 'priceListCreate setup');
  const detachedPriceListId = priceListId(priceListSetup.response);
  createdPriceListIds.push(detachedPriceListId);
  cases.push(priceListSetup);

  const catalogSetup = await captureCase<CatalogCreateData>(
    'catalogCreate initial price-list attachment',
    catalogCreateDocument,
    catalogCreateVariables(`Catalog detach initial ${unique}`, marketId, detachedPriceListId),
  );
  assertNoUserErrors(catalogSetup.response, 'catalogCreate', 'catalogCreate initial attachment');
  const initialCatalogId = catalogId(catalogSetup.response);
  createdCatalogIds.push(initialCatalogId);
  cases.push(catalogSetup);

  const emptyCatalogId = await captureCase<PriceListUpdateData>(
    'priceListUpdate empty catalogId validation',
    priceListUpdateDocument,
    {
      id: detachedPriceListId,
      input: {
        catalogId: '',
      },
    },
  );
  assertInvalidCatalogIdVariable(emptyCatalogId.response, 'priceListUpdate empty catalogId validation');
  cases.push(emptyCatalogId);

  const detach = await captureCase<PriceListUpdateData>(
    'priceListUpdate explicit null catalogId detaches',
    priceListUpdateDocument,
    {
      id: detachedPriceListId,
      input: {
        catalogId: null,
      },
    },
  );
  assertDetached(detach.response, 'priceListUpdate detach');
  cases.push(detach);

  const detachRead = await captureCase('price list and catalog read after detach', detachReadDocument, {
    catalogId: initialCatalogId,
    priceListId: detachedPriceListId,
  });
  assertNoGraphqlErrors(detachRead.response, 'detach read');
  cases.push(detachRead);

  const reattach = await captureCase<CatalogCreateData>(
    'catalogCreate can claim detached price list',
    catalogCreateDocument,
    catalogCreateVariables(`Catalog detach reattach ${unique}`, marketId, detachedPriceListId),
  );
  assertNoUserErrors(reattach.response, 'catalogCreate', 'catalogCreate claim detached price list');
  createdCatalogIds.push(catalogId(reattach.response));
  cases.push(reattach);
} finally {
  for (const id of createdCatalogIds.slice().reverse()) {
    cleanup.push({
      type: 'catalog',
      id,
      response: await runGraphqlRequest(catalogDeleteDocument, { id }),
    });
  }
  for (const id of createdPriceListIds.slice().reverse()) {
    cleanup.push({
      type: 'priceList',
      id,
      response: await runGraphqlRequest(priceListDeleteDocument, { id }),
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
      scope: 'priceListUpdate explicit null catalog detach and empty catalogId validation',
      cases,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'CatalogRelationMarketsRead',
          variables: marketsReadVariables,
          query: marketsReadDocument,
          response: {
            status: marketsRead.response.status,
            body: marketsRead.response.payload,
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
      marketId,
      cases: cases.map((entry) => ({
        name: entry.name,
        status: entry.response.status,
      })),
      cleanup: cleanup.map((entry) => ({
        type: entry.type,
        id: entry.id,
        status: entry.response.status,
      })),
    },
    null,
    2,
  ),
);
