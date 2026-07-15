/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { readFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createConformanceAuthRequest,
  getStorefrontConformanceAuthProfile,
  resolveDefaultAppRoot,
} from './shopify-conformance-auth.mjs';
import { readConformanceScriptConfig } from './conformance-script-config.js';

function extractTomlString(source: string, key: string): string | null {
  const pattern = new RegExp(`^${key}\\s*=\\s*"([^"]+)"`, 'mu');
  return pattern.exec(source)?.[1] ?? null;
}

function extractTomlScopes(source: string): string[] {
  const scopesValue = extractTomlString(source, 'scopes');
  return scopesValue
    ? scopesValue
        .split(',')
        .map((entry) => entry.trim())
        .filter(Boolean)
    : [];
}

function extractRedirectUrls(source: string): string[] {
  const match = source.match(/redirect_urls\s*=\s*\[([\s\S]*?)\]/mu);
  return match?.[1] ? Array.from(match[1].matchAll(/"([^"]+)"/gu), (entry) => entry[1]) : [];
}

const { storeDomain } = readConformanceScriptConfig({ exitOnMissing: true, requireAdminOrigin: false });
const profile = getStorefrontConformanceAuthProfile();
const appRoot = resolveDefaultAppRoot({ appHandle: profile.appHandle });
const appConfigPath = path.join(appRoot, 'shopify.app.toml');
const appConfig = await readFile(appConfigPath, 'utf8');
const clientId = extractTomlString(appConfig, 'client_id');
const scopes = extractTomlScopes(appConfig);
const redirectUrls = extractRedirectUrls(appConfig);

if (!clientId) {
  throw new Error(`Could not find client_id in ${appConfigPath}.`);
}
if (scopes.length === 0) {
  throw new Error(`Could not find access_scopes.scopes in ${appConfigPath}.`);
}
if (!redirectUrls.includes(profile.redirectUri)) {
  throw new Error(
    `Storefront app config ${appConfigPath} must include ${profile.redirectUri} before generating an auth link.`,
  );
}

const authRequest = await createConformanceAuthRequest({
  storeDomain,
  clientId,
  scopes,
  redirectUri: profile.redirectUri,
  authRequestPath: profile.authRequestPath,
  pkcePath: profile.pkcePath,
});

process.stdout.write(
  JSON.stringify(
    {
      ok: true,
      appHandle: profile.appHandle,
      storeDomain,
      clientId,
      scopes,
      redirectUri: profile.redirectUri,
      authRequestPath: profile.authRequestPath,
      pkcePath: profile.pkcePath,
      authorizeUrl: authRequest.authorize_url,
    },
    null,
    2,
  ) + '\n',
);
