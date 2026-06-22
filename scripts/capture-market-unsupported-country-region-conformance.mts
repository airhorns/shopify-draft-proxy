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

type MarketCreateData = {
  marketCreate?: {
    market?: {
      id?: string | null;
      name?: string | null;
      handle?: string | null;
      status?: string | null;
      enabled?: boolean | null;
    } | null;
    userErrors?: UserError[];
  } | null;
};

type CapturedCase<TData = unknown> = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult<TData>;
};

type CountryProbe = {
  countryCode: string;
  outcome: 'supported' | 'unsupported' | 'error';
  userErrors: UserError[];
  errorMessages?: string[];
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'market-create-unsupported-country-region.json');
const generatedModulePath = path.join('src', 'proxy', 'market_unsupported_country_regions.rs');
const marketCreateDocumentPath = path.join(
  'config',
  'parity-requests',
  'markets',
  'market-create-unsupported-country-region.graphql',
);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const countryCodeEnumQuery = `#graphql
query MarketCountryCodeEnumValues {
  __type(name: "CountryCode") {
    enumValues {
      name
    }
  }
}`;

const marketCreateMutation = await readFile(marketCreateDocumentPath, 'utf8');

const marketsReadAfterRejectedCreateQuery = `#graphql
query MarketsReadAfterUnsupportedCountryRegion($first: Int!) {
  markets(first: $first) {
    nodes {
      id
      name
      handle
      status
      enabled
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}`;

const captureStampValue = captureStamp();
const primaryVariables = {
  input: {
    name: `Draft Proxy Unsupported Region CU ${captureStampValue}`,
    regions: [{ countryCode: 'CU' }],
  },
};
const nonCuUnsupportedCountryCode = 'KP';
const nonCuUnsupportedMarketName = `Draft Proxy Unsupported Region ${nonCuUnsupportedCountryCode} ${captureStampValue}`;
const nonCuUnsupportedVariables = {
  input: {
    name: nonCuUnsupportedMarketName,
    regions: [{ countryCode: nonCuUnsupportedCountryCode }],
  },
};

function captureStamp(): string {
  return new Date()
    .toISOString()
    .replace(/[^0-9]/g, '')
    .slice(0, 14);
}

function enumValuesFromCapture(capture: CapturedCase): string[] {
  const data = capture.response.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw new Error('CountryCode introspection returned no data object.');
  }
  const typeInfo = data['__type'];
  if (typeof typeInfo !== 'object' || typeInfo === null || Array.isArray(typeInfo)) {
    throw new Error('CountryCode introspection returned no __type object.');
  }
  const enumValues = typeInfo['enumValues'];
  if (!Array.isArray(enumValues)) {
    throw new Error('CountryCode introspection returned no enumValues array.');
  }
  return enumValues
    .map((value) =>
      typeof value === 'object' && value !== null && !Array.isArray(value) && typeof value['name'] === 'string'
        ? value['name']
        : null,
    )
    .filter((value): value is string => value !== null)
    .sort();
}

function marketId(capture: CapturedCase<MarketCreateData>): string | null {
  const id = capture.response.payload.data?.marketCreate?.market?.id;
  return typeof id === 'string' ? id : null;
}

function userErrors(capture: CapturedCase<MarketCreateData>): UserError[] {
  return capture.response.payload.data?.marketCreate?.userErrors ?? [];
}

function assertUnsupportedCountryRegion(countryCode: string, capture: CapturedCase<MarketCreateData>): void {
  const errors = userErrors(capture);
  if (marketId(capture) || !errors.some((error) => error.code === 'UNSUPPORTED_COUNTRY_REGION')) {
    throw new Error(
      `${countryCode} capture did not return UNSUPPORTED_COUNTRY_REGION: ${JSON.stringify(capture.response.payload)}`,
    );
  }
}

function classifyProbe(capture: CapturedCase<MarketCreateData>): CountryProbe['outcome'] {
  if (capture.response.payload.errors) return 'error';
  const errors = userErrors(capture);
  if (errors.some((error) => error.code === 'UNSUPPORTED_COUNTRY_REGION')) return 'unsupported';
  if (errors.some((error) => error.code === 'TOO_SHORT')) return 'supported';
  throw new Error(`Unexpected marketCreate probe response: ${JSON.stringify(capture.response.payload)}`);
}

function errorMessages(capture: CapturedCase<MarketCreateData>): string[] {
  const errors = capture.response.payload.errors;
  if (!Array.isArray(errors)) return [];
  return errors
    .map((error) =>
      typeof error === 'object' && error !== null && !Array.isArray(error) && typeof error['message'] === 'string'
        ? error['message']
        : null,
    )
    .filter((message): message is string => message !== null);
}

function marketNamesFromRead(capture: CapturedCase): string[] {
  const data = capture.response.payload.data;
  if (typeof data !== 'object' || data === null || Array.isArray(data)) return [];
  const markets = data['markets'];
  if (typeof markets !== 'object' || markets === null || Array.isArray(markets)) return [];
  const nodes = markets['nodes'];
  if (!Array.isArray(nodes)) return [];
  return nodes
    .map((node) =>
      typeof node === 'object' && node !== null && !Array.isArray(node) && typeof node['name'] === 'string'
        ? node['name']
        : null,
    )
    .filter((name): name is string => name !== null);
}

async function captureCase<TData>(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase<TData>> {
  const response = await runGraphqlRequest<TData>(query, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${name} failed: ${JSON.stringify(response.payload)}`);
  }

  return {
    name,
    query,
    variables,
    response,
  };
}

async function captureMarketCreate(countryCode: string): Promise<CapturedCase<MarketCreateData>> {
  const variables = {
    input: {
      name: 'X',
      regions: [{ countryCode }],
    },
  };
  return {
    name: `marketCreate${countryCode}`,
    query: marketCreateMutation,
    variables,
    response: await runGraphqlRequest<MarketCreateData>(marketCreateMutation, variables),
  };
}

function renderUnsupportedModule(unsupportedCodes: string[]): string {
  const codeLines = unsupportedCodes.map((code) => `    "${code}",`).join('\n');
  return `// Shopify-derived Markets country/region support data.
//
// Generated from live Admin GraphQL conformance capture:
// fixtures/conformance/${storeDomain}/${apiVersion}/markets/market-create-unsupported-country-region.json

pub(in crate::proxy) fn is_unsupported_country_region(country_code: &str) -> bool {
    UNSUPPORTED_COUNTRY_REGION_CODES.contains(&country_code)
}

pub(in crate::proxy) const UNSUPPORTED_COUNTRY_REGION_CODES: &[&str] = &[
${codeLines}
];
`;
}

const introspectionCase = await captureCase('countryCodeEnumValues', countryCodeEnumQuery, {});
const countryCodes = enumValuesFromCapture(introspectionCase);
const primaryCase = await captureCase<MarketCreateData>(
  'marketCreateUnsupportedCountryRegion',
  marketCreateMutation,
  primaryVariables,
);
assertUnsupportedCountryRegion('CU', primaryCase);
const nonCuUnsupportedCase = await captureCase<MarketCreateData>(
  'marketCreateUnsupportedCountryRegionNonCu',
  marketCreateMutation,
  nonCuUnsupportedVariables,
);
assertUnsupportedCountryRegion(nonCuUnsupportedCountryCode, nonCuUnsupportedCase);
const readAfterNonCuUnsupportedCase = await captureCase(
  'marketsReadAfterUnsupportedCountryRegionNonCu',
  marketsReadAfterRejectedCreateQuery,
  { first: 10 },
);
if (marketNamesFromRead(readAfterNonCuUnsupportedCase).includes(nonCuUnsupportedMarketName)) {
  throw new Error(
    `Rejected ${nonCuUnsupportedCountryCode} market appeared in follow-up markets read: ${nonCuUnsupportedMarketName}`,
  );
}

const probes: CountryProbe[] = [];

for (const countryCode of countryCodes) {
  const probeCase = await captureMarketCreate(countryCode);
  const outcome = classifyProbe(probeCase);
  probes.push({
    countryCode,
    outcome,
    userErrors: userErrors(probeCase),
    errorMessages: errorMessages(probeCase),
  });
}

const unsupportedCountryCodes = probes
  .filter((probe) => probe.outcome === 'unsupported')
  .map((probe) => probe.countryCode)
  .sort();

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cases: [primaryCase, nonCuUnsupportedCase, readAfterNonCuUnsupportedCase],
      evidence: {
        source: 'Live Admin GraphQL CountryCode enum plus per-code marketCreate resolver probes.',
        countryCodeEnumCase: introspectionCase,
        countryCodes,
        unsupportedCountryCodes,
        probes,
      },
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);
await writeFile(generatedModulePath, renderUnsupportedModule(unsupportedCountryCodes), 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      generatedModulePath,
      storeDomain,
      apiVersion,
      probedCountryCodes: countryCodes.length,
      unsupportedCountryCodes,
    },
    null,
    2,
  ),
);
