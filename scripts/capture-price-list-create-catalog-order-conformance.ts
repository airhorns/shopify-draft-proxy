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
  priceList?: { id?: string } | null;
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
const outputPath = path.join(outputDir, 'price-list-create-catalog-order.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketsReadQuery = `#graphql
query PriceListCatalogOrderMarketsRead($first: Int!) {
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
mutation PriceListCreateCatalogOrder($input: PriceListCreateInput!) {
  priceListCreate(input: $input) {
    priceList {
      id
      name
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
mutation PriceListCatalogOrderCleanup($id: ID!) {
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

function marketCurrency(market: Record<string, unknown>): string | undefined {
  const currencySettings = market['currencySettings'] as { baseCurrency?: { currencyCode?: string } } | undefined;
  return currencySettings?.baseCurrency?.currencyCode;
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
const missingCatalogId = 'gid://shopify/MarketCatalog/999999999999999';
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
  'markets read for priceListCreate catalog-order validation',
  marketsReadQuery,
  marketsReadVariables,
);
assertNoGraphqlErrors(marketsRead.response, 'markets read');
cases.push(marketsRead);
const market = firstMarket(marketsRead.response);

try {
  const duplicateName = `Catalog order duplicate ${unique}`;
  const duplicateSetup = await captureCase(
    'priceListCreate baseline for duplicate-name plus nonexistent catalog order',
    priceListCreateMutation,
    priceListInput(duplicateName, 'USD'),
  );
  assertNoUserErrors(duplicateSetup.response, 'priceListCreate', 'duplicate-name setup');
  createdPriceListIds.push(priceListId(duplicateSetup.response, 'priceListCreate'));
  cases.push(duplicateSetup);

  const duplicateNameMissingCatalog = await captureCase(
    'priceListCreate duplicate name plus nonexistent catalogId',
    priceListCreateMutation,
    priceListInput(duplicateName, 'USD', { catalogId: missingCatalogId }),
  );
  assertUserError(
    duplicateNameMissingCatalog.response,
    'priceListCreate',
    'CATALOG_DOES_NOT_EXIST',
    catalogField,
    'Catalog does not exist.',
    'duplicate name plus nonexistent catalogId',
  );
  assertPriceListNull(
    duplicateNameMissingCatalog.response,
    'priceListCreate',
    'duplicate name plus nonexistent catalogId',
  );
  cases.push(duplicateNameMissingCatalog);

  const invalidAdjustmentMissingCatalog = await captureCase(
    'priceListCreate invalid adjustment plus nonexistent catalogId',
    priceListCreateMutation,
    {
      input: {
        name: `Catalog order invalid adjustment ${unique}`,
        currency: 'USD',
        parent: {
          adjustment: {
            type: 'PERCENTAGE_DECREASE',
            value: 250,
          },
        },
        catalogId: missingCatalogId,
      },
    },
  );
  assertUserError(
    invalidAdjustmentMissingCatalog.response,
    'priceListCreate',
    'CATALOG_DOES_NOT_EXIST',
    catalogField,
    'Catalog does not exist.',
    'invalid adjustment plus nonexistent catalogId',
  );
  assertPriceListNull(
    invalidAdjustmentMissingCatalog.response,
    'priceListCreate',
    'invalid adjustment plus nonexistent catalogId',
  );
  cases.push(invalidAdjustmentMissingCatalog);

  const takenName = `Catalog order taken ${unique}`;
  const takenPriceListSetup = await captureCase(
    'priceListCreate baseline for duplicate-name plus taken catalog order',
    priceListCreateMutation,
    priceListInput(takenName, market.currency),
  );
  assertNoUserErrors(takenPriceListSetup.response, 'priceListCreate', 'taken price-list setup');
  const takenPriceListId = priceListId(takenPriceListSetup.response, 'priceListCreate');
  createdPriceListIds.push(takenPriceListId);
  cases.push(takenPriceListSetup);

  const takenCatalogSetup = await captureCase(
    'catalogCreate setup with assigned price list for catalog-order validation',
    catalogCreateMutation,
    catalogInput(`Catalog order taken ${unique}`, market.id, { priceListId: takenPriceListId }),
  );
  assertNoUserErrors(takenCatalogSetup.response, 'catalogCreate', 'taken catalog setup');
  const takenCatalogId = catalogId(takenCatalogSetup.response, 'catalogCreate');
  createdCatalogIds.push(takenCatalogId);
  cases.push(takenCatalogSetup);

  const duplicateNameTakenCatalog = await captureCase(
    'priceListCreate duplicate name plus taken catalogId',
    priceListCreateMutation,
    priceListInput(takenName, market.currency, { catalogId: takenCatalogId }),
  );
  assertUserError(
    duplicateNameTakenCatalog.response,
    'priceListCreate',
    'CATALOG_TAKEN',
    catalogField,
    'Catalog has a price list already assigned.',
    'duplicate name plus taken catalogId',
  );
  assertPriceListNull(duplicateNameTakenCatalog.response, 'priceListCreate', 'duplicate name plus taken catalogId');
  cases.push(duplicateNameTakenCatalog);
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
      scope: 'priceListCreate catalogId validation order',
      setup: {
        market,
        missingCatalogId,
        note: 'Records public Admin GraphQL behavior when priceListCreate input is invalid in more than one way. CatalogId existence/taken validation is returned before duplicate-name and parent adjustment validation.',
      },
      cases,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'PriceListCatalogOrderMarketsRead',
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
