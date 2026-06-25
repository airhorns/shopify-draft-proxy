// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const supportedNonCaCountryCode = 'AE';
const supportedUsCountryCode = 'US';
const supportedOutsideOldTableCountryCode = 'JP';
const baselineCountryCode = 'CA';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, 'admin-platform-backup-region-update-extended.json');
const backupRegionUpdateUsMutation = await readFile(
  'config/parity-requests/admin-platform/admin-platform-backup-region-update-us.graphql',
  'utf8',
);
const backupRegionUpdateOutsideOldTableMutation = await readFile(
  'config/parity-requests/admin-platform/admin-platform-backup-region-update-jp.graphql',
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

const backupRegionSelection = `
  backupRegion {
    __typename
    id
    name
    ... on MarketRegionCountry {
      code
    }
  }
  userErrors {
    field
    message
    code
  }
`;

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

const backupRegionAccessScopesQuery =
  'query BackupRegionAccessScopes { currentAppInstallation { accessScopes { handle } } }';

const backupRegionCurrentHydrateQuery = `query BackupRegionCurrentHydrate {
  backupRegion {
    __typename
    id
    name
    ... on MarketRegionCountry {
      code
    }
  }
}`;

const backupRegionMarketsHydrateQuery = `query BackupRegionMarketsHydrate($first: Int!, $regionsFirst: Int!) {
  markets(first: $first) {
    nodes {
      id
      name
      handle
      status
      type
      conditions {
        conditionTypes
        regionsCondition {
          regions(first: $regionsFirst) {
            nodes {
              __typename
              id
              name
              ... on MarketRegionCountry {
                code
              }
            }
          }
        }
      }
    }
  }
}`;

const backupRegionUpdateRestoreMutation = `#graphql
  mutation BackupRegionUpdateRestore {
    backupRegionUpdate(region: { countryCode: ${baselineCountryCode} }) {
      ${backupRegionSelection}
    }
  }
`;

const backupRegionUpdateOmittedMutation = `#graphql
  mutation BackupRegionUpdateOmitted {
    backupRegionUpdate {
      ${backupRegionSelection}
    }
  }
`;

const backupRegionUpdateNullMutation = `#graphql
  mutation BackupRegionUpdateNull {
    backupRegionUpdate(region: null) {
      ${backupRegionSelection}
    }
  }
`;

const backupRegionUpdateNonCaMutation = `#graphql
  mutation BackupRegionUpdateNonCa {
    backupRegionUpdate(region: { countryCode: ${supportedNonCaCountryCode} }) {
      ${backupRegionSelection}
    }
  }
`;

const backupRegionUpdateInvalidMutation = `#graphql
  mutation BackupRegionUpdateInvalid {
    backupRegionUpdate(region: { countryCode: ZZ }) {
      ${backupRegionSelection}
    }
  }
`;

const marketCreateMutation = `#graphql
  mutation BackupRegionUpdateTemporaryMarketCreate($input: MarketCreateInput!) {
    marketCreate(input: $input) {
      market {
        id
        name
        status
        enabled
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const marketDeleteMutation = `#graphql
  mutation BackupRegionUpdateTemporaryMarketDelete($id: ID!) {
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

function captureData(capture) {
  return capture.payload?.data ?? {};
}

function assertNoTopLevelErrors(name, capture) {
  const errors = capture.payload?.errors ?? [];
  if (capture.status !== 200 || errors.length > 0) {
    throw new Error(`${name} returned unexpected top-level GraphQL errors: ${JSON.stringify(capture)}`);
  }
}

function assertBackupRegionReadCode(name, capture, code) {
  assertNoTopLevelErrors(name, capture);
  const backupRegion = captureData(capture).backupRegion;
  if (backupRegion?.code !== code) {
    throw new Error(`${name} expected backupRegion.code ${code}, got ${JSON.stringify(backupRegion)}`);
  }
}

function assertSuccessfulUpdateCode(name, capture, code) {
  assertNoTopLevelErrors(name, capture);
  const update = captureData(capture).backupRegionUpdate;
  if (update?.backupRegion?.code !== code || (update?.userErrors ?? []).length !== 0) {
    throw new Error(`${name} expected successful ${code} update, got ${JSON.stringify(update)}`);
  }
}

function assertInvalidRegion(name, capture) {
  assertNoTopLevelErrors(name, capture);
  const update = captureData(capture).backupRegionUpdate;
  const error = update?.userErrors?.[0];
  if (update?.backupRegion !== null || error?.code !== 'REGION_NOT_FOUND' || error?.message !== 'Region not found.') {
    throw new Error(`${name} expected REGION_NOT_FOUND, got ${JSON.stringify(update)}`);
  }
}

function isInvalidRegion(capture) {
  assertNoTopLevelErrors('backupRegionUpdate probe', capture);
  const update = captureData(capture).backupRegionUpdate;
  const error = update?.userErrors?.[0];
  return update?.backupRegion === null && error?.code === 'REGION_NOT_FOUND' && error?.message === 'Region not found.';
}

function assertNoUserErrors(name, capture, root) {
  assertNoTopLevelErrors(name, capture);
  const errors = captureData(capture)[root]?.userErrors ?? [];
  if (errors.length > 0) {
    throw new Error(`${name} returned ${root} userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertAccessScopesIncludeMarkets(name, capture) {
  assertNoTopLevelErrors(name, capture);
  const handles = captureData(capture).currentAppInstallation?.accessScopes?.map((scope) => scope?.handle) ?? [];
  if (!handles.includes('read_markets') || !handles.includes('write_markets')) {
    throw new Error(`${name} expected read_markets and write_markets scopes, got ${JSON.stringify(handles)}`);
  }
}

function createdMarketId(capture) {
  return captureData(capture).marketCreate?.market?.id ?? null;
}

function upstreamCall(operationName, query, variables, capture) {
  return {
    operationName,
    variables,
    query,
    response: {
      status: capture.result.status,
      body: capture.result.payload,
    },
  };
}

const captures = {};
let captureError = null;
let cleanupCountryCode = supportedNonCaCountryCode;
const temporaryMarketIds = [];

function backupRegionUpdateMutationForCountry(countryCode, operationName) {
  return `#graphql
    mutation ${operationName} {
      backupRegionUpdate(region: { countryCode: ${countryCode} }) {
        ${backupRegionSelection}
      }
    }
  `;
}

async function createTemporaryMarketForCountry(captureName, countryCode) {
  const variables = {
    input: {
      name: `Backup Region ${countryCode} ${Date.now()}`,
      status: 'ACTIVE',
      enabled: true,
      conditions: {
        regionsCondition: {
          regions: [{ countryCode }],
        },
      },
    },
  };
  captures[captureName] = {
    query: marketCreateMutation,
    variables,
    result: await runGraphqlCapture(marketCreateMutation, variables),
  };
  assertNoUserErrors(captureName, captures[captureName].result, 'marketCreate');
  const marketId = createdMarketId(captures[captureName].result);
  if (!marketId) {
    throw new Error(`${captureName} did not return a market id: ${JSON.stringify(captures[captureName].result)}`);
  }
  temporaryMarketIds.push(marketId);
  return marketId;
}

async function captureSuccessfulCountryUpdate(captureName, countryCode, mutation, temporaryMarketCaptureName) {
  const initial = {
    query: mutation,
    result: await runGraphqlCapture(mutation),
  };
  if (isInvalidRegion(initial.result)) {
    captures[`${captureName}BeforeTemporaryMarket`] = initial;
    await createTemporaryMarketForCountry(temporaryMarketCaptureName, countryCode);
    captures[captureName] = {
      query: mutation,
      result: await runGraphqlCapture(mutation),
    };
  } else {
    captures[captureName] = initial;
  }
  assertSuccessfulUpdateCode(captureName, captures[captureName].result, countryCode);
}

try {
  captures.originalBackupRegion = {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  };
  assertNoTopLevelErrors('originalBackupRegion', captures.originalBackupRegion.result);
  cleanupCountryCode = captureData(captures.originalBackupRegion.result).backupRegion?.code ?? cleanupCountryCode;

  await captureSuccessfulCountryUpdate(
    'setupCurrentBackupRegion',
    baselineCountryCode,
    backupRegionUpdateRestoreMutation,
    'createTemporaryBaselineMarket',
  );

  captures.backupRegionAfterIdempotentUpdate = {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  };
  assertBackupRegionReadCode(
    'backupRegionAfterIdempotentUpdate',
    captures.backupRegionAfterIdempotentUpdate.result,
    baselineCountryCode,
  );

  captures.upstreamBackupRegionAccessScopes = {
    query: backupRegionAccessScopesQuery,
    result: await runGraphqlCapture(backupRegionAccessScopesQuery),
  };
  assertAccessScopesIncludeMarkets(
    'upstreamBackupRegionAccessScopes',
    captures.upstreamBackupRegionAccessScopes.result,
  );

  captures.upstreamBackupRegionCurrentHydrate = {
    query: backupRegionCurrentHydrateQuery,
    result: await runGraphqlCapture(backupRegionCurrentHydrateQuery),
  };
  assertBackupRegionReadCode(
    'upstreamBackupRegionCurrentHydrate',
    captures.upstreamBackupRegionCurrentHydrate.result,
    baselineCountryCode,
  );

  captures.backupRegionUpdateOmittedLive = {
    query: backupRegionUpdateOmittedMutation,
    result: await runGraphqlCapture(backupRegionUpdateOmittedMutation),
  };

  captures.backupRegionUpdateNullLive = {
    query: backupRegionUpdateNullMutation,
    result: await runGraphqlCapture(backupRegionUpdateNullMutation),
  };

  captures.backupRegionUpdateNonCa = {
    query: backupRegionUpdateNonCaMutation,
    result: await runGraphqlCapture(backupRegionUpdateNonCaMutation),
  };
  assertSuccessfulUpdateCode(
    'backupRegionUpdateNonCa',
    captures.backupRegionUpdateNonCa.result,
    supportedNonCaCountryCode,
  );

  captures.backupRegionAfterNonCaUpdate = {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  };
  assertBackupRegionReadCode(
    'backupRegionAfterNonCaUpdate',
    captures.backupRegionAfterNonCaUpdate.result,
    supportedNonCaCountryCode,
  );

  await captureSuccessfulCountryUpdate(
    'backupRegionUpdateUs',
    supportedUsCountryCode,
    backupRegionUpdateUsMutation,
    'createTemporaryUsMarket',
  );

  captures.backupRegionAfterUsUpdate = {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  };
  assertBackupRegionReadCode(
    'backupRegionAfterUsUpdate',
    captures.backupRegionAfterUsUpdate.result,
    supportedUsCountryCode,
  );

  await captureSuccessfulCountryUpdate(
    'backupRegionUpdateOutsideOldTable',
    supportedOutsideOldTableCountryCode,
    backupRegionUpdateOutsideOldTableMutation,
    'createTemporaryOutsideOldTableMarket',
  );

  captures.backupRegionAfterOutsideOldTableUpdate = {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  };
  assertBackupRegionReadCode(
    'backupRegionAfterOutsideOldTableUpdate',
    captures.backupRegionAfterOutsideOldTableUpdate.result,
    supportedOutsideOldTableCountryCode,
  );

  captures.upstreamBackupRegionMarketsHydrate = {
    query: backupRegionMarketsHydrateQuery,
    variables: { first: 250, regionsFirst: 250 },
    result: await runGraphqlCapture(backupRegionMarketsHydrateQuery, { first: 250, regionsFirst: 250 }),
  };
  assertNoTopLevelErrors('upstreamBackupRegionMarketsHydrate', captures.upstreamBackupRegionMarketsHydrate.result);

  captures.backupRegionUpdateInvalid = {
    query: backupRegionUpdateInvalidMutation,
    result: await runGraphqlCapture(backupRegionUpdateInvalidMutation),
  };
  assertInvalidRegion('backupRegionUpdateInvalid', captures.backupRegionUpdateInvalid.result);
} catch (err) {
  captureError = err;
} finally {
  const cleanupBackupRegionMutation = backupRegionUpdateMutationForCountry(
    cleanupCountryCode,
    'BackupRegionUpdateRestoreOriginal',
  );
  captures.cleanupBackupRegion = {
    query: cleanupBackupRegionMutation,
    result: await runGraphqlCapture(cleanupBackupRegionMutation),
  };
  for (const [index, marketId] of temporaryMarketIds.entries()) {
    const variables = { id: marketId };
    captures[`cleanupTemporaryMarket${index + 1}`] = {
      query: marketDeleteMutation,
      variables,
      result: await runGraphqlCapture(marketDeleteMutation, variables),
    };
    try {
      assertNoUserErrors(
        `cleanupTemporaryMarket${index + 1}`,
        captures[`cleanupTemporaryMarket${index + 1}`].result,
        'marketDelete',
      );
    } catch (err) {
      captureError ??= err;
    }
  }
}

assertSuccessfulUpdateCode('cleanupBackupRegion', captures.cleanupBackupRegion.result, cleanupCountryCode);
if (captureError) {
  throw captureError;
}

const currentBackupRegion = captures.backupRegionAfterIdempotentUpdate.result.payload.data.backupRegion;
const emptyUserErrors = captures.setupCurrentBackupRegion.result.payload.data.backupRegionUpdate.userErrors;
const captureOutput = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  supportedNonCaCountryCode,
  supportedUsCountryCode,
  supportedOutsideOldTableCountryCode,
  baselineCountryCode,
  restoredCountryCode: cleanupCountryCode,
  temporaryMarketIds,
  notes:
    'Captures backupRegionUpdate current-region baseline, non-CA success, US success, JP success outside the old captured country table, read-after-write, unknown-region validation, and cleanup back to the original live backup region. The recorder creates temporary region markets when the live store does not already cover a target country and deletes them after restoring the original backup region. Live omitted/null invocations currently return Shopify INTERNAL_SERVER_ERROR on this store/API, so expected omitted/null current-state parity is derived from the captured current backupRegion and empty successful-update userErrors.',
  captures,
  expected: {
    backupRegionUpdateOmitted: {
      backupRegion: currentBackupRegion,
      userErrors: emptyUserErrors,
    },
    backupRegionUpdateNull: {
      backupRegion: currentBackupRegion,
      userErrors: emptyUserErrors,
    },
  },
  upstreamCalls: [
    upstreamCall(
      'BackupRegionAccessScopes',
      backupRegionAccessScopesQuery,
      {},
      captures.upstreamBackupRegionAccessScopes,
    ),
    upstreamCall(
      'BackupRegionCurrentHydrate',
      backupRegionCurrentHydrateQuery,
      {},
      captures.upstreamBackupRegionCurrentHydrate,
    ),
    upstreamCall(
      'BackupRegionAccessScopes',
      backupRegionAccessScopesQuery,
      {},
      captures.upstreamBackupRegionAccessScopes,
    ),
    upstreamCall(
      'BackupRegionAccessScopes',
      backupRegionAccessScopesQuery,
      {},
      captures.upstreamBackupRegionAccessScopes,
    ),
    upstreamCall(
      'BackupRegionMarketsHydrate',
      backupRegionMarketsHydrateQuery,
      { first: 250, regionsFirst: 250 },
      captures.upstreamBackupRegionMarketsHydrate,
    ),
    upstreamCall(
      'BackupRegionAccessScopes',
      backupRegionAccessScopesQuery,
      {},
      captures.upstreamBackupRegionAccessScopes,
    ),
    upstreamCall(
      'BackupRegionAccessScopes',
      backupRegionAccessScopesQuery,
      {},
      captures.upstreamBackupRegionAccessScopes,
    ),
    upstreamCall(
      'BackupRegionAccessScopes',
      backupRegionAccessScopesQuery,
      {},
      captures.upstreamBackupRegionAccessScopes,
    ),
    upstreamCall(
      'BackupRegionMarketsHydrate',
      backupRegionMarketsHydrateQuery,
      { first: 250, regionsFirst: 250 },
      captures.upstreamBackupRegionMarketsHydrate,
    ),
  ],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(captureOutput, null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
