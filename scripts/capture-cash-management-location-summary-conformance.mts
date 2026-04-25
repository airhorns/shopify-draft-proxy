// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputPath = path.join(outputDir, 'cash-management-location-summary-access-denied.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaInventoryQuery = `#graphql
  query CashManagementLocationSummarySchemaInventory {
    queryRoot: __type(name: "QueryRoot") {
      fields {
        name
        args {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
              }
            }
          }
        }
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
    cashManagementSummary: __type(name: "CashManagementSummary") {
      fields {
        name
        type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
              }
            }
          }
        }
      }
    }
    moneyV2: __type(name: "MoneyV2") {
      fields {
        name
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
  }
`;

const locationsProbeQuery = `#graphql
  query CashManagementLocationSummaryProbeLocations {
    locations(first: 1) {
      nodes {
        id
        name
      }
    }
  }
`;

const summaryQuery = `#graphql
  query CashManagementLocationSummaryProbe($locationId: ID!, $startDate: Date!, $endDate: Date!) {
    cashManagementLocationSummary(locationId: $locationId, startDate: $startDate, endDate: $endDate) {
      cashBalanceAtStart {
        amount
        currencyCode
      }
      cashBalanceAtEnd {
        amount
        currencyCode
      }
      netCash {
        amount
        currencyCode
      }
      totalDiscrepancies {
        amount
        currencyCode
      }
      sessionsOpened
      sessionsClosed
    }
  }
`;

const startDate = '2026-04-01';
const endDate = '2026-04-25';
const unknownLocationId = 'gid://shopify/Location/999999999999';

async function runCase(name, query, variables) {
  const result = await runGraphqlRequest(query, variables);
  return {
    name,
    status: result.status,
    variables,
    response: result.payload,
  };
}

const schemaInventory = await runCase('schemaInventory', schemaInventoryQuery, {});
const locationsProbe = await runCase('locationsProbe', locationsProbeQuery, {});
const selectedLocationId = locationsProbe.response.data?.locations?.nodes?.[0]?.id ?? unknownLocationId;
const selectedLocationName = locationsProbe.response.data?.locations?.nodes?.[0]?.name ?? null;
const invalidStartDateLiteralQuery = `#graphql
  query CashManagementLocationSummaryInvalidDate {
    cashManagementLocationSummary(locationId: "${selectedLocationId}", startDate: "not-a-date", endDate: "${endDate}") {
      sessionsOpened
    }
  }
`;

const behavior = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  operation: 'cashManagementLocationSummary',
  selectedLocation: {
    id: selectedLocationId,
    name: selectedLocationName,
  },
  conclusion:
    'Current conformance credential can introspect CashManagementSummary but cannot read cashManagementLocationSummary data. Keep runtime support unimplemented and do not synthesize balances or session counts until read_cash_tracking plus required POS/retail role evidence is available.',
  schemaInventory: {
    queryRoot:
      schemaInventory.response.data?.queryRoot?.fields?.find(
        (field) => field.name === 'cashManagementLocationSummary',
      ) ?? null,
    cashManagementSummary: schemaInventory.response.data?.cashManagementSummary ?? null,
    moneyV2: schemaInventory.response.data?.moneyV2 ?? null,
  },
  probes: [
    locationsProbe,
    await runCase('knownLocationAccessDenied', summaryQuery, {
      locationId: selectedLocationId,
      startDate,
      endDate,
    }),
    await runCase('unknownLocationAccessDenied', summaryQuery, {
      locationId: unknownLocationId,
      startDate,
      endDate,
    }),
    await runCase('missingLocationIdVariable', summaryQuery, { startDate, endDate }),
    await runCase('missingStartDateVariable', summaryQuery, { locationId: selectedLocationId, endDate }),
    await runCase('missingEndDateVariable', summaryQuery, { locationId: selectedLocationId, startDate }),
    await runCase('invalidStartDateLiteral', invalidStartDateLiteralQuery, {}),
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(behavior, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      storeDomain,
      apiVersion,
      outputPath,
      selectedLocationId,
      accessDeniedMessage:
        behavior.probes.find((probe) => probe.name === 'knownLocationAccessDenied')?.response.errors?.[0]?.message ??
        null,
    },
    null,
    2,
  ),
);
