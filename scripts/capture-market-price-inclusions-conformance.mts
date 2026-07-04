/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
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

type MarketPricingData = {
  shop?: {
    currencyCode?: string;
  };
  marketCreate?: {
    market?: {
      id?: string;
      name?: string;
      handle?: string;
      status?: string;
      type?: string;
      currencySettings?: {
        baseCurrency?: {
          currencyCode?: string;
          currencyName?: string;
        } | null;
        localCurrencies?: boolean;
        roundingEnabled?: boolean;
      } | null;
      priceInclusions?: {
        inclusiveDutiesPricingStrategy?: string;
        inclusiveTaxPricingStrategy?: string;
      } | null;
    } | null;
    userErrors?: UserError[];
  };
  market?: {
    id?: string;
    name?: string;
    handle?: string;
    status?: string;
    type?: string;
    currencySettings?: {
      baseCurrency?: {
        currencyCode?: string;
        currencyName?: string;
      } | null;
      localCurrencies?: boolean;
      roundingEnabled?: boolean;
    } | null;
    priceInclusions?: {
      inclusiveDutiesPricingStrategy?: string;
      inclusiveTaxPricingStrategy?: string;
    } | null;
  } | null;
  marketsResolvedValues?: {
    currencyCode?: string;
    priceInclusivity?: {
      dutiesIncluded?: boolean;
      taxesIncluded?: boolean;
    } | null;
  } | null;
  locations?: {
    nodes?: Array<{ id?: string; name?: string; isActive?: boolean }>;
  };
};

type MarketDeleteData = {
  marketDelete?: {
    deletedId?: string | null;
    userErrors?: UserError[];
  };
};

type CapturedCase = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<MarketPricingData>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'market-create-price-inclusions.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const shopCurrencyQuery = `#graphql
query MarketPriceInclusionsShopCurrency {
  shop {
    currencyCode
  }
}
`;

const marketCreatePriceInclusionsMutation = `#graphql
mutation MarketCreatePriceInclusions($input: MarketCreateInput!) {
  marketCreate(input: $input) {
    market {
      id
      name
      handle
      status
      type
      currencySettings {
        baseCurrency {
          currencyCode
          currencyName
        }
        localCurrencies
        roundingEnabled
      }
      priceInclusions {
        inclusiveDutiesPricingStrategy
        inclusiveTaxPricingStrategy
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

const marketPriceInclusionsReadQuery = `#graphql
query MarketPriceInclusionsRead($id: ID!) {
  market(id: $id) {
    id
    name
    handle
    status
    type
    currencySettings {
      baseCurrency {
        currencyCode
        currencyName
      }
      localCurrencies
      roundingEnabled
    }
    priceInclusions {
      inclusiveDutiesPricingStrategy
      inclusiveTaxPricingStrategy
    }
  }
}
`;

const marketsResolvedValuesReadQuery = `#graphql
query MarketPriceInclusionsResolvedValuesRead($buyerSignal: BuyerSignalInput!) {
  marketsResolvedValues(buyerSignal: $buyerSignal) {
    currencyCode
    priceInclusivity {
      dutiesIncluded
      taxesIncluded
    }
  }
}
`;

const locationsQuery = `#graphql
query MarketPriceInclusionsLocations {
  locations(first: 5) {
    nodes {
      id
      name
      isActive
    }
  }
}
`;

const marketDeleteMutation = `#graphql
mutation MarketPriceInclusionsCleanup($id: ID!) {
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

function createdMarketId(result: ConformanceGraphqlResult<MarketPricingData>): string | null {
  const market = result.payload.data?.marketCreate?.market;
  return typeof market?.id === 'string' ? market.id : null;
}

function marketFromResult(
  result: ConformanceGraphqlResult<MarketPricingData>,
  root: 'marketCreate' | 'market',
): NonNullable<MarketPricingData['market']> | null {
  return root === 'marketCreate'
    ? (result.payload.data?.marketCreate?.market ?? null)
    : (result.payload.data?.market ?? null);
}

function shopCurrencyCode(result: ConformanceGraphqlResult<MarketPricingData>): string {
  const currencyCode = result.payload.data?.shop?.currencyCode;
  if (result.status !== 200 || typeof currencyCode !== 'string' || currencyCode.length === 0) {
    throw new Error(
      `shop currency probe did not return a currencyCode: status=${result.status} payload=${JSON.stringify(
        result.payload,
      )}`,
    );
  }
  if (currencyCode === 'USD') {
    throw new Error('marketCreate price-inclusions parity requires a non-USD conformance shop currency.');
  }
  return currencyCode;
}

function assertCurrencySettings(
  result: ConformanceGraphqlResult<MarketPricingData>,
  root: 'marketCreate' | 'market',
  expectedCurrencyCode: string,
  label: string,
): void {
  const market = marketFromResult(result, root);
  const userErrors = result.payload.data?.marketCreate?.userErrors ?? [];
  const settings = market?.currencySettings;
  if (
    result.status !== 200 ||
    !market ||
    userErrors.length > 0 ||
    settings?.baseCurrency?.currencyCode !== expectedCurrencyCode ||
    typeof settings?.baseCurrency?.currencyName !== 'string' ||
    settings?.localCurrencies !== true ||
    settings?.roundingEnabled !== true
  ) {
    throw new Error(
      `${label} did not return expected shop-currency defaults: status=${result.status} market=${JSON.stringify(
        market,
      )} userErrors=${JSON.stringify(userErrors)} errors=${JSON.stringify(result.payload.errors ?? null)}`,
    );
  }
}

function assertPriceInclusions(
  result: ConformanceGraphqlResult<MarketPricingData>,
  root: 'marketCreate' | 'market',
  label: string,
): void {
  const market = marketFromResult(result, root);
  const userErrors = result.payload.data?.marketCreate?.userErrors ?? [];
  const priceInclusions = market?.priceInclusions;
  if (
    result.status !== 200 ||
    !market ||
    userErrors.length > 0 ||
    priceInclusions?.inclusiveDutiesPricingStrategy !== 'ADD_DUTIES_AT_CHECKOUT' ||
    priceInclusions.inclusiveTaxPricingStrategy !== 'INCLUDES_TAXES_IN_PRICE'
  ) {
    throw new Error(
      `${label} did not return expected price inclusions: status=${result.status} market=${JSON.stringify(
        market,
      )} userErrors=${JSON.stringify(userErrors)} errors=${JSON.stringify(result.payload.errors ?? null)}`,
    );
  }
}

function assertResolvedValues(
  result: ConformanceGraphqlResult<MarketPricingData>,
  expectedCurrencyCode: string,
  label: string,
): void {
  const resolved = result.payload.data?.marketsResolvedValues;
  const priceInclusivity = resolved?.priceInclusivity;
  if (
    result.status !== 200 ||
    resolved?.currencyCode !== expectedCurrencyCode ||
    priceInclusivity?.dutiesIncluded !== false ||
    priceInclusivity?.taxesIncluded !== true
  ) {
    throw new Error(
      `${label} did not return expected resolved values: status=${result.status} resolved=${JSON.stringify(
        resolved,
      )} errors=${JSON.stringify(result.payload.errors ?? null)}`,
    );
  }
}

function assertInclusivePricingError(result: ConformanceGraphqlResult<MarketPricingData>, label: string): void {
  const market = result.payload.data?.marketCreate?.market;
  const userErrors = result.payload.data?.marketCreate?.userErrors ?? [];
  const hasExpectedError = userErrors.some(
    (error) =>
      error.code === 'INCLUSIVE_PRICING_NOT_COMPATIBLE_WITH_CONDITION_TYPES' &&
      JSON.stringify(error.field ?? null) === JSON.stringify(['input', 'priceInclusions']),
  );
  if (result.status !== 200 || market !== null || !hasExpectedError) {
    throw new Error(
      `${label} did not return expected inclusive-pricing userError: status=${result.status} market=${JSON.stringify(
        market,
      )} userErrors=${JSON.stringify(userErrors)} errors=${JSON.stringify(result.payload.errors ?? null)}`,
    );
  }
}

async function cleanupMarket(id: string): Promise<ConformanceGraphqlResult<MarketDeleteData>> {
  return runGraphqlRequest<MarketDeleteData>(marketDeleteMutation, { id });
}

await mkdir(outputDir, { recursive: true });

const cases: CapturedCase[] = [];
const createdIds: string[] = [];
const cleanup: Array<{
  id: string;
  response: ConformanceGraphqlResult<MarketDeleteData>;
}> = [];

let selectedLocationId: string | null = null;

try {
  const shopCurrency = await runGraphqlRequest<MarketPricingData>(shopCurrencyQuery, {});
  const expectedShopCurrencyCode = shopCurrencyCode(shopCurrency);
  cases.push({
    name: 'shop currency hydrates non-USD market defaults',
    query: shopCurrencyQuery,
    variables: {},
    response: shopCurrency,
  });

  const createVariables = {
    input: {
      name: `Draft Proxy Pricing ${new Date()
        .toISOString()
        .replace(/[-:.TZ]/gu, '')
        .slice(0, 14)}`,
      conditions: {
        regionsCondition: {
          regions: [{ countryCode: 'DK' }],
        },
      },
      currencySettings: {
        localCurrencies: true,
        roundingEnabled: true,
      },
      priceInclusions: {
        taxPricingStrategy: 'INCLUDES_TAXES_IN_PRICE',
        dutiesPricingStrategy: 'ADD_DUTIES_AT_CHECKOUT',
      },
    },
  };
  const create = await runGraphqlRequest<MarketPricingData>(marketCreatePriceInclusionsMutation, createVariables);
  const marketId = createdMarketId(create);
  if (marketId) createdIds.push(marketId);
  assertPriceInclusions(create, 'marketCreate', 'marketCreate priceInclusions');
  assertCurrencySettings(
    create,
    'marketCreate',
    expectedShopCurrencyCode,
    'marketCreate currencySettings from shop currency',
  );
  cases.push({
    name: 'marketCreate accepts nested priceInclusions on a region market',
    query: marketCreatePriceInclusionsMutation,
    variables: createVariables,
    response: create,
  });

  if (!marketId) {
    throw new Error('marketCreate priceInclusions success did not return a market id.');
  }

  const readVariables = { id: marketId };
  const read = await runGraphqlRequest<MarketPricingData>(marketPriceInclusionsReadQuery, readVariables);
  assertPriceInclusions(read, 'market', 'market read-after-create priceInclusions');
  assertCurrencySettings(read, 'market', expectedShopCurrencyCode, 'market read-after-create currencySettings');
  cases.push({
    name: 'market read-after-create returns requested priceInclusions',
    query: marketPriceInclusionsReadQuery,
    variables: readVariables,
    response: read,
  });

  const resolvedValuesVariables = { buyerSignal: { countryCode: 'DK' } };
  const resolvedValues = await runGraphqlRequest<MarketPricingData>(
    marketsResolvedValuesReadQuery,
    resolvedValuesVariables,
  );
  assertResolvedValues(resolvedValues, expectedShopCurrencyCode, 'marketsResolvedValues after priceInclusions create');
  cases.push({
    name: 'marketsResolvedValues reflects created market price inclusivity',
    query: marketsResolvedValuesReadQuery,
    variables: resolvedValuesVariables,
    response: resolvedValues,
  });

  const locations = await runGraphqlRequest<MarketPricingData>(locationsQuery, {});
  selectedLocationId =
    locations.payload.data?.locations?.nodes?.find((location) => location.isActive && typeof location.id === 'string')
      ?.id ?? null;
  if (!selectedLocationId) {
    throw new Error(`No active location available for inclusive-pricing negative branch: ${JSON.stringify(locations)}`);
  }

  const negativeVariables = {
    input: {
      name: `Draft Proxy Location Pricing ${new Date()
        .toISOString()
        .replace(/[-:.TZ]/gu, '')
        .slice(0, 14)}`,
      conditions: {
        locationsCondition: {
          locationIds: [selectedLocationId],
        },
      },
      priceInclusions: {
        taxPricingStrategy: 'INCLUDES_TAXES_IN_PRICE',
        dutiesPricingStrategy: 'INCLUDE_DUTIES_IN_PRICE',
      },
    },
  };
  const negative = await runGraphqlRequest<MarketPricingData>(marketCreatePriceInclusionsMutation, negativeVariables);
  assertInclusivePricingError(negative, 'locationsCondition inclusive priceInclusions');
  cases.push({
    name: 'marketCreate rejects inclusive priceInclusions on locationsCondition',
    query: marketCreatePriceInclusionsMutation,
    variables: negativeVariables,
    response: negative,
  });
} finally {
  for (const id of createdIds.toReversed()) {
    cleanup.push({ id, response: await cleanupMarket(id) });
  }
}

const capture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scope: 'marketCreate priceInclusions and inclusive-pricing validation',
  selectedLocationId,
  cases,
  cleanup,
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain,
      apiVersion,
      selectedLocationId,
      createdIds,
      cleanupDeletedIds: cleanup.map((entry) => entry.response.payload.data?.marketDelete?.deletedId ?? null),
      caseNames: cases.map((entry) => entry.name),
    },
    null,
    2,
  ),
);
