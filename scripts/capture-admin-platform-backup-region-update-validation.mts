// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'admin-platform');
const outputPath = path.join(outputDir, 'admin-platform-backup-region-update-validation.json');

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

async function readRequestDocument(name) {
  return readFile(path.join('config', 'parity-requests', 'admin-platform', name), 'utf8');
}

const userErrorTypenameMutation = await readRequestDocument(
  'admin-platform-backup-region-update-validation-typename.graphql',
);
const missingCountryCodeMutation = await readRequestDocument(
  'admin-platform-backup-region-update-validation-missing-country-code.graphql',
);
const nullCountryCodeMutation = await readRequestDocument(
  'admin-platform-backup-region-update-validation-null-country-code.graphql',
);
const numericCountryCodeMutation = await readRequestDocument(
  'admin-platform-backup-region-update-validation-numeric-country-code.graphql',
);

function assertStatusOk(name, capture) {
  if (capture.status !== 200) {
    throw new Error(`${name} expected HTTP 200, got ${JSON.stringify(capture)}`);
  }
}

function assertMarketUserErrorTypename(name, capture) {
  assertStatusOk(name, capture);
  const payload = capture.payload?.data?.backupRegionUpdate;
  const error = payload?.userErrors?.[0];
  if (
    payload?.backupRegion !== null ||
    error?.__typename !== 'MarketUserError' ||
    error?.code !== 'REGION_NOT_FOUND' ||
    error?.message !== 'Region not found.'
  ) {
    throw new Error(`${name} expected MarketUserError REGION_NOT_FOUND, got ${JSON.stringify(payload)}`);
  }
}

function assertTopLevelInputError(name, capture, expectedCode, expectedMessage) {
  assertStatusOk(name, capture);
  const error = capture.payload?.errors?.[0];
  if (capture.payload?.data !== undefined || error?.extensions?.code !== expectedCode) {
    throw new Error(`${name} expected top-level ${expectedCode} and no data, got ${JSON.stringify(capture.payload)}`);
  }
  if (error?.message !== expectedMessage) {
    throw new Error(`${name} expected message ${expectedMessage}, got ${JSON.stringify(error?.message)}`);
  }
}

const captures = {
  userErrorTypename: {
    query: userErrorTypenameMutation,
    result: await runGraphqlCapture(userErrorTypenameMutation),
  },
  missingCountryCode: {
    query: missingCountryCodeMutation,
    result: await runGraphqlCapture(missingCountryCodeMutation),
  },
  nullCountryCode: {
    query: nullCountryCodeMutation,
    result: await runGraphqlCapture(nullCountryCodeMutation),
  },
  numericCountryCode: {
    query: numericCountryCodeMutation,
    result: await runGraphqlCapture(numericCountryCodeMutation),
  },
};

assertMarketUserErrorTypename('userErrorTypename', captures.userErrorTypename.result);
assertTopLevelInputError(
  'missingCountryCode',
  captures.missingCountryCode.result,
  'missingRequiredInputObjectAttribute',
  "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' is required. Expected type CountryCode!",
);
assertTopLevelInputError(
  'nullCountryCode',
  captures.nullCountryCode.result,
  'argumentLiteralsIncompatible',
  "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value (null). Expected type 'CountryCode!'.",
);
assertTopLevelInputError(
  'numericCountryCode',
  captures.numericCountryCode.result,
  'argumentLiteralsIncompatible',
  "Argument 'countryCode' on InputObject 'BackupRegionUpdateInput' has an invalid value (42). Expected type 'CountryCode!'.",
);

const captureOutput = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  notes:
    'HAR-745 validation-only capture for backupRegionUpdate: well-formed unknown CountryCode ZZ returns MarketUserError REGION_NOT_FOUND, while missing, null, and non-enum literal region.countryCode fail GraphQL input-object coercion before resolver execution.',
  captures,
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(captureOutput, null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
