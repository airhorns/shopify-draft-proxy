/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import {
  SHOPIFY_CONFORMANCE_STOREFRONT_AUTH_PATH,
  getStorefrontConformanceAuthProfile,
  grantStorefrontAccessToken,
  resolveDefaultAppEnvPath,
} from './shopify-conformance-auth.mjs';
import { readConformanceScriptConfig } from './conformance-script-config.js';

const { adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });

const titleArg = process.argv.slice(2).find((arg) => !arg.startsWith('--'));
const title = titleArg ?? 'hermes-conformance-storefront';
const profile = getStorefrontConformanceAuthProfile();

const storedAuth = await grantStorefrontAccessToken({
  adminOrigin,
  apiVersion,
  title,
  credentialPath: profile.credentialPath,
  appEnvPath: resolveDefaultAppEnvPath({ appHandle: profile.appHandle }),
});

process.stdout.write(
  JSON.stringify(
    {
      ok: true,
      storefrontCredentialPath: SHOPIFY_CONFORMANCE_STOREFRONT_AUTH_PATH,
      shop: storedAuth.shop,
      storefrontTokenId: storedAuth.storefront_token_id,
      storefrontTokenTitle: storedAuth.storefront_token_title,
      storefrontAccessScopes: storedAuth.storefront_access_scopes,
      obtainedAt: storedAuth.obtained_at,
      storefrontApiUrl: `https://${storedAuth.shop}/api/${apiVersion}/graphql.json`,
    },
    null,
    2,
  ) + '\n',
);
