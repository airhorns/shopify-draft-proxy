/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import {
  exchangeConformanceAuthCallback,
  getStorefrontConformanceAuthProfile,
  getValidConformanceAccessToken,
  resolveDefaultAppEnvPath,
} from './shopify-conformance-auth.mjs';
import { readConformanceScriptConfig } from './conformance-script-config.js';

const callbackArgs = process.argv.slice(2).filter((value) => value !== '--');
const callbackUrl = callbackArgs[0];
if (!callbackUrl) {
  throw new Error("Usage: corepack pnpm conformance:exchange-storefront-auth -- '<full callback url>'");
}

const { adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const profile = getStorefrontConformanceAuthProfile();
const appEnvPath = resolveDefaultAppEnvPath({ appHandle: profile.appHandle });
const persisted = await exchangeConformanceAuthCallback({
  callbackUrl,
  credentialPath: profile.credentialPath,
  authRequestPath: profile.authRequestPath,
  appEnvPath,
});
const accessToken = await getValidConformanceAccessToken({
  adminOrigin,
  apiVersion,
  credentialPath: profile.credentialPath,
  appEnvPath,
});

process.stdout.write(
  JSON.stringify(
    {
      ok: true,
      appHandle: profile.appHandle,
      credentialPath: profile.credentialPath,
      shop: persisted.shop,
      tokenFamily: persisted.token_family,
      refreshedTokenFamily: accessToken.split('_', 1)[0] ?? null,
      apiVersion,
      validatedAgainst: adminOrigin,
    },
    null,
    2,
  ) + '\n',
);
