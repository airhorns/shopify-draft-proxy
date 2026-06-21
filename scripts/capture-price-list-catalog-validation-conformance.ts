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

type MutationPayload = {
  userErrors?: UserError[];
  priceList?: { id?: string; catalog?: { id?: string } | null } | null;
  catalog?: { id?: string } | null;
};

type CapturedCase<TData = unknown> = {
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
const outputPath = path.join(outputDir, 'price-list-catalog-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketsReadQuery = `#graphql
query PriceListCatalogValidationMarketsRead($first: Int!) {
  markets(first: $first) {
    nodes {
      id
      name
      currencySettings {
        baseCurrency {
          currencyCode
        }
      }
    }
  }
}
`;

const priceListCreateMutation = `#graphql
mutation PriceListCreateCatalogValidation($input: PriceListCreateInput!) {
  priceListCreate(input: $input) {
    priceList {
      id
      currency
      parent {
        adjustment {
          type
          value
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

const priceListUpdateMutation = `#graphql
mutation PriceListUpdateCatalogValidation($id: ID!, $input: PriceListUpdateInput!) {
  priceListUpdate(id: $id, input: $input) {
    priceList {
      id
      currency
      catalog {
        id
      }
      parent {
        adjustment {
          type
          value
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

const catalogCreateMutation = `#graphql
mutation CatalogCreateRelationValidation($input: CatalogCreateInput!) {
  catalogCreate(input: $input) {
    catalog {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const catalogDeleteMutation = `#graphql
mutation CatalogRelationCatalogDelete($id: ID!) {
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

const priceListDeleteMutation = `#graphql
mutation PriceListCatalogValidationCleanup($id: ID!) {
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

function payloadUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = (data as Record<string, unknown>)[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const maybeErrors = (payload as MutationPayload).userErrors;
  return Array.isArray(maybeErrors) ? maybeErrors : [];
}

function mutationPayload<TData>(result: ConformanceGraphqlResult<TData>, root: string): MutationPayload {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return {};
  const payload = (data as Record<string, unknown>)[root];
  return typeof payload === 'object' && payload !== null ? (payload as MutationPayload) : {};
}

function assertNoGraphqlErrors<TData>(result: ConformanceGraphqlResult<TData>, label: string): void {
  if (result.status !== 200 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors<TData>(result: ConformanceGraphqlResult<TData>, root: string, label: string): void {
  assertNoGraphqlErrors(result, label);
  const errors = payloadUserErrors(result, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertUserError<TData>(
  result: ConformanceGraphqlResult<TData>,
  root: string,
  expectedCode: string,
  expectedField: string[],
  expectedMessage: string,
  label: string,
): void {
  assertNoGraphqlErrors(result, label);
  const errors = payloadUserErrors(result, root);
  const matched = errors.some(
    (error) =>
      error.code === expectedCode &&
      error.message === expectedMessage &&
      JSON.stringify(error.field ?? null) === JSON.stringify(expectedField),
  );
  if (!matched) {
    throw new Error(`${label} missing ${expectedCode}: ${JSON.stringify(errors)}`);
  }
}

function assertResourceNotFound<TData>(
  result: ConformanceGraphqlResult<TData>,
  root: string,
  id: string,
  label: string,
): void {
  if (result.status !== 200) {
    throw new Error(`${label} returned HTTP ${result.status}: ${JSON.stringify(result.payload)}`);
  }
  const errors = result.payload.errors;
  const matched =
    Array.isArray(errors) &&
    errors.some((error) => {
      const entry = error as {
        message?: unknown;
        path?: unknown;
        extensions?: { code?: unknown };
      };
      return (
        entry.message === `Invalid id: ${id}` &&
        JSON.stringify(entry.path ?? null) === JSON.stringify([root]) &&
        entry.extensions?.code === 'RESOURCE_NOT_FOUND'
      );
    });
  const data = result.payload.data;
  const rootValue = typeof data === 'object' && data !== null ? (data as Record<string, unknown>)[root] : undefined;
  if (!matched || rootValue !== null) {
    throw new Error(`${label} missing RESOURCE_NOT_FOUND invalid-id envelope: ${JSON.stringify(result.payload)}`);
  }
}

function assertPriceListNull<TData>(result: ConformanceGraphqlResult<TData>, root: string, label: string): void {
  if (mutationPayload(result, root).priceList !== null) {
    throw new Error(`${label} should return priceList null: ${JSON.stringify(result.payload)}`);
  }
}

function priceListId<TData>(result: ConformanceGraphqlResult<TData>, root: string): string {
  const id = mutationPayload(result, root).priceList?.id;
  if (typeof id !== 'string') {
    throw new Error(`${root} did not return a price list id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function catalogId<TData>(result: ConformanceGraphqlResult<TData>, root: string): string {
  const id = mutationPayload(result, root).catalog?.id;
  if (typeof id !== 'string') {
    throw new Error(`${root} did not return a catalog id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function firstMarket(result: ConformanceGraphqlResult<unknown>): { id: string; currency: string } {
  const nodes =
    (result.payload.data as { markets?: { nodes?: Array<Record<string, unknown>> } } | undefined)?.markets?.nodes ?? [];
  const market = nodes.find((node) => typeof node['id'] === 'string' && typeof marketCurrency(node) === 'string');
  const currency = market ? marketCurrency(market) : undefined;
  if (!market || typeof market['id'] !== 'string' || typeof currency !== 'string') {
    throw new Error(`No market with base currency was available: ${JSON.stringify(result.payload)}`);
  }
  return { id: market['id'], currency };
}

function marketCurrency(market: Record<string, unknown>): string | undefined {
  const currencySettings = market['currencySettings'] as { baseCurrency?: { currencyCode?: string } } | undefined;
  return currencySettings?.baseCurrency?.currencyCode;
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

function priceListInput(name: string, currency: string, extra: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    input: {
      name,
      currency,
      parent: {
        adjustment: {
          type: 'PERCENTAGE_DECREASE',
          value: 10,
        },
      },
      ...extra,
    },
  };
}

function catalogInput(title: string, marketId: string, extra: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    input: {
      title,
      status: 'ACTIVE',
      context: {
        marketIds: [marketId],
      },
      ...extra,
    },
  };
}

const unique = Date.now().toString(36);
const missingCatalogId = 'gid://shopify/MarketCatalog/99999999';
const wrongTypeCatalogId = 'gid://shopify/CatalogMarket/99999999';
const catalogField = ['input', 'catalogId'];
const createdPriceListIds: string[] = [];
const createdCatalogIds: string[] = [];
const cleanup: Array<{
  type: 'priceList' | 'catalog';
  id: string;
  response: ConformanceGraphqlResult<unknown>;
}> = [];
const cases: Array<CapturedCase<unknown>> = [];

const marketsReadVariables = { first: 10 };
const marketsRead = await captureCase(
  'markets read for price-list catalog validation',
  marketsReadQuery,
  marketsReadVariables,
);
assertNoGraphqlErrors(marketsRead.response, 'markets read');
cases.push(marketsRead);
const market = firstMarket(marketsRead.response);

try {
  const createMissingCatalog = await captureCase(
    'priceListCreate nonexistent catalogId validation',
    priceListCreateMutation,
    priceListInput(`Price list missing catalog ${unique}`, 'USD', { catalogId: missingCatalogId }),
  );
  assertUserError(
    createMissingCatalog.response,
    'priceListCreate',
    'CATALOG_DOES_NOT_EXIST',
    catalogField,
    'Catalog does not exist.',
    'priceListCreate nonexistent catalogId',
  );
  assertPriceListNull(createMissingCatalog.response, 'priceListCreate', 'priceListCreate nonexistent catalogId');
  cases.push(createMissingCatalog);

  const takenPriceListSetup = await captureCase(
    'priceListCreate setup for catalog taken validation',
    priceListCreateMutation,
    priceListInput(`Price list catalog taken setup ${unique}`, market.currency),
  );
  assertNoUserErrors(takenPriceListSetup.response, 'priceListCreate', 'catalog taken price-list setup');
  const takenPriceListId = priceListId(takenPriceListSetup.response, 'priceListCreate');
  createdPriceListIds.push(takenPriceListId);
  cases.push(takenPriceListSetup);

  const takenCatalogSetup = await captureCase(
    'catalogCreate setup with assigned price list',
    catalogCreateMutation,
    catalogInput(`Catalog taken setup ${unique}`, market.id, { priceListId: takenPriceListId }),
  );
  assertNoUserErrors(takenCatalogSetup.response, 'catalogCreate', 'catalog taken setup');
  const takenCatalogId = catalogId(takenCatalogSetup.response, 'catalogCreate');
  createdCatalogIds.push(takenCatalogId);
  cases.push(takenCatalogSetup);

  const createTakenCatalog = await captureCase(
    'priceListCreate catalog already has price list validation',
    priceListCreateMutation,
    priceListInput(`Price list catalog taken ${unique}`, market.currency, { catalogId: takenCatalogId }),
  );
  assertUserError(
    createTakenCatalog.response,
    'priceListCreate',
    'CATALOG_TAKEN',
    catalogField,
    'Catalog has a price list already assigned.',
    'priceListCreate catalog taken',
  );
  assertPriceListNull(createTakenCatalog.response, 'priceListCreate', 'priceListCreate catalog taken');
  cases.push(createTakenCatalog);

  const updateMissingSetup = await captureCase(
    'priceListCreate setup for update nonexistent catalogId validation',
    priceListCreateMutation,
    priceListInput(`Price list update missing catalog setup ${unique}`, 'USD'),
  );
  assertNoUserErrors(updateMissingSetup.response, 'priceListCreate', 'update missing price-list setup');
  const updateMissingPriceListId = priceListId(updateMissingSetup.response, 'priceListCreate');
  createdPriceListIds.push(updateMissingPriceListId);
  cases.push(updateMissingSetup);

  const updateMissingCatalog = await captureCase(
    'priceListUpdate nonexistent catalogId validation',
    priceListUpdateMutation,
    {
      id: updateMissingPriceListId,
      input: {
        catalogId: missingCatalogId,
      },
    },
  );
  assertUserError(
    updateMissingCatalog.response,
    'priceListUpdate',
    'CATALOG_DOES_NOT_EXIST',
    catalogField,
    'Catalog does not exist.',
    'priceListUpdate nonexistent catalogId',
  );
  cases.push(updateMissingCatalog);

  const updateTakenCatalog = await captureCase(
    'priceListUpdate catalog already has price list validation',
    priceListUpdateMutation,
    {
      id: updateMissingPriceListId,
      input: {
        catalogId: takenCatalogId,
      },
    },
  );
  assertUserError(
    updateTakenCatalog.response,
    'priceListUpdate',
    'CATALOG_TAKEN',
    catalogField,
    'Catalog has a price list already assigned.',
    'priceListUpdate catalog taken',
  );
  cases.push(updateTakenCatalog);

  const createWrongTypeCatalog = await captureCase(
    'priceListCreate wrong-type catalogId validation',
    priceListCreateMutation,
    priceListInput(`Price list wrong type catalog ${unique}`, 'USD', { catalogId: wrongTypeCatalogId }),
  );
  assertResourceNotFound(
    createWrongTypeCatalog.response,
    'priceListCreate',
    wrongTypeCatalogId,
    'priceListCreate wrong-type catalogId',
  );
  cases.push(createWrongTypeCatalog);

  const updateWrongTypeCatalog = await captureCase(
    'priceListUpdate wrong-type catalogId validation',
    priceListUpdateMutation,
    {
      id: updateMissingPriceListId,
      input: {
        catalogId: wrongTypeCatalogId,
      },
    },
  );
  assertResourceNotFound(
    updateWrongTypeCatalog.response,
    'priceListUpdate',
    wrongTypeCatalogId,
    'priceListUpdate wrong-type catalogId',
  );
  cases.push(updateWrongTypeCatalog);
} finally {
  for (const id of [...createdPriceListIds].reverse()) {
    cleanup.push({
      type: 'priceList',
      id,
      response: await runGraphqlRequest(priceListDeleteMutation, { id }),
    });
  }
  for (const id of [...createdCatalogIds].reverse()) {
    cleanup.push({
      type: 'catalog',
      id,
      response: await runGraphqlRequest(catalogDeleteMutation, { id }),
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
      scope: 'price-list catalogId existence and taken validation',
      setup: {
        market,
        missingCatalogId,
        wrongTypeCatalogId,
        note: 'Records public Admin GraphQL 2026-04 catalogId validation for priceListCreate/priceListUpdate. Correctly typed never-created MarketCatalog ids return PriceListUserError CATALOG_DOES_NOT_EXIST, taken MarketCatalog ids return CATALOG_TAKEN, and wrong-resource Shopify catalog ids return top-level RESOURCE_NOT_FOUND invalid-id errors. Validation failures do not create or update price lists.',
      },
      cases,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'PriceListCatalogValidationMarketsRead',
          variables: marketsReadVariables,
          query: marketsReadQuery,
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
      market,
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
