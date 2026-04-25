// @ts-nocheck
import 'dotenv/config';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mts';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const expectedOrigin = `https://${storeDomain}`;

if (adminOrigin !== expectedOrigin) {
  // oxlint-disable-next-line no-console -- CLI error output is intentionally written to stderr.
  console.error(
    `Expected SHOPIFY_CONFORMANCE_ADMIN_ORIGIN=${expectedOrigin} to match SHOPIFY_CONFORMANCE_STORE_DOMAIN=${storeDomain}`,
  );
  process.exit(1);
}

const query = `#graphql
  query ConformanceProbe {
    shop {
      id
      name
      myshopifyDomain
      primaryDomain {
        host
        url
      }
    }
  }
`;

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const { status, payload } = await runGraphqlRequest(query);

if (status < 200 || status >= 300 || payload.errors) {
  // oxlint-disable-next-line no-console -- CLI error output is intentionally written to stderr.
  console.error(JSON.stringify({ status, payload }, null, 2));
  process.exit(1);
}

// oxlint-disable-next-line no-console -- CLI probe result is intentionally written to stdout.
console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      storeDomain,
      shop: payload.data?.shop ?? null,
    },
    null,
    2,
  ),
);
