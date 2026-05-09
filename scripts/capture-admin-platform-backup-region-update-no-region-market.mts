// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const targetCountryCode = 'AT';
const marketReadVariables = { first: 20 };

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, 'admin-platform-backup-region-update-no-region-market.json');
const marketCatalogReadQuery = await readFile('config/parity-requests/markets/markets-catalog-read.graphql', 'utf8');
const backupRegionUpdateNoRegionMarketMutation = await readFile(
  'config/parity-requests/admin-platform/admin-platform-backup-region-update-no-region-market.graphql',
  'utf8',
);

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlCapture(query, variables = {}) {
  const result = await runGraphqlRequest(query, variables);
  return {
    status: result.status,
    payload: result.payload,
  };
}

const backupRegionQuery = `#graphql
  query BackupRegionRead {
    backupRegion {
      __typename
      id
      name
      ... on MarketRegionCountry {
        code
      }
    }
  }
`;

const marketScanQuery = `#graphql
  query BackupRegionNoRegionMarketScan {
    backupRegion {
      __typename
      ... on MarketRegionCountry {
        code
      }
    }
    markets(first: 20) {
      nodes {
        id
        name
        status
        type
        conditions {
          regionsCondition {
            regions(first: 250) {
              nodes {
                ... on MarketRegionCountry {
                  code
                }
              }
            }
          }
        }
      }
    }
  }
`;

const marketConditionsUpdateMutation = `#graphql
  mutation BackupRegionNoRegionMarketConditionsUpdate($id: ID!, $input: MarketUpdateInput!) {
    marketUpdate(id: $id, input: $input) {
      market {
        id
        name
        status
        type
        conditions {
          regionsCondition {
            regions(first: 250) {
              nodes {
                ... on MarketRegionCountry {
                  code
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

function captureData(capture) {
  return capture.payload?.data ?? {};
}

function assertNoTopLevelErrors(name, capture) {
  const errors = capture.payload?.errors ?? [];
  if (capture.status !== 200 || errors.length > 0) {
    throw new Error(`${name} returned unexpected top-level GraphQL errors: ${JSON.stringify(capture)}`);
  }
}

function assertNoMarketUpdateErrors(name, capture) {
  assertNoTopLevelErrors(name, capture);
  const errors = captureData(capture).marketUpdate?.userErrors ?? [];
  if (errors.length > 0) {
    throw new Error(`${name} returned marketUpdate userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertInvalidRegion(name, capture) {
  assertNoTopLevelErrors(name, capture);
  const update = captureData(capture).backupRegionUpdate;
  const error = update?.userErrors?.[0];
  if (
    update?.backupRegion !== null ||
    error?.__typename !== 'MarketUserError' ||
    error?.field?.[0] !== 'region' ||
    error?.code !== 'REGION_NOT_FOUND' ||
    error?.message !== 'Region not found.'
  ) {
    throw new Error(`${name} expected REGION_NOT_FOUND, got ${JSON.stringify(update)}`);
  }
}

function marketRegionCodes(market) {
  return market?.conditions?.regionsCondition?.regions?.nodes?.map((node) => node?.code).filter(Boolean) ?? [];
}

function findTargetMarket(scanCapture) {
  const markets = captureData(scanCapture).markets?.nodes ?? [];
  return markets.find((market) => {
    return (
      market?.status === 'ACTIVE' &&
      market?.type === 'REGION' &&
      marketRegionCodes(market).includes(targetCountryCode) &&
      marketRegionCodes(market).length > 1
    );
  });
}

function assertMarketContains(name, market, expected) {
  const codes = marketRegionCodes(market);
  if (codes.includes(expected) === false) {
    throw new Error(`${name} expected ${expected} in market regions, got ${JSON.stringify(codes)}`);
  }
}

function assertMarketLacks(name, market, expected) {
  const codes = marketRegionCodes(market);
  if (codes.includes(expected)) {
    throw new Error(`${name} expected ${expected} to be absent from market regions, got ${JSON.stringify(codes)}`);
  }
}

const captures = {};
let removedTargetCountry = false;
let captureError = null;
let targetMarketId = null;

try {
  captures.setupScan = {
    query: marketScanQuery,
    result: await runGraphqlCapture(marketScanQuery),
  };
  assertNoTopLevelErrors('setupScan', captures.setupScan.result);

  const targetMarket = findTargetMarket(captures.setupScan.result);
  if (!targetMarket?.id) {
    throw new Error(`Could not find active multi-country region market containing ${targetCountryCode}`);
  }
  targetMarketId = targetMarket.id;
  assertMarketContains('setupScan', targetMarket, targetCountryCode);

  captures.backupRegionBefore = {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  };
  assertNoTopLevelErrors('backupRegionBefore', captures.backupRegionBefore.result);

  captures.removeCountryFromMarket = {
    query: marketConditionsUpdateMutation,
    variables: {
      id: targetMarketId,
      input: {
        conditions: {
          conditionsToDelete: {
            regionsCondition: {
              regions: [{ countryCode: targetCountryCode }],
            },
          },
        },
      },
    },
    result: await runGraphqlCapture(marketConditionsUpdateMutation, {
      id: targetMarketId,
      input: {
        conditions: {
          conditionsToDelete: {
            regionsCondition: {
              regions: [{ countryCode: targetCountryCode }],
            },
          },
        },
      },
    }),
  };
  assertNoMarketUpdateErrors('removeCountryFromMarket', captures.removeCountryFromMarket.result);
  removedTargetCountry = true;
  assertMarketLacks(
    'removeCountryFromMarket',
    captureData(captures.removeCountryFromMarket.result).marketUpdate?.market,
    targetCountryCode,
  );

  captures.marketAfterRemoval = {
    query: marketCatalogReadQuery,
    variables: marketReadVariables,
    result: await runGraphqlCapture(marketCatalogReadQuery, marketReadVariables),
  };
  assertNoTopLevelErrors('marketAfterRemoval', captures.marketAfterRemoval.result);

  captures.backupRegionUpdateNoRegionMarket = {
    query: backupRegionUpdateNoRegionMarketMutation,
    result: await runGraphqlCapture(backupRegionUpdateNoRegionMarketMutation),
  };
  assertInvalidRegion('backupRegionUpdateNoRegionMarket', captures.backupRegionUpdateNoRegionMarket.result);
} catch (err) {
  captureError = err;
} finally {
  if (targetMarketId && removedTargetCountry) {
    captures.restoreCountryToMarket = {
      query: marketConditionsUpdateMutation,
      variables: {
        id: targetMarketId,
        input: {
          conditions: {
            conditionsToAdd: {
              regionsCondition: {
                regions: [{ countryCode: targetCountryCode }],
              },
            },
          },
        },
      },
      result: await runGraphqlCapture(marketConditionsUpdateMutation, {
        id: targetMarketId,
        input: {
          conditions: {
            conditionsToAdd: {
              regionsCondition: {
                regions: [{ countryCode: targetCountryCode }],
              },
            },
          },
        },
      }),
    };
    assertNoMarketUpdateErrors('restoreCountryToMarket', captures.restoreCountryToMarket.result);
    assertMarketContains(
      'restoreCountryToMarket',
      captureData(captures.restoreCountryToMarket.result).marketUpdate?.market,
      targetCountryCode,
    );
  }

  captures.backupRegionAfterCleanup = {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  };
}

if (captureError) {
  throw captureError;
}

const captureOutput = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  targetCountryCode,
  targetMarketId,
  notes:
    'Records backupRegionUpdate returning REGION_NOT_FOUND when a captured backup-region country has been temporarily removed from every active region market. The capture removes AT from a multi-country active region market, records the failed backupRegionUpdate response, then restores AT to the market.',
  captures,
  upstreamCalls: [
    {
      operationName: 'MarketsCatalogRead',
      variables: marketReadVariables,
      query: marketCatalogReadQuery,
      response: {
        status: captures.marketAfterRemoval.result.status,
        body: captures.marketAfterRemoval.result.payload,
      },
    },
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(captureOutput, null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
