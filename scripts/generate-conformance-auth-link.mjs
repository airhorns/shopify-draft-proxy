import 'dotenv/config';

import { readFile } from 'node:fs/promises';
import path from 'node:path';

import {
  SHOPIFY_CONFORMANCE_AUTH_REQUEST_PATH,
  SHOPIFY_CONFORMANCE_PKCE_PATH,
  createConformanceAuthRequest,
} from './shopify-conformance-auth.mjs';

function extractTomlString(source, key) {
  const pattern = new RegExp(`^${key}\\s*=\\s*"([^"]+)"`, 'mu');
  return pattern.exec(source)?.[1] ?? null;
}

function extractTomlScopes(source) {
  const scopesValue = extractTomlString(source, 'scopes');
  if (!scopesValue) {
    return [];
  }
  return scopesValue
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function extractRedirectUrls(source) {
  const match = source.match(/redirect_urls\s*=\s*\[([\s\S]*?)\]/mu);
  if (!match?.[1]) {
    return [];
  }

  return Array.from(match[1].matchAll(/"([^"]+)"/gu), (entry) => entry[1]);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
if (!storeDomain) {
  throw new Error('SHOPIFY_CONFORMANCE_STORE_DOMAIN is required to generate a Shopify auth link.');
}

const appHandle = process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] || 'hermes-conformance-products';
const appConfigPath = path.join('/tmp/shopify-conformance-app', appHandle, 'shopify.app.toml');
const appConfig = await readFile(appConfigPath, 'utf8');
const clientId = extractTomlString(appConfig, 'client_id');
const scopes = extractTomlScopes(appConfig);
const redirectUri =
  extractRedirectUrls(appConfig).find((value) => value === 'http://127.0.0.1:13387/auth/callback') ||
  'http://127.0.0.1:13387/auth/callback';

if (!clientId) {
  throw new Error(`Could not find client_id in ${appConfigPath}.`);
}
if (scopes.length === 0) {
  throw new Error(`Could not find access_scopes.scopes in ${appConfigPath}.`);
}

const authRequest = await createConformanceAuthRequest({
  storeDomain,
  clientId,
  scopes,
  redirectUri,
});

console.log(
  JSON.stringify(
    {
      ok: true,
      storeDomain,
      clientId,
      scopes,
      redirectUri,
      authRequestPath: SHOPIFY_CONFORMANCE_AUTH_REQUEST_PATH,
      pkcePath: SHOPIFY_CONFORMANCE_PKCE_PATH,
      authorizeUrl: authRequest.authorize_url,
    },
    null,
    2,
  ),
);
