// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const supportedNonCaCountryCode = 'AE';
const restoreCountryCode = 'CA';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, 'admin-platform-backup-region-update-extended.json');

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

const backupRegionUpdateRestoreMutation = `#graphql
  mutation BackupRegionUpdateRestore {
    backupRegionUpdate(region: { countryCode: ${restoreCountryCode} }) {
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

const captures = {};
let captureError = null;

try {
  captures.setupCurrentBackupRegion = {
    query: backupRegionUpdateRestoreMutation,
    result: await runGraphqlCapture(backupRegionUpdateRestoreMutation),
  };
  assertSuccessfulUpdateCode('setupCurrentBackupRegion', captures.setupCurrentBackupRegion.result, restoreCountryCode);

  captures.backupRegionAfterIdempotentUpdate = {
    query: backupRegionQuery,
    result: await runGraphqlCapture(backupRegionQuery),
  };
  assertBackupRegionReadCode(
    'backupRegionAfterIdempotentUpdate',
    captures.backupRegionAfterIdempotentUpdate.result,
    restoreCountryCode,
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

  captures.backupRegionUpdateInvalid = {
    query: backupRegionUpdateInvalidMutation,
    result: await runGraphqlCapture(backupRegionUpdateInvalidMutation),
  };
  assertInvalidRegion('backupRegionUpdateInvalid', captures.backupRegionUpdateInvalid.result);
} catch (err) {
  captureError = err;
} finally {
  captures.cleanupBackupRegion = {
    query: backupRegionUpdateRestoreMutation,
    result: await runGraphqlCapture(backupRegionUpdateRestoreMutation),
  };
}

assertSuccessfulUpdateCode('cleanupBackupRegion', captures.cleanupBackupRegion.result, restoreCountryCode);
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
  restoredCountryCode: restoreCountryCode,
  notes:
    'HAR-737 captures backupRegionUpdate current-region baseline, harry-test-heelo non-CA success, read-after-write, unknown-region validation, and cleanup back to CA. Live omitted/null invocations currently return Shopify INTERNAL_SERVER_ERROR on this store/API, so expected omitted/null current-state parity is derived from the captured current backupRegion and empty successful-update userErrors, matching the source resolver contract cited by HAR-737.',
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
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(captureOutput, null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
