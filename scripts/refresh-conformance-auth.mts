/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import {
  SHOPIFY_CONFORMANCE_AUTH_PATH,
  buildAdminAuthHeaders,
  refreshConformanceAccessToken,
  resolveDefaultAppEnvPath,
} from './shopify-conformance-auth.mts';
import { runAdminGraphqlRequest } from './conformance-graphql-client.ts';

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];

if (!storeDomain) {
  console.error('Missing required environment variable: SHOPIFY_CONFORMANCE_STORE_DOMAIN');
  process.exit(1);
}

const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'] || `https://${storeDomain}`;
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const credentialPath = SHOPIFY_CONFORMANCE_AUTH_PATH;
const appEnvPath = resolveDefaultAppEnvPath();

function tokenSuffix(token: unknown): string | null {
  return typeof token === 'string' && token.length >= 6 ? token.slice(-6) : null;
}

function tokenFamily(token: unknown): string {
  if (typeof token !== 'string' || token.length === 0) {
    return 'unknown';
  }

  const match = token.match(/^(shp[a-z]+)_/i);
  return match ? match[1].toLowerCase() : 'unknown';
}

function recommendedNextStep(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (/active refresh_token/i.test(message)) {
    return 'manual-store-auth-reauthorization';
  }
  if (/credential file not found|missing refresh_token|missing client_id|missing shop\/store/i.test(message)) {
    return 'restore-shared-home-credential-or-reauthorize';
  }
  if (/Shopify app env file not found|SHOPIFY_API_SECRET is missing/i.test(message)) {
    return 'restore-shopify-app-env-or-set-SHOPIFY_CONFORMANCE_APP_ENV_PATH';
  }

  return 'retry-refresh-or-investigate-auth-state';
}

async function runProbe(accessToken: string) {
  const result = await runAdminGraphqlRequest(
    { adminOrigin, apiVersion, headers: buildAdminAuthHeaders(accessToken) },
    `#graphql
        query RefreshProbe {
          shop {
            id
            name
            myshopifyDomain
          }
        }
      `,
  );

  return {
    ...result,
    ok: result.status >= 200 && result.status < 300 && !result.payload.errors,
  };
}

let refreshedAuth: Record<string, unknown>;

try {
  refreshedAuth = await refreshConformanceAccessToken({ credentialPath, appEnvPath });
} catch (error) {
  console.error(
    JSON.stringify(
      {
        ok: false,
        message: error instanceof Error ? error.message : String(error),
        recommendedNextStep: recommendedNextStep(error),
        credentialPath,
        appEnvPath,
      },
      null,
      2,
    ),
  );
  process.exit(1);
}

const accessToken = refreshedAuth['access_token'];
if (typeof accessToken !== 'string' || accessToken.length === 0) {
  console.error(
    JSON.stringify(
      {
        ok: false,
        message: `Refreshed credential at ${credentialPath} did not include access_token.`,
        recommendedNextStep: 'retry-refresh-or-investigate-auth-state',
        credentialPath,
        appEnvPath,
      },
      null,
      2,
    ),
  );
  process.exit(1);
}

const probe = await runProbe(accessToken);
if (!probe.ok) {
  console.error(
    JSON.stringify(
      {
        ok: false,
        message: 'Token refresh persisted to the shared home credential, but verification probe failed.',
        probe,
        credentialPath,
        appEnvPath,
        tokenFamily: refreshedAuth['token_family'] ?? tokenFamily(accessToken),
        accessTokenSuffix: tokenSuffix(accessToken),
      },
      null,
      2,
    ),
  );
  process.exit(1);
}

console.log(
  JSON.stringify(
    {
      ok: true,
      refreshed: true,
      credentialPath,
      appEnvPath,
      tokenFamily: refreshedAuth['token_family'] ?? tokenFamily(accessToken),
      accessTokenSuffix: tokenSuffix(accessToken),
      refreshTokenSuffix: tokenSuffix(refreshedAuth['refresh_token']),
      expiresAt: refreshedAuth['expires_at'] ?? null,
      refreshTokenExpiresAt: refreshedAuth['refresh_token_expires_at'] ?? null,
      probeStatus: probe.status,
      shop: probe.payload?.data?.shop ?? null,
    },
    null,
    2,
  ),
);
