/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import path from 'node:path';
import { readFile, writeFile } from 'node:fs/promises';

import { classifyRefreshFailure, resolveRefreshClientId } from './refresh-conformance-auth-lib.mjs';

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const appHandle = process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] || 'hermes-conformance-products';

if (!storeDomain) {
  console.error('Missing required environment variable: SHOPIFY_CONFORMANCE_STORE_DOMAIN');
  process.exit(1);
}

const manualStoreAuthTokenPath = path.resolve('.manual-store-auth-token.json');
const manualStoreAuthPkcePath = path.resolve('.manual-store-auth-pkce.json');
const manualStoreAuthPath = path.resolve('.manual-store-auth.json');
const envPath = path.resolve('.env');
const appEnvCandidates = [
  process.env['SHOPIFY_CONFORMANCE_APP_ENV_PATH'],
  path.join('/tmp/shopify-conformance-app', appHandle, '.env'),
  path.join('/tmp/shopify-conformance-app', 'hermes-conformance-products', '.env'),
].filter(Boolean);

function extractTokenFamily(token) {
  if (typeof token !== 'string' || token.length === 0) {
    return 'unknown';
  }

  const match = token.match(/^(shp[a-z]+)_/i);
  return match ? match[1].toLowerCase() : 'unknown';
}

function buildAdminAuthHeaders(token) {
  if (/^shp[a-z]+_/i.test(token)) {
    return {
      'X-Shopify-Access-Token': token,
    };
  }

  const bearerToken = token.startsWith('Bearer ') ? token : `Bearer ${token}`;
  return {
    Authorization: bearerToken,
    'X-Shopify-Access-Token': bearerToken,
  };
}

async function findReadableAppEnvPath() {
  for (const candidate of appEnvCandidates) {
    try {
      await readFile(candidate, 'utf8');
      return candidate;
    } catch (error) {
      if (error?.code === 'ENOENT') {
        continue;
      }

      throw error;
    }
  }

  throw new Error(
    [
      'Could not find a readable Shopify app .env with SHOPIFY_API_SECRET.',
      'Checked:',
      ...appEnvCandidates.map((candidate) => `- ${candidate}`),
      'Set SHOPIFY_CONFORMANCE_APP_ENV_PATH to override.',
    ].join('\n'),
  );
}

async function loadJson(pathname) {
  return JSON.parse(await readFile(pathname, 'utf8'));
}

async function loadOptionalJson(pathname) {
  try {
    return JSON.parse(await readFile(pathname, 'utf8'));
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return null;
    }

    throw error;
  }
}

function extractRequiredString(payload, key, sourceName) {
  const value = payload?.[key];
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${sourceName} is missing required string field ${key}.`);
  }

  return value;
}

function parseClientSecret(appEnvSource, appEnvPath) {
  const match = appEnvSource.match(/^SHOPIFY_API_SECRET=(.+)$/m);
  if (!match || match[1].trim().length === 0) {
    throw new Error(`SHOPIFY_API_SECRET is missing from ${appEnvPath}.`);
  }

  return match[1].trim();
}

function mergeRefreshedTokenPayload(previousPayload, refreshedPayload, store, clientId) {
  const accessToken = extractRequiredString(refreshedPayload, 'access_token', 'refresh response');
  const refreshToken = extractRequiredString(refreshedPayload, 'refresh_token', 'refresh response');

  return {
    ...previousPayload,
    ...refreshedPayload,
    store,
    client_id: clientId,
    access_token: accessToken,
    refresh_token: refreshToken,
    token_family: extractTokenFamily(accessToken),
  };
}

function updateEnvAccessToken(envSource, accessToken) {
  const [nextEnvSource, replacements] =
    envSource.replace(
      /^SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN=.*$/m,
      `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN=${accessToken}`,
    ) === envSource
      ? [envSource, 0]
      : [
          envSource.replace(
            /^SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN=.*$/m,
            `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN=${accessToken}`,
          ),
          1,
        ];

  if (replacements !== 1) {
    throw new Error('Failed to replace SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN in .env');
  }

  return nextEnvSource;
}

async function runProbe(adminOrigin, apiVersion, accessToken) {
  const response = await fetch(`${adminOrigin}/admin/api/${apiVersion}/graphql.json`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...buildAdminAuthHeaders(accessToken),
    },
    body: JSON.stringify({
      query: `#graphql
        query RefreshProbe {
          shop {
            id
            name
            myshopifyDomain
          }
        }
      `,
    }),
  });

  const payload = await response.json();
  return {
    status: response.status,
    ok: response.ok && !payload.errors,
    payload,
  };
}

const manualStoreAuthPayload = await loadJson(manualStoreAuthTokenPath);
const refreshToken = extractRequiredString(manualStoreAuthPayload, 'refresh_token', manualStoreAuthTokenPath);
const clientIdResolution = resolveRefreshClientId(manualStoreAuthPayload, [
  {
    sourceName: manualStoreAuthPkcePath,
    payload: await loadOptionalJson(manualStoreAuthPkcePath),
  },
  {
    sourceName: manualStoreAuthPath,
    payload: await loadOptionalJson(manualStoreAuthPath),
  },
]);
const clientId = clientIdResolution.clientId;
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'] || `https://${storeDomain}`;
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';

const appEnvPath = await findReadableAppEnvPath();
const appEnvSource = await readFile(appEnvPath, 'utf8');
const clientSecret = parseClientSecret(appEnvSource, appEnvPath);

const refreshBody = new URLSearchParams({
  client_id: clientId,
  client_secret: clientSecret,
  grant_type: 'refresh_token',
  refresh_token: refreshToken,
});

const refreshResponse = await fetch(`https://${storeDomain}/admin/oauth/access_token`, {
  method: 'POST',
  headers: {
    'Content-Type': 'application/x-www-form-urlencoded',
    Accept: 'application/json',
  },
  body: refreshBody,
});

const refreshPayload = await refreshResponse.json();
if (!refreshResponse.ok || refreshPayload.errors) {
  const refreshFailure = classifyRefreshFailure(refreshPayload);
  console.error(
    JSON.stringify(
      {
        ok: false,
        status: refreshResponse.status,
        refreshPayload,
        refreshFailure,
        recommendedNextStep: refreshFailure?.recommendedNextStep ?? 'retry-refresh-or-investigate-auth-state',
        manualStoreAuthTokenPath,
        appEnvPath,
      },
      null,
      2,
    ),
  );
  process.exit(1);
}

const nextManualStoreAuthPayload = mergeRefreshedTokenPayload(
  manualStoreAuthPayload,
  refreshPayload,
  storeDomain,
  clientId,
);

const envSource = await readFile(envPath, 'utf8');
const nextEnvSource = updateEnvAccessToken(envSource, nextManualStoreAuthPayload.access_token);

await writeFile(manualStoreAuthTokenPath, `${JSON.stringify(nextManualStoreAuthPayload, null, 2)}\n`, 'utf8');
await writeFile(envPath, nextEnvSource, 'utf8');

const probe = await runProbe(adminOrigin, apiVersion, nextManualStoreAuthPayload.access_token);
if (!probe.ok) {
  console.error(
    JSON.stringify(
      {
        ok: false,
        message: 'Token refresh persisted, but verification probe failed.',
        refreshStatus: refreshResponse.status,
        probe,
        tokenFamily: nextManualStoreAuthPayload.token_family,
        accessTokenSuffix: nextManualStoreAuthPayload.access_token.slice(-6),
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
      manualStoreAuthTokenPath,
      appEnvPath,
      envPath,
      tokenFamily: nextManualStoreAuthPayload.token_family,
      accessTokenSuffix: nextManualStoreAuthPayload.access_token.slice(-6),
      refreshTokenSuffix: nextManualStoreAuthPayload.refresh_token.slice(-6),
      expiresIn: nextManualStoreAuthPayload.expires_in ?? null,
      refreshTokenExpiresIn: nextManualStoreAuthPayload.refresh_token_expires_in ?? null,
      probeStatus: probe.status,
      shop: probe.payload?.data?.shop ?? null,
    },
    null,
    2,
  ),
);
