import 'dotenv/config';

import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];

const missingVars = requiredVars.filter((name) => !process.env[name]);

if (missingVars.length > 0) {
  // oxlint-disable-next-line no-console -- CLI error output is intentionally written to stderr.
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
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

const response = await fetch(`${adminOrigin}/admin/api/${apiVersion}/graphql.json`, {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
    ...buildAdminAuthHeaders(adminAccessToken),
  },
  body: JSON.stringify({ query }),
});

const payload = await response.json();

if (!response.ok || payload.errors) {
  // oxlint-disable-next-line no-console -- CLI error output is intentionally written to stderr.
  console.error(JSON.stringify({ status: response.status, payload }, null, 2));
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
