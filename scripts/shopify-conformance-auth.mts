import 'dotenv/config';

import { randomBytes, createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { access, mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';

import { z } from 'zod';

import { DEFAULT_ADMIN_API_VERSION } from '../src/shopify/api-version.js';
import { runAdminGraphqlRequest } from './conformance-graphql-client.js';

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

const storedConformanceAuthSchema = z.record(z.string(), z.unknown());
type StoredConformanceAuth = z.infer<typeof storedConformanceAuthSchema>;
type JsonRecord = Record<string, unknown>;
type ProbeAccessTokenResult = {
  ok: boolean;
  status: number;
  payload: unknown;
};

const accessTokenRefreshResponseSchema = z
  .object({
    access_token: z.string().min(1),
    refresh_token: z.string().min(1).optional(),
    scope: z.string().optional(),
    expires_in: z.number().int().optional(),
    refresh_token_expires_in: z.number().int().optional(),
  })
  .catchall(z.unknown());

type AccessTokenRefreshResponse = z.infer<typeof accessTokenRefreshResponseSchema>;

function tokenFamily(token: unknown): string | null {
  if (typeof token !== 'string') {
    return null;
  }

  const match = /^([A-Za-z0-9]+)_/.exec(token);
  return match?.[1] ?? null;
}

type FetchImpl = typeof fetch;

export function buildAdminAuthHeaders(token: string): Record<string, string> {
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

async function fileExists(filePath: string): Promise<boolean> {
  try {
    await access(filePath);
    return true;
  } catch {
    return false;
  }
}

async function readJsonFile(filePath: string): Promise<unknown> {
  return JSON.parse(await readFile(filePath, 'utf8')) as unknown;
}

function parseEnvFile(content: string): Record<string, string> {
  const vars: Record<string, string> = {};
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

async function writeJsonAtomically(filePath: string, value: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  const tempPath = `${filePath}.tmp`;
  await writeFile(tempPath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
  await rename(tempPath, filePath);
}

function isLikelyAuthFailure(result: { status: number; payload: unknown }): boolean {
  if (result.status === 401) {
    return true;
  }

  const payloadErrors = readArrayProperty(result.payload, 'errors');
  return payloadErrors.some((entry) => {
    const message = readStringProperty(entry, 'message');
    return typeof message === 'string' && /access token|authentication|invalid api key/i.test(message);
  });
}

function normalizeErrorText(input: string): string {
  return input.replace(/\s+/gu, ' ').trim();
}

function extractClearErrorMessage(payload: unknown, fallbackStatus: number): string {
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

  const errors = readUnknownProperty(payload, 'errors');
  if (Array.isArray(errors)) {
    const messages = errors
      .map((entry) => {
        const message = readStringProperty(entry, 'message');
        return typeof message === 'string' ? normalizeErrorText(message) : null;
      })
      .filter(Boolean);
    if (messages.length > 0) {
      return messages.join('; ');
    }
  }

  if (typeof errors === 'string') {
    return normalizeErrorText(errors);
  }

  const errorDescription = readStringProperty(payload, 'error_description');
  if (typeof errorDescription === 'string') {
    return normalizeErrorText(errorDescription);
  }

  const error = readStringProperty(payload, 'error');
  if (typeof error === 'string') {
    return normalizeErrorText(error);
  }

  return `HTTP ${fallbackStatus}`;
}

async function parseResponsePayload(response: Response): Promise<unknown> {
  const contentType = response.headers.get('content-type') ?? '';
  if (contentType.includes('application/json')) {
    return await response.json();
  }
  return await response.text();
}

async function probeAccessToken({
  adminOrigin,
  apiVersion = DEFAULT_ADMIN_API_VERSION,
  accessToken,
  fetchImpl = fetch,
}: {
  adminOrigin: string;
  apiVersion?: string;
  accessToken: string;
  fetchImpl?: FetchImpl;
}): Promise<ProbeAccessTokenResult> {
  const { status, payload } = await runAdminGraphqlRequest(
    {
      adminOrigin,
      apiVersion,
      headers: buildAdminAuthHeaders(accessToken),
      fetchImpl,
    },
    PROBE_QUERY,
  );
  return {
    ok: status >= 200 && status < 300 && !hasProperty(payload, 'errors'),
    status,
    payload,
  };
}

export function resolveDefaultAppRoot({ repoRoot = process.cwd() }: { repoRoot?: string } = {}): string {
  const appHandle = process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] || 'hermes-conformance-products';
  const repoLocalRoot = path.join(repoRoot, 'shopify-conformance-app', appHandle);
  if (existsSync(repoLocalRoot)) {
    return repoLocalRoot;
  }

  return path.join('/tmp/shopify-conformance-app', appHandle);
}

export function resolveDefaultAppEnvPath({ repoRoot = process.cwd() }: { repoRoot?: string } = {}): string {
  if (process.env['SHOPIFY_CONFORMANCE_APP_ENV_PATH']) {
    return process.env['SHOPIFY_CONFORMANCE_APP_ENV_PATH'];
  }

  const appHandle = process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] || 'hermes-conformance-products';
  const repoLocalEnvPath = path.join(repoRoot, 'shopify-conformance-app', appHandle, '.env');
  if (existsSync(repoLocalEnvPath)) {
    return repoLocalEnvPath;
  }

  return path.join('/tmp/shopify-conformance-app', appHandle, '.env');
}

async function readShopifyApiSecret(appEnvPath: string): Promise<string> {
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

async function loadStoredConformanceAuth(credentialPath: string): Promise<StoredConformanceAuth> {
  if (!(await fileExists(credentialPath))) {
    throw new Error(
      `Shopify conformance credential file not found at ${credentialPath}. Run the app grant flow to create a fresh token pair.`,
    );
  }

  const parsed = storedConformanceAuthSchema.safeParse(await readJsonFile(credentialPath));
  if (!parsed.success) {
    throw new Error(`Shopify conformance credential file at ${credentialPath} does not contain a JSON object.`);
  }

  return parsed.data;
}

export async function refreshConformanceAccessToken({
  credentialPath = SHOPIFY_CONFORMANCE_AUTH_PATH,
  appEnvPath = resolveDefaultAppEnvPath(),
  fetchImpl = fetch,
}: {
  credentialPath?: string;
  appEnvPath?: string;
  fetchImpl?: FetchImpl;
} = {}): Promise<StoredConformanceAuth> {
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

  const tokenPayload = parseAccessTokenRefreshResponse(payload);
  if (!tokenPayload) {
    throw new Error(
      `Shopify refresh response from https://${shop}/admin/oauth/access_token did not include access_token.`,
    );
  }

  const expiresIn = tokenPayload.expires_in;
  const refreshTokenExpiresIn = tokenPayload.refresh_token_expires_in;
  const hasExpiresIn = typeof expiresIn === 'number' && Number.isInteger(expiresIn);
  const hasRefreshTokenExpiresIn = typeof refreshTokenExpiresIn === 'number' && Number.isInteger(refreshTokenExpiresIn);
  const obtainedAt = new Date().toISOString();
  const updatedAuth = {
    ...storedAuth,
    access_token: tokenPayload.access_token,
    refresh_token:
      typeof tokenPayload.refresh_token === 'string' && tokenPayload.refresh_token.length > 0
        ? tokenPayload.refresh_token
        : refreshToken,
    scope: typeof tokenPayload.scope === 'string' ? tokenPayload.scope : (storedAuth['scope'] ?? null),
    expires_in: hasExpiresIn ? expiresIn : (storedAuth['expires_in'] ?? null),
    expires_at: hasExpiresIn
      ? new Date(Date.now() + expiresIn * 1000).toISOString()
      : (storedAuth['expires_at'] ?? null),
    refresh_token_expires_in: hasRefreshTokenExpiresIn
      ? refreshTokenExpiresIn
      : (storedAuth['refresh_token_expires_in'] ?? null),
    refresh_token_expires_at: hasRefreshTokenExpiresIn
      ? new Date(Date.now() + refreshTokenExpiresIn * 1000).toISOString()
      : (storedAuth['refresh_token_expires_at'] ?? null),
    obtained_at: obtainedAt,
    token_family: tokenFamily(tokenPayload.access_token),
    client_id: clientId,
    shop,
    store: shop,
  };

  await writeJsonAtomically(credentialPath, updatedAuth);
  return updatedAuth;
}

export async function getValidConformanceAccessToken({
  adminOrigin,
  apiVersion = DEFAULT_ADMIN_API_VERSION,
  credentialPath = SHOPIFY_CONFORMANCE_AUTH_PATH,
  appEnvPath = resolveDefaultAppEnvPath(),
  fetchImpl = fetch,
}: {
  adminOrigin: string;
  apiVersion?: string;
  credentialPath?: string;
  appEnvPath?: string;
  fetchImpl?: FetchImpl;
}): Promise<string> {
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
      accessToken: readRequiredStringProperty(refreshedAuth, 'access_token'),
      fetchImpl,
    });
    if (!refreshedProbe.ok) {
      throw new Error(extractClearErrorMessage(refreshedProbe.payload, refreshedProbe.status));
    }
    return readRequiredStringProperty(refreshedAuth, 'access_token');
  } catch (error) {
    throw new Error(
      `Stored Shopify conformance access token is invalid and refresh failed: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

function encodeBase64Url(buffer: Buffer): string {
  return buffer.toString('base64url');
}

function buildCodeChallenge(codeVerifier: string): string {
  return encodeBase64Url(createHash('sha256').update(codeVerifier).digest());
}

export async function createConformanceAuthRequest({
  storeDomain,
  clientId,
  scopes,
  redirectUri = DEFAULT_REDIRECT_URI,
  authRequestPath = SHOPIFY_CONFORMANCE_AUTH_REQUEST_PATH,
  pkcePath = SHOPIFY_CONFORMANCE_PKCE_PATH,
}: {
  storeDomain: string;
  clientId: string;
  scopes: string[];
  redirectUri?: string;
  authRequestPath?: string;
  pkcePath?: string;
}): Promise<StoredConformanceAuth> {
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
}: {
  callbackUrl: string;
  credentialPath?: string;
  authRequestPath?: string;
  appEnvPath?: string;
  fetchImpl?: FetchImpl;
}): Promise<StoredConformanceAuth> {
  if (typeof callbackUrl !== 'string' || callbackUrl.length === 0) {
    throw new Error('exchangeConformanceAuthCallback requires callbackUrl.');
  }

  if (!(await fileExists(authRequestPath))) {
    throw new Error(
      `Shopify conformance auth request file not found at ${authRequestPath}. Generate a fresh auth link first.`,
    );
  }

  const parsedRequestState = storedConformanceAuthSchema.safeParse(await readJsonFile(authRequestPath));
  if (!parsedRequestState.success) {
    throw new Error(`Shopify conformance auth request file at ${authRequestPath} does not contain a JSON object.`);
  }
  const requestState = parsedRequestState.data;
  const requestClientId = readRequiredStringProperty(requestState, 'client_id');
  const requestCodeVerifier = readRequiredStringProperty(requestState, 'code_verifier');
  const requestRedirectUri = readRequiredStringProperty(requestState, 'redirect_uri');
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
      client_id: requestClientId,
      client_secret: clientSecret,
      code,
      code_verifier: requestCodeVerifier,
      redirect_uri: requestRedirectUri,
      expiring: '1',
    }),
  });
  const payload = await parseResponsePayload(response);
  if (!response.ok) {
    throw new Error(extractClearErrorMessage(payload, response.status));
  }

  const accessToken = readRequiredStringProperty(payload, 'access_token');
  const obtainedAt = new Date().toISOString();
  const persisted = {
    shop,
    store: shop,
    client_id: requestClientId,
    access_token: accessToken,
    refresh_token: readStringProperty(payload, 'refresh_token') ?? null,
    scope:
      readStringProperty(payload, 'scope') ??
      (Array.isArray(requestState['scopes']) ? requestState['scopes'].join(',') : null),
    expires_in: readNumberProperty(payload, 'expires_in') ?? null,
    expires_at: Number.isInteger(readNumberProperty(payload, 'expires_in'))
      ? new Date(Date.now() + readRequiredNumberProperty(payload, 'expires_in') * 1000).toISOString()
      : null,
    refresh_token_expires_in: readNumberProperty(payload, 'refresh_token_expires_in') ?? null,
    refresh_token_expires_at: Number.isInteger(readNumberProperty(payload, 'refresh_token_expires_in'))
      ? new Date(Date.now() + readRequiredNumberProperty(payload, 'refresh_token_expires_in') * 1000).toISOString()
      : null,
    obtained_at: obtainedAt,
    source_callback_url: callbackUrl,
    grant_mode: 'expiring-offline-token-pkce',
    token_family: tokenFamily(accessToken),
  };

  await writeJsonAtomically(credentialPath, persisted);
  return persisted;
}

function parseAccessTokenRefreshResponse(payload: unknown): AccessTokenRefreshResponse | null {
  const parsed = accessTokenRefreshResponseSchema.safeParse(payload);
  return parsed.success ? parsed.data : null;
}

function readRequiredStringProperty(value: unknown, key: string): string {
  const result = readStringProperty(value, key);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Error(`Expected ${key} to be a non-empty string.`);
  }
  return result;
}

function readRequiredNumberProperty(value: unknown, key: string): number {
  const result = readNumberProperty(value, key);
  if (typeof result !== 'number' || !Number.isFinite(result)) {
    throw new Error(`Expected ${key} to be a number.`);
  }
  return result;
}

function readStringProperty(value: unknown, key: string): string | null {
  const property = readUnknownProperty(value, key);
  return typeof property === 'string' ? property : null;
}

function readNumberProperty(value: unknown, key: string): number | null {
  const property = readUnknownProperty(value, key);
  return typeof property === 'number' ? property : null;
}

function readArrayProperty(value: unknown, key: string): unknown[] {
  const property = readUnknownProperty(value, key);
  return Array.isArray(property) ? property : [];
}

function readUnknownProperty(value: unknown, key: string): unknown {
  return isRecord(value) ? value[key] : undefined;
}

function hasProperty(value: unknown, key: string): boolean {
  return isRecord(value) && key in value;
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
