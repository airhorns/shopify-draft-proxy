import 'dotenv/config';

import { randomBytes, createHash } from 'node:crypto';
import { access, mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';

export const SHOPIFY_CONFORMANCE_AUTH_DIR = path.join(homedir(), '.shopify-draft-proxy');
export const SHOPIFY_CONFORMANCE_AUTH_PATH = path.join(SHOPIFY_CONFORMANCE_AUTH_DIR, 'conformance-admin-auth.json');
export const SHOPIFY_CONFORMANCE_PKCE_PATH = path.join(
  SHOPIFY_CONFORMANCE_AUTH_DIR,
  'conformance-admin-auth-pkce.json',
);
export const SHOPIFY_CONFORMANCE_AUTH_REQUEST_PATH = path.join(
  SHOPIFY_CONFORMANCE_AUTH_DIR,
  'conformance-admin-auth-request.json',
);

const DEFAULT_API_VERSION = '2025-01';
const DEFAULT_REDIRECT_URI = 'http://127.0.0.1:13387/auth/callback';
const PROBE_QUERY = `#graphql
  query ConformanceProbe {
    shop {
      id
      name
      myshopifyDomain
    }
  }
`;

function tokenFamily(token) {
  if (typeof token !== 'string') {
    return null;
  }

  const match = /^([A-Za-z0-9]+)_/.exec(token);
  return match?.[1] ?? null;
}

export function buildAdminAuthHeaders(token) {
  if (typeof token === 'string' && /^shp[a-z]+_/i.test(token)) {
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

async function fileExists(filePath) {
  try {
    await access(filePath);
    return true;
  } catch {
    return false;
  }
}

async function readJsonFile(filePath) {
  return JSON.parse(await readFile(filePath, 'utf8'));
}

function parseEnvFile(content) {
  const vars = {};
  for (const line of content.split(/\r?\n/u)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) {
      continue;
    }
    const separatorIndex = trimmed.indexOf('=');
    if (separatorIndex === -1) {
      continue;
    }
    const key = trimmed.slice(0, separatorIndex).trim();
    const value = trimmed.slice(separatorIndex + 1).trim();
    vars[key] = value;
  }
  return vars;
}

async function writeJsonAtomically(filePath, value) {
  await mkdir(path.dirname(filePath), { recursive: true });
  const tempPath = `${filePath}.tmp`;
  await writeFile(tempPath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
  await rename(tempPath, filePath);
}

function isLikelyAuthFailure(result) {
  if (result.status === 401) {
    return true;
  }

  const payloadErrors = Array.isArray(result.payload?.errors) ? result.payload.errors : [];
  return payloadErrors.some(
    (entry) => typeof entry?.message === 'string' && /access token|authentication|invalid api key/i.test(entry.message),
  );
}

function normalizeErrorText(input) {
  return input.replace(/\s+/gu, ' ').trim();
}

function extractClearErrorMessage(payload, fallbackStatus) {
  if (typeof payload === 'string') {
    const htmlStripped = normalizeErrorText(
      payload.replace(/<style[\s\S]*?<\/style>/giu, ' ').replace(/<[^>]+>/gu, ' '),
    );
    const activeRefreshTokenMatch = htmlStripped.match(/This request requires an active refresh_token/iu);
    if (activeRefreshTokenMatch?.[0]) {
      return activeRefreshTokenMatch[0];
    }

    const oauthMatch = htmlStripped.match(/Oauth error [^:]+:\s*(.+)$/iu);
    if (oauthMatch?.[1]) {
      return normalizeErrorText(oauthMatch[1]);
    }

    const invalidApiTokenMatch = htmlStripped.match(/Invalid API key or access token.*/iu);
    if (invalidApiTokenMatch?.[0]) {
      return normalizeErrorText(invalidApiTokenMatch[0]);
    }

    return htmlStripped.slice(0, 300);
  }

  if (Array.isArray(payload?.errors)) {
    const messages = payload.errors
      .map((entry) => (typeof entry?.message === 'string' ? normalizeErrorText(entry.message) : null))
      .filter(Boolean);
    if (messages.length > 0) {
      return messages.join('; ');
    }
  }

  if (typeof payload?.errors === 'string') {
    return normalizeErrorText(payload.errors);
  }

  if (typeof payload?.error_description === 'string') {
    return normalizeErrorText(payload.error_description);
  }

  if (typeof payload?.error === 'string') {
    return normalizeErrorText(payload.error);
  }

  return `HTTP ${fallbackStatus}`;
}

async function parseResponsePayload(response) {
  const contentType = response.headers.get('content-type') ?? '';
  if (contentType.includes('application/json')) {
    return await response.json();
  }
  return await response.text();
}

async function probeAccessToken({ adminOrigin, apiVersion = DEFAULT_API_VERSION, accessToken, fetchImpl = fetch }) {
  const response = await fetchImpl(`${adminOrigin}/admin/api/${apiVersion}/graphql.json`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...buildAdminAuthHeaders(accessToken),
    },
    body: JSON.stringify({ query: PROBE_QUERY }),
  });
  const payload = await parseResponsePayload(response);
  return {
    ok: response.ok && !payload?.errors,
    status: response.status,
    payload,
  };
}

function resolveDefaultAppEnvPath() {
  if (process.env['SHOPIFY_CONFORMANCE_APP_ENV_PATH']) {
    return process.env['SHOPIFY_CONFORMANCE_APP_ENV_PATH'];
  }

  const appHandle = process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] || 'hermes-conformance-products';
  return path.join('/tmp/shopify-conformance-app', appHandle, '.env');
}

async function readShopifyApiSecret(appEnvPath) {
  if (!(await fileExists(appEnvPath))) {
    throw new Error(
      `Shopify app env file not found at ${appEnvPath}. Set SHOPIFY_CONFORMANCE_APP_ENV_PATH or restore the linked app workspace before refreshing the token.`,
    );
  }

  const envVars = parseEnvFile(await readFile(appEnvPath, 'utf8'));
  const secret = envVars['SHOPIFY_API_SECRET'];
  if (!secret) {
    throw new Error(`SHOPIFY_API_SECRET is missing from ${appEnvPath}.`);
  }
  return secret;
}

async function loadStoredConformanceAuth(credentialPath) {
  if (!(await fileExists(credentialPath))) {
    throw new Error(
      `Shopify conformance credential file not found at ${credentialPath}. Run the app grant flow to create a fresh token pair.`,
    );
  }

  const auth = await readJsonFile(credentialPath);
  if (typeof auth !== 'object' || auth === null) {
    throw new Error(`Shopify conformance credential file at ${credentialPath} does not contain a JSON object.`);
  }

  return auth;
}

export async function refreshConformanceAccessToken({
  credentialPath = SHOPIFY_CONFORMANCE_AUTH_PATH,
  appEnvPath = resolveDefaultAppEnvPath(),
  fetchImpl = fetch,
} = {}) {
  const storedAuth = await loadStoredConformanceAuth(credentialPath);
  const refreshToken = storedAuth['refresh_token'];
  const clientId = storedAuth['client_id'] ?? storedAuth['clientId'];
  const shop = storedAuth['shop'] ?? storedAuth['store'];

  if (typeof refreshToken !== 'string' || refreshToken.length === 0) {
    throw new Error(`Shopify conformance credential at ${credentialPath} is missing refresh_token.`);
  }
  if (typeof clientId !== 'string' || clientId.length === 0) {
    throw new Error(`Shopify conformance credential at ${credentialPath} is missing client_id.`);
  }
  if (typeof shop !== 'string' || shop.length === 0) {
    throw new Error(`Shopify conformance credential at ${credentialPath} is missing shop/store.`);
  }

  const clientSecret = await readShopifyApiSecret(appEnvPath);
  const response = await fetchImpl(`https://${shop}/admin/oauth/access_token`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: new URLSearchParams({
      client_id: clientId,
      client_secret: clientSecret,
      grant_type: 'refresh_token',
      refresh_token: refreshToken,
    }),
  });
  const payload = await parseResponsePayload(response);
  if (!response.ok) {
    throw new Error(extractClearErrorMessage(payload, response.status));
  }

  if (typeof payload?.access_token !== 'string' || payload.access_token.length === 0) {
    throw new Error(
      `Shopify refresh response from https://${shop}/admin/oauth/access_token did not include access_token.`,
    );
  }

  const obtainedAt = new Date().toISOString();
  const updatedAuth = {
    ...storedAuth,
    access_token: payload.access_token,
    refresh_token:
      typeof payload.refresh_token === 'string' && payload.refresh_token.length > 0
        ? payload.refresh_token
        : refreshToken,
    scope: typeof payload.scope === 'string' ? payload.scope : (storedAuth['scope'] ?? null),
    expires_in: Number.isInteger(payload.expires_in) ? payload.expires_in : (storedAuth['expires_in'] ?? null),
    expires_at: Number.isInteger(payload.expires_in)
      ? new Date(Date.now() + payload.expires_in * 1000).toISOString()
      : (storedAuth['expires_at'] ?? null),
    refresh_token_expires_in: Number.isInteger(payload.refresh_token_expires_in)
      ? payload.refresh_token_expires_in
      : (storedAuth['refresh_token_expires_in'] ?? null),
    refresh_token_expires_at: Number.isInteger(payload.refresh_token_expires_in)
      ? new Date(Date.now() + payload.refresh_token_expires_in * 1000).toISOString()
      : (storedAuth['refresh_token_expires_at'] ?? null),
    obtained_at: obtainedAt,
    token_family: tokenFamily(payload.access_token),
    client_id: clientId,
    shop,
    store: shop,
  };

  await writeJsonAtomically(credentialPath, updatedAuth);
  return updatedAuth;
}

export async function getValidConformanceAccessToken({
  adminOrigin,
  apiVersion = DEFAULT_API_VERSION,
  credentialPath = SHOPIFY_CONFORMANCE_AUTH_PATH,
  appEnvPath = resolveDefaultAppEnvPath(),
  fetchImpl = fetch,
} = {}) {
  if (typeof adminOrigin !== 'string' || adminOrigin.length === 0) {
    throw new Error('getValidConformanceAccessToken requires adminOrigin.');
  }

  const storedAuth = await loadStoredConformanceAuth(credentialPath);
  const accessToken = storedAuth['access_token'];
  if (typeof accessToken !== 'string' || accessToken.length === 0) {
    throw new Error(`Shopify conformance credential at ${credentialPath} is missing access_token.`);
  }

  const probeResult = await probeAccessToken({ adminOrigin, apiVersion, accessToken, fetchImpl });
  if (probeResult.ok) {
    return accessToken;
  }

  if (!isLikelyAuthFailure(probeResult)) {
    throw new Error(
      `Stored Shopify conformance access token probe failed: ${extractClearErrorMessage(probeResult.payload, probeResult.status)}`,
    );
  }

  try {
    const refreshedAuth = await refreshConformanceAccessToken({ credentialPath, appEnvPath, fetchImpl });
    const refreshedProbe = await probeAccessToken({
      adminOrigin,
      apiVersion,
      accessToken: refreshedAuth['access_token'],
      fetchImpl,
    });
    if (!refreshedProbe.ok) {
      throw new Error(extractClearErrorMessage(refreshedProbe.payload, refreshedProbe.status));
    }
    return refreshedAuth['access_token'];
  } catch (error) {
    throw new Error(
      `Stored Shopify conformance access token is invalid and refresh failed: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

function encodeBase64Url(buffer) {
  return buffer.toString('base64url');
}

function buildCodeChallenge(codeVerifier) {
  return encodeBase64Url(createHash('sha256').update(codeVerifier).digest());
}

export async function createConformanceAuthRequest({
  storeDomain,
  clientId,
  scopes,
  redirectUri = DEFAULT_REDIRECT_URI,
  authRequestPath = SHOPIFY_CONFORMANCE_AUTH_REQUEST_PATH,
  pkcePath = SHOPIFY_CONFORMANCE_PKCE_PATH,
} = {}) {
  if (typeof storeDomain !== 'string' || storeDomain.length === 0) {
    throw new Error('createConformanceAuthRequest requires storeDomain.');
  }
  if (typeof clientId !== 'string' || clientId.length === 0) {
    throw new Error('createConformanceAuthRequest requires clientId.');
  }
  if (!Array.isArray(scopes) || scopes.length === 0) {
    throw new Error('createConformanceAuthRequest requires at least one scope.');
  }

  const state = encodeBase64Url(randomBytes(24));
  const codeVerifier = encodeBase64Url(randomBytes(72));
  const codeChallenge = buildCodeChallenge(codeVerifier);
  const authorizeUrl = new URL(`https://${storeDomain}/admin/oauth/authorize`);
  authorizeUrl.searchParams.set('client_id', clientId);
  authorizeUrl.searchParams.set('scope', scopes.join(','));
  authorizeUrl.searchParams.set('redirect_uri', redirectUri);
  authorizeUrl.searchParams.set('state', state);
  authorizeUrl.searchParams.set('response_type', 'code');
  authorizeUrl.searchParams.set('code_challenge', codeChallenge);
  authorizeUrl.searchParams.set('code_challenge_method', 'S256');

  const payload = {
    shop: storeDomain,
    client_id: clientId,
    scopes,
    redirect_uri: redirectUri,
    state,
    code_verifier: codeVerifier,
    code_challenge: codeChallenge,
    code_challenge_method: 'S256',
    authorize_url: authorizeUrl.toString(),
    generated_at: new Date().toISOString(),
  };

  await writeJsonAtomically(authRequestPath, payload);
  await writeJsonAtomically(pkcePath, payload);
  return payload;
}

export async function exchangeConformanceAuthCallback({
  callbackUrl,
  credentialPath = SHOPIFY_CONFORMANCE_AUTH_PATH,
  authRequestPath = SHOPIFY_CONFORMANCE_AUTH_REQUEST_PATH,
  appEnvPath = resolveDefaultAppEnvPath(),
  fetchImpl = fetch,
} = {}) {
  if (typeof callbackUrl !== 'string' || callbackUrl.length === 0) {
    throw new Error('exchangeConformanceAuthCallback requires callbackUrl.');
  }

  if (!(await fileExists(authRequestPath))) {
    throw new Error(
      `Shopify conformance auth request file not found at ${authRequestPath}. Generate a fresh auth link first.`,
    );
  }

  const requestState = await readJsonFile(authRequestPath);
  const callback = new URL(callbackUrl);
  const code = callback.searchParams.get('code');
  const shop = callback.searchParams.get('shop');
  const state = callback.searchParams.get('state');
  if (!code) {
    throw new Error('Shopify callback URL is missing code.');
  }
  if (!shop) {
    throw new Error('Shopify callback URL is missing shop.');
  }
  if (state !== requestState['state']) {
    throw new Error('Shopify callback state did not match the saved PKCE state.');
  }

  const clientSecret = await readShopifyApiSecret(appEnvPath);
  const response = await fetchImpl(`https://${shop}/admin/oauth/access_token`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: new URLSearchParams({
      client_id: requestState['client_id'],
      client_secret: clientSecret,
      code,
      code_verifier: requestState['code_verifier'],
      redirect_uri: requestState['redirect_uri'],
      expiring: '1',
    }),
  });
  const payload = await parseResponsePayload(response);
  if (!response.ok) {
    throw new Error(extractClearErrorMessage(payload, response.status));
  }

  const obtainedAt = new Date().toISOString();
  const persisted = {
    shop,
    store: shop,
    client_id: requestState['client_id'],
    access_token: payload['access_token'],
    refresh_token: payload['refresh_token'] ?? null,
    scope: payload['scope'] ?? requestState['scopes']?.join(',') ?? null,
    expires_in: payload['expires_in'] ?? null,
    expires_at: Number.isInteger(payload['expires_in'])
      ? new Date(Date.now() + payload['expires_in'] * 1000).toISOString()
      : null,
    refresh_token_expires_in: payload['refresh_token_expires_in'] ?? null,
    refresh_token_expires_at: Number.isInteger(payload['refresh_token_expires_in'])
      ? new Date(Date.now() + payload['refresh_token_expires_in'] * 1000).toISOString()
      : null,
    obtained_at: obtainedAt,
    source_callback_url: callbackUrl,
    grant_mode: 'expiring-offline-token-pkce',
    token_family: tokenFamily(payload['access_token']),
  };

  await writeJsonAtomically(credentialPath, persisted);
  return persisted;
}
