import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);

function buildAdminAuthHeaders(token) {
  if (/^shp[a-z]+_/.test(token)) {
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

async function runGraphql(query, variables = {}) {
  const response = await fetch(`${adminOrigin}/admin/api/${apiVersion}/graphql.json`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...buildAdminAuthHeaders(adminAccessToken),
    },
    body: JSON.stringify({ query, variables }),
  });

  const payload = await response.json();
  if (!response.ok || payload.errors) {
    throw new Error(JSON.stringify({ status: response.status, payload }, null, 2));
  }

  return payload;
}

const locationsCatalogQuery = `#graphql
  query LocationsCatalogRead($first: Int!) {
    locations(first: $first) {
      edges {
        cursor
        node {
          id
          name
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

await mkdir(outputDir, { recursive: true });

const first = 10;
const locationsCatalog = await runGraphql(locationsCatalogQuery, { first });

await writeFile(path.join(outputDir, 'locations-catalog.json'), `${JSON.stringify(locationsCatalog, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      files: ['locations-catalog.json'],
      first,
    },
    null,
    2,
  ),
);
