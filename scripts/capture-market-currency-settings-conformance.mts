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

type MarketCurrencySettings = {
  baseCurrency?: {
    currencyCode?: string | null;
    currencyName?: string | null;
  } | null;
  localCurrencies?: boolean | null;
  roundingEnabled?: boolean | null;
};

type MarketData = {
  id?: string | null;
  name?: string | null;
  status?: string | null;
  enabled?: boolean | null;
  currencySettings?: MarketCurrencySettings | null;
};

type MarketCreateData = {
  marketCreate?: {
    market?: MarketData | null;
    userErrors?: UserError[];
  } | null;
};

type MarketReadData = {
  market?: MarketData | null;
};

type MarketDeleteData = {
  marketDelete?: {
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'market-create-currency-settings.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const marketCreateMutation = `#graphql
mutation MarketCreateCurrencySettings($input: MarketCreateInput!) {
  marketCreate(input: $input) {
    market {
      id
      name
      status
      enabled
      currencySettings {
        baseCurrency {
          currencyCode
          currencyName
        }
        localCurrencies
        roundingEnabled
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
query MarketCreateCurrencySettingsRead($id: ID!) {
  market(id: $id) {
    id
    name
    status
    enabled
    currencySettings {
        baseCurrency {
          currencyCode
          currencyName
        }
      localCurrencies
      roundingEnabled
    }
  }
}
`;

const marketDeleteMutation = `#graphql
mutation MarketCreateCurrencySettingsCleanup($id: ID!) {
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

async function captureCase<TData>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  options: { allowTopLevelErrors?: boolean } = {},
): Promise<CapturedCase<TData>> {
  const response = await runGraphqlRequest<TData>(query, variables);
  if (response.status < 200 || response.status >= 300 || (!options.allowTopLevelErrors && response.payload.errors)) {
    throw new Error(`${name} failed: ${JSON.stringify(response.payload)}`);
  }

  return { name, query, variables, response };
}

function marketCreate(capture: CapturedCase<MarketCreateData>): MarketCreateData['marketCreate'] {
  return capture.response.payload.data?.marketCreate;
}

function marketId(capture: CapturedCase<MarketCreateData>): string | null {
  const id = marketCreate(capture)?.market?.id;
  return typeof id === 'string' ? id : null;
}

function userErrors(capture: CapturedCase<MarketCreateData>): UserError[] {
  return marketCreate(capture)?.userErrors ?? [];
}

function assertCreatedWithoutErrors(capture: CapturedCase<MarketCreateData>, label: string): string {
  const id = marketId(capture);
  const errors = userErrors(capture);
  if (!id || errors.length > 0) {
    throw new Error(`${label} did not create a market cleanly: ${JSON.stringify(capture.response.payload)}`);
  }
  return id;
}

function assertCurrencyFlag(
  capture: CapturedCase<MarketCreateData>,
  field: keyof Pick<MarketCurrencySettings, 'localCurrencies' | 'roundingEnabled'>,
  expected: boolean,
  label: string,
): void {
  const actual = marketCreate(capture)?.market?.currencySettings?.[field];
  if (actual !== expected) {
    throw new Error(`${label} expected ${field}=${expected}, got ${JSON.stringify(actual)}`);
  }
}

function assertManualRateRejected(capture: CapturedCase<MarketCreateData>): void {
  const errors = userErrors(capture);
  const rejected = errors.some((error) => {
    return (
      Array.isArray(error.field) &&
      error.field.join('.') === 'input.currencySettings.baseCurrencyManualRate' &&
      typeof error.message === 'string'
    );
  });
  if (!rejected) {
    throw new Error(`Expected manual-rate validation error: ${JSON.stringify(capture.response.payload)}`);
  }
  if (marketId(capture)) {
    throw new Error(
      `Manual-rate validation unexpectedly created a market: ${JSON.stringify(capture.response.payload)}`,
    );
  }
}

function assertBaseCurrencyName(capture: CapturedCase<MarketCreateData>, code: string, expected: string): void {
  const baseCurrency = marketCreate(capture)?.market?.currencySettings?.baseCurrency;
  if (baseCurrency?.currencyCode !== code || baseCurrency.currencyName !== expected) {
    throw new Error(
      `Expected base currency ${code} / ${expected}, got ${JSON.stringify(baseCurrency)} in ${JSON.stringify(
        capture.response.payload,
      )}`,
    );
  }
}

function assertBaseCurrencyCode(capture: CapturedCase<MarketCreateData>, code: string): void {
  const baseCurrency = marketCreate(capture)?.market?.currencySettings?.baseCurrency;
  if (baseCurrency?.currencyCode !== code) {
    throw new Error(`Expected base currency code ${code}, got ${JSON.stringify(baseCurrency)}`);
  }
}

function assertInvalidCurrencyCoercion(capture: CapturedCase<MarketCreateData>, code: string): void {
  const errors = capture.response.payload.errors;
  if (!Array.isArray(errors)) {
    throw new Error(
      `Expected top-level INVALID_VARIABLE errors for ${code}: ${JSON.stringify(capture.response.payload)}`,
    );
  }
  const found = errors.some((error) => {
    if (typeof error !== 'object' || error === null || Array.isArray(error)) return false;
    const extensions = (error as Record<string, unknown>)['extensions'];
    if (typeof extensions !== 'object' || extensions === null || Array.isArray(extensions)) return false;
    const problems = (extensions as Record<string, unknown>)['problems'];
    return (
      (extensions as Record<string, unknown>)['code'] === 'INVALID_VARIABLE' &&
      Array.isArray(problems) &&
      problems.some((problem) => {
        if (typeof problem !== 'object' || problem === null || Array.isArray(problem)) return false;
        return (
          JSON.stringify((problem as Record<string, unknown>)['path']) ===
          JSON.stringify(['currencySettings', 'baseCurrency'])
        );
      })
    );
  });
  if (!found) {
    throw new Error(`Expected CurrencyCode coercion error for ${code}: ${JSON.stringify(capture.response.payload)}`);
  }
  if (marketId(capture)) {
    throw new Error(
      `Invalid currency ${code} unexpectedly created a market: ${JSON.stringify(capture.response.payload)}`,
    );
  }
}

const stamp = new Date()
  .toISOString()
  .replace(/[^0-9]/g, '')
  .slice(0, 14);
const cases: CapturedCase<MarketCreateData | MarketReadData>[] = [];
const cleanupCases: CapturedCase<MarketDeleteData>[] = [];
const createdIds: string[] = [];

try {
  const localCurrenciesCreate = await captureCase<MarketCreateData>(
    'marketCreateLocalCurrencies',
    marketCreateMutation,
    {
      input: {
        name: `Draft Proxy Currency Settings ${stamp} Local Currencies`,
        status: 'ACTIVE',
        enabled: true,
        currencySettings: {
          baseCurrency: 'USD',
          localCurrencies: true,
        },
      },
    },
  );
  const localCurrenciesId = assertCreatedWithoutErrors(localCurrenciesCreate, 'localCurrencies marketCreate');
  assertCurrencyFlag(localCurrenciesCreate, 'localCurrencies', true, 'localCurrencies marketCreate');
  createdIds.push(localCurrenciesId);
  cases.push(localCurrenciesCreate);
  cases.push(
    await captureCase<MarketReadData>('marketReadLocalCurrencies', marketReadQuery, {
      id: localCurrenciesId,
    }),
  );

  const roundingCreate = await captureCase<MarketCreateData>('marketCreateRoundingEnabled', marketCreateMutation, {
    input: {
      name: `Draft Proxy Currency Settings ${stamp} Rounding`,
      status: 'ACTIVE',
      enabled: true,
      currencySettings: {
        baseCurrency: 'USD',
        roundingEnabled: true,
      },
    },
  });
  const roundingId = assertCreatedWithoutErrors(roundingCreate, 'roundingEnabled marketCreate');
  assertCurrencyFlag(roundingCreate, 'roundingEnabled', true, 'roundingEnabled marketCreate');
  createdIds.push(roundingId);
  cases.push(roundingCreate);
  cases.push(
    await captureCase<MarketReadData>('marketReadRoundingEnabled', marketReadQuery, {
      id: roundingId,
    }),
  );

  const manualRateInvalid = await captureCase<MarketCreateData>('marketCreateInvalidManualRate', marketCreateMutation, {
    input: {
      name: `Draft Proxy Currency Settings ${stamp} Manual Rate`,
      currencySettings: {
        baseCurrency: 'USD',
        baseCurrencyManualRate: 0,
      },
    },
  });
  assertManualRateRejected(manualRateInvalid);
  cases.push(manualRateInvalid);

  const euroCurrencyCreate = await captureCase<MarketCreateData>('marketCreateEuroCurrencyName', marketCreateMutation, {
    input: {
      name: `Draft Proxy Currency Settings ${stamp} Euro`,
      currencySettings: {
        baseCurrency: 'EUR',
      },
    },
  });
  const euroCurrencyId = assertCreatedWithoutErrors(euroCurrencyCreate, 'EUR currencyName marketCreate');
  assertBaseCurrencyName(euroCurrencyCreate, 'EUR', 'Euro');
  createdIds.push(euroCurrencyId);
  cases.push(euroCurrencyCreate);
  cases.push(
    await captureCase<MarketReadData>('marketReadEuroCurrencyName', marketReadQuery, {
      id: euroCurrencyId,
    }),
  );

  const xafCurrencyCreate = await captureCase<MarketCreateData>('marketCreateXafCurrency', marketCreateMutation, {
    input: {
      name: `Draft Proxy Currency Settings ${stamp} XAF`,
      currencySettings: {
        baseCurrency: 'XAF',
      },
    },
  });
  const xafCurrencyId = assertCreatedWithoutErrors(xafCurrencyCreate, 'XAF currency marketCreate');
  assertBaseCurrencyCode(xafCurrencyCreate, 'XAF');
  createdIds.push(xafCurrencyId);
  cases.push(xafCurrencyCreate);
  cases.push(
    await captureCase<MarketReadData>('marketReadXafCurrency', marketReadQuery, {
      id: xafCurrencyId,
    }),
  );

  const unknownCurrencyInvalid = await captureCase<MarketCreateData>(
    'marketCreateUnknownCurrencyCoercion',
    marketCreateMutation,
    {
      input: {
        name: `Draft Proxy Currency Settings ${stamp} Unknown`,
        currencySettings: {
          baseCurrency: 'ZZZ',
        },
      },
    },
    { allowTopLevelErrors: true },
  );
  assertInvalidCurrencyCoercion(unknownCurrencyInvalid, 'ZZZ');
  cases.push(unknownCurrencyInvalid);
} finally {
  for (const id of createdIds.toReversed()) {
    cleanupCases.push(
      await captureCase<MarketDeleteData>('marketDeleteCleanup', marketDeleteMutation, {
        id,
      }),
    );
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
      cases,
      cleanupCases,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

const manualRateErrors = userErrors(cases[4] as CapturedCase<MarketCreateData>);
const euroBaseCurrency = marketCreate(cases[5] as CapturedCase<MarketCreateData>)?.market?.currencySettings
  ?.baseCurrency;
const xafBaseCurrency = marketCreate(cases[7] as CapturedCase<MarketCreateData>)?.market?.currencySettings
  ?.baseCurrency;
const invalidCurrencyErrors = (cases[9] as CapturedCase<MarketCreateData>).response.payload.errors;
console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain,
      apiVersion,
      createdMarkets: createdIds.length,
      cleanedUpMarkets: cleanupCases.length,
      manualRateErrors,
      euroBaseCurrency,
      xafBaseCurrency,
      invalidCurrencyErrors,
    },
    null,
    2,
  ),
);
