// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import {
  SHOPIFY_CONFORMANCE_AUTH_PATH,
  exchangeConformanceAuthCallback,
  getValidConformanceAccessToken,
} from './shopify-conformance-auth.mjs';

const callbackArgs = process.argv.slice(2).filter((value) => value !== '--');
const callbackUrl = callbackArgs[0];
if (!callbackUrl) {
  throw new Error("Usage: corepack pnpm conformance:exchange-auth -- '<full callback url>'");
}

const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
if (!adminOrigin) {
  throw new Error('SHOPIFY_CONFORMANCE_ADMIN_ORIGIN is required to validate exchanged Shopify auth.');
}
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';

const persisted = await exchangeConformanceAuthCallback({ callbackUrl });
const accessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });

process.stdout.write(
  JSON.stringify(
    {
      ok: true,
      credentialPath: SHOPIFY_CONFORMANCE_AUTH_PATH,
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
