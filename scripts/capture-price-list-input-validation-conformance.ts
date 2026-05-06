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

type CapturedCase<TData = unknown> = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<TData>;
};

type MutationPayload = {
  userErrors?: UserError[];
  priceList?: { id?: string; currency?: string } | null;
  catalog?: { id?: string } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'price-list-input-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketsReadQuery = `#graphql
query PriceListInputValidationMarketsRead($first: Int!) {
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

const priceListCreateMutation = `#graphql
mutation PriceListCreateInputValidation($input: PriceListCreateInput!) {
  priceListCreate(input: $input) {
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

const priceListUpdateMutation = `#graphql
mutation PriceListUpdateInputValidation($id: ID!, $input: PriceListUpdateInput!) {
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

const priceListDeleteMutation = `#graphql
mutation PriceListDeleteInputValidationCleanup($id: ID!) {
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
  label: string,
): void {
  assertNoGraphqlErrors(result, label);
  const errors = payloadUserErrors(result, root);
  const matched = errors.some(
    (error) => error.code === expectedCode && JSON.stringify(error.field ?? null) === JSON.stringify(expectedField),
  );
  if (!matched) {
    throw new Error(`${label} missing ${expectedCode}: ${JSON.stringify(errors)}`);
  }
}

function priceListId<TData>(result: ConformanceGraphqlResult<TData>, root: string): string {
  const id = mutationPayload(result, root).priceList?.id;
  if (typeof id !== 'string') {
    throw new Error(`${String(root)} did not return a price list id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function catalogId<TData>(result: ConformanceGraphqlResult<TData>, root: string): string {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) {
    throw new Error(`${String(root)} did not return data: ${JSON.stringify(result.payload)}`);
  }
  const payload = (data as Record<string, unknown>)[root];
  const id =
    typeof payload === 'object' && payload !== null
      ? (payload as { catalog?: { id?: string } | null }).catalog?.id
      : undefined;
  if (typeof id !== 'string') {
    throw new Error(`${String(root)} did not return a catalog id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function firstNonUsdMarket(result: ConformanceGraphqlResult<unknown>): {
  id: string;
  currency: string;
} {
  const nodes =
    (result.payload.data as { markets?: { nodes?: Array<Record<string, unknown>> } } | undefined)?.markets?.nodes ?? [];
  const market =
    nodes.find((node) => {
      const currency = marketCurrency(node);
      return typeof node['id'] === 'string' && typeof currency === 'string' && currency !== 'USD';
    }) ?? nodes.find((node) => typeof node['id'] === 'string' && typeof marketCurrency(node) === 'string');
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

function parentAdjustment(type: 'PERCENTAGE_DECREASE' | 'PERCENTAGE_INCREASE', value: number): Record<string, unknown> {
  return {
    parent: {
      adjustment: {
        type,
        value,
      },
    },
  };
}

function createInput(
  name: string,
  currency: string,
  adjustmentType: 'PERCENTAGE_DECREASE' | 'PERCENTAGE_INCREASE',
  adjustmentValue: number,
  extra: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    input: {
      name,
      currency,
      ...parentAdjustment(adjustmentType, adjustmentValue),
      ...extra,
    },
  };
}

function catalogCreateInput(title: string, marketId: string): Record<string, unknown> {
  return {
    input: {
      title,
      status: 'ACTIVE',
      context: {
        marketIds: [marketId],
      },
    },
  };
}

const unique = Date.now().toString(36);
const createdPriceListIds: string[] = [];
const createdCatalogIds: string[] = [];
const cleanup: Array<{
  type: 'priceList' | 'catalog';
  id: string;
  response: ConformanceGraphqlResult<unknown>;
}> = [];
const cases: Array<CapturedCase<unknown>> = [];
const adjustmentField = ['input', 'parent', 'adjustment', 'value'];

const marketsReadVariables = { first: 10 };
const marketsRead = await captureCase(
  'markets read for price-list input validation',
  marketsReadQuery,
  marketsReadVariables,
);
assertNoGraphqlErrors(marketsRead.response, 'markets read');
cases.push(marketsRead);
const market = firstNonUsdMarket(marketsRead.response);
const mismatchedCurrency = market.currency === 'USD' ? 'CAD' : 'USD';

try {
  const zeroAdjustment = await captureCase(
    'priceListCreate zero percentage decrease accepted',
    priceListCreateMutation,
    createInput(`Price list zero adjustment ${unique}`, 'USD', 'PERCENTAGE_DECREASE', 0),
  );
  assertNoUserErrors(zeroAdjustment.response, 'priceListCreate', 'zero adjustment create');
  createdPriceListIds.push(priceListId(zeroAdjustment.response, 'priceListCreate'));
  cases.push(zeroAdjustment);

  const negativeAdjustment = await captureCase(
    'priceListCreate negative adjustment invalid',
    priceListCreateMutation,
    createInput(`Price list negative adjustment ${unique}`, 'USD', 'PERCENTAGE_DECREASE', -10),
  );
  assertUserError(
    negativeAdjustment.response,
    'priceListCreate',
    'INVALID_ADJUSTMENT_VALUE',
    adjustmentField,
    'negative adjustment create',
  );
  cases.push(negativeAdjustment);

  const decreaseTooLarge = await captureCase(
    'priceListCreate percentage decrease above one hundred invalid',
    priceListCreateMutation,
    createInput(`Price list decrease too large ${unique}`, 'USD', 'PERCENTAGE_DECREASE', 250),
  );
  assertUserError(
    decreaseTooLarge.response,
    'priceListCreate',
    'INVALID_ADJUSTMENT_VALUE',
    adjustmentField,
    'percentage decrease above one hundred create',
  );
  cases.push(decreaseTooLarge);

  const increaseTooLarge = await captureCase(
    'priceListCreate percentage increase above one thousand invalid',
    priceListCreateMutation,
    createInput(`Price list increase too large ${unique}`, 'USD', 'PERCENTAGE_INCREASE', 5000),
  );
  assertUserError(
    increaseTooLarge.response,
    'priceListCreate',
    'INVALID_ADJUSTMENT_VALUE',
    adjustmentField,
    'percentage increase above one thousand create',
  );
  cases.push(increaseTooLarge);

  const updateAdjustmentSetup = await captureCase(
    'priceListCreate for update adjustment validation',
    priceListCreateMutation,
    createInput(`Price list update adjustment setup ${unique}`, 'USD', 'PERCENTAGE_DECREASE', 10),
  );
  assertNoUserErrors(updateAdjustmentSetup.response, 'priceListCreate', 'update adjustment setup');
  const updateAdjustmentPriceListId = priceListId(updateAdjustmentSetup.response, 'priceListCreate');
  createdPriceListIds.push(updateAdjustmentPriceListId);
  cases.push(updateAdjustmentSetup);

  const updateDecreaseTooLarge = await captureCase(
    'priceListUpdate percentage decrease above one hundred invalid',
    priceListUpdateMutation,
    {
      id: updateAdjustmentPriceListId,
      input: parentAdjustment('PERCENTAGE_DECREASE', 250),
    },
  );
  assertUserError(
    updateDecreaseTooLarge.response,
    'priceListUpdate',
    'INVALID_ADJUSTMENT_VALUE',
    adjustmentField,
    'percentage decrease above one hundred update',
  );
  cases.push(updateDecreaseTooLarge);

  const createMismatchCatalog = await captureCase(
    'catalogCreate for priceListCreate currency mismatch acceptance',
    catalogCreateMutation,
    catalogCreateInput(`Catalog create mismatch ${unique}`, market.id),
  );
  assertNoUserErrors(createMismatchCatalog.response, 'catalogCreate', 'create mismatch catalog setup');
  const createMismatchCatalogId = catalogId(createMismatchCatalog.response, 'catalogCreate');
  createdCatalogIds.push(createMismatchCatalogId);
  cases.push(createMismatchCatalog);

  const createMismatch = await captureCase(
    'priceListCreate accepts catalog market currency mismatch',
    priceListCreateMutation,
    createInput(`Price list create mismatch ${unique}`, mismatchedCurrency, 'PERCENTAGE_DECREASE', 10, {
      catalogId: createMismatchCatalogId,
    }),
  );
  assertNoUserErrors(createMismatch.response, 'priceListCreate', 'create catalog currency mismatch');
  createdPriceListIds.push(priceListId(createMismatch.response, 'priceListCreate'));
  cases.push(createMismatch);

  const updateMismatchCatalog = await captureCase(
    'catalogCreate for priceListUpdate currency mismatch acceptance',
    catalogCreateMutation,
    catalogCreateInput(`Catalog update mismatch ${unique}`, market.id),
  );
  assertNoUserErrors(updateMismatchCatalog.response, 'catalogCreate', 'update mismatch catalog setup');
  const updateMismatchCatalogId = catalogId(updateMismatchCatalog.response, 'catalogCreate');
  createdCatalogIds.push(updateMismatchCatalogId);
  cases.push(updateMismatchCatalog);

  const updateMismatchSetup = await captureCase(
    'priceListCreate for priceListUpdate currency mismatch acceptance',
    priceListCreateMutation,
    createInput(`Price list update mismatch setup ${unique}`, market.currency, 'PERCENTAGE_DECREASE', 10, {
      catalogId: updateMismatchCatalogId,
    }),
  );
  assertNoUserErrors(updateMismatchSetup.response, 'priceListCreate', 'update mismatch price-list setup');
  const updateMismatchPriceListId = priceListId(updateMismatchSetup.response, 'priceListCreate');
  createdPriceListIds.push(updateMismatchPriceListId);
  cases.push(updateMismatchSetup);

  const updateMismatch = await captureCase(
    'priceListUpdate accepts catalog market currency mismatch',
    priceListUpdateMutation,
    {
      id: updateMismatchPriceListId,
      input: {
        currency: mismatchedCurrency,
      },
    },
  );
  assertNoUserErrors(updateMismatch.response, 'priceListUpdate', 'update catalog currency mismatch');
  cases.push(updateMismatch);
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
      scope: 'price-list input validation and catalog currency behavior',
      setup: {
        market,
        mismatchedCurrency,
        note: 'Current public Admin GraphQL 2026-04 accepts a zero percentage-decrease adjustment and accepts catalog-linked price-list currencies that differ from the linked market base currency.',
      },
      cases,
      cleanup,
      upstreamCalls: [
        {
          operationName: 'PriceListInputValidationMarketsRead',
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
      mismatchedCurrency,
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
