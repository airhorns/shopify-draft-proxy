import 'dotenv/config';

import { printJson } from './stdout.mjs';

const requiredVars = [
  'SHOPIFY_CONFORMANCE_STORE_DOMAIN',
  'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN',
  'SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN',
];

const missingVars = requiredVars.filter((name) => !process.env[name]);

if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const adminAccessToken = process.env['SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN'];
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const expectedOrigin = `https://${storeDomain}`;

if (adminOrigin !== expectedOrigin) {
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

function buildAdminAuthHeaders(token) {
  if (token.startsWith('shpat_')) {
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
  console.error(JSON.stringify({ status: response.status, payload }, null, 2));
  process.exit(1);
}

printJson({
  ok: true,
  apiVersion,
  storeDomain,
  shop: payload.data?.shop ?? null,
});
