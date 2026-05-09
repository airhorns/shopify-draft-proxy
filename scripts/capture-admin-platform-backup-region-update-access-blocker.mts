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
const outputPath = path.join(outputDir, 'admin-platform-backup-region-update-access-blocker.json');

const backupRegionUpdateQuery = await readFile(
  'config/parity-requests/admin-platform/admin-platform-backup-region-update-idempotent.graphql',
  'utf8',
);

const createDelegateTokenMutation = `#graphql
  mutation BackupRegionUpdateAccessBlockerDelegateSetup {
    delegateAccessTokenCreate(input: { delegateAccessScope: ["read_products"], expiresIn: 300 }) {
      delegateAccessToken {
        accessToken
        accessScopes
        createdAt
        expiresIn
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const destroyDelegateTokenMutation = `#graphql
  mutation BackupRegionUpdateAccessBlockerDelegateCleanup($token: String!) {
    delegateAccessTokenDestroy(accessToken: $token) {
      status
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function captureClient(accessToken: string) {
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(accessToken),
  });
}

async function runGraphqlCapture(accessToken: string, query: string, variables = {}) {
  const { runGraphqlRequest } = captureClient(accessToken);
  const result = await runGraphqlRequest(query, variables);
  return {
    status: result.status,
    payload: result.payload,
  };
}

function readDelegateToken(capture: unknown): string {
  const token = capture?.payload?.data?.delegateAccessTokenCreate?.delegateAccessToken?.accessToken;
  if (typeof token !== 'string' || token.length === 0) {
    throw new Error(`delegateAccessTokenCreate did not return a token: ${JSON.stringify(capture)}`);
  }
  return token;
}

function redactDelegateToken(capture: unknown): unknown {
  const payload = JSON.parse(JSON.stringify(capture));
  const token = payload?.payload?.data?.delegateAccessTokenCreate?.delegateAccessToken;
  if (token && typeof token === 'object') {
    token.accessToken = '[redacted-live-delegate-token]';
  }
  return payload;
}

function assertAccessDenied(capture: unknown) {
  const payload = capture?.payload;
  const update = payload?.data?.backupRegionUpdate;
  const errors = payload?.errors;
  const firstError = Array.isArray(errors) ? errors[0] : undefined;
  if (
    capture?.status !== 200 ||
    update !== null ||
    !firstError ||
    firstError?.path?.[0] !== 'backupRegionUpdate' ||
    firstError?.extensions?.code !== 'ACCESS_DENIED'
  ) {
    throw new Error(`backupRegionUpdate did not return ACCESS_DENIED: ${JSON.stringify(capture)}`);
  }
}

let delegateToken: string | undefined;
let accessDeniedCapture: unknown;
let setupCapture: unknown;
let cleanupCapture: unknown;
let captureError: unknown;

try {
  setupCapture = await runGraphqlCapture(adminAccessToken, createDelegateTokenMutation);
  delegateToken = readDelegateToken(setupCapture);
  accessDeniedCapture = await runGraphqlCapture(delegateToken, backupRegionUpdateQuery);
  assertAccessDenied(accessDeniedCapture);
} catch (err) {
  captureError = err;
} finally {
  if (delegateToken) {
    cleanupCapture = await runGraphqlCapture(adminAccessToken, destroyDelegateTokenMutation, { token: delegateToken });
  }
}

if (captureError) {
  throw captureError;
}

const captureOutput = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  notes:
    'Captures backupRegionUpdate pre-resolve Markets access denial by issuing the mutation through a short-lived delegate token that only has read_products. The delegate token setup response is redacted and the cleanup response proves the token was destroyed after capture.',
  requests: {
    delegateSetup: createDelegateTokenMutation,
    backupRegionUpdate: backupRegionUpdateQuery,
    delegateCleanup: destroyDelegateTokenMutation,
  },
  captures: {
    delegateSetup: redactDelegateToken(setupCapture),
    backupRegionUpdateAccessDenied: {
      query: backupRegionUpdateQuery,
      result: accessDeniedCapture,
    },
    delegateCleanup: {
      query: destroyDelegateTokenMutation,
      result: cleanupCapture,
    },
  },
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(captureOutput, null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
