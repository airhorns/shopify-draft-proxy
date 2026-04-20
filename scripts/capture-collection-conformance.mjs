import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { pickCollectionCaptureSeed } from './collection-conformance-lib.mjs';

const requiredVars = [
  'SHOPIFY_CONFORMANCE_STORE_DOMAIN',
  'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN',
  'SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN',
];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  // oxlint-disable-next-line no-console -- CLI error output is intentionally written to stderr.
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

const collectionSeedQuery = `#graphql
  query CollectionSeedCatalog {
    products(first: 10, sortKey: UPDATED_AT, reverse: true) {
      edges {
        node {
          id
          title
          collections(first: 5) {
            edges {
              node {
                id
                title
                handle
              }
            }
          }
        }
      }
    }
    collections(first: 5) {
      edges {
        node {
          id
          title
          handle
        }
      }
    }
  }
`;

const collectionDetailQuery = `#graphql
  query CollectionDetailRead($id: ID!) {
    collection(id: $id) {
      id
      title
      handle
      products(first: 3) {
        edges {
          cursor
          node {
            id
            title
            handle
            vendor
            productType
            tags
            totalInventory
            tracksInventory
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
  }
`;

const collectionsCatalogQuery = `#graphql
  query CollectionsCatalogRead($first: Int!) {
    collections(first: $first) {
      edges {
        cursor
        node {
          id
          title
          handle
          products(first: 2) {
            edges {
              cursor
              node {
                id
                title
                handle
                vendor
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

const collectionSeedCatalog = await runGraphql(collectionSeedQuery);
const sampleCollection = pickCollectionCaptureSeed(collectionSeedCatalog);
const collectionDetail = await runGraphql(collectionDetailQuery, { id: sampleCollection.id });
const collectionsCatalog = await runGraphql(collectionsCatalogQuery, { first: 3 });

const captures = {
  'collection-detail.json': collectionDetail,
  'collections-catalog.json': collectionsCatalog,
};

for (const [filename, payload] of Object.entries(captures)) {
  await writeFile(path.join(outputDir, filename), `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

// oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      sampleCollection,
      files: Object.keys(captures),
    },
    null,
    2,
  ),
);
