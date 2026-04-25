// @ts-nocheck
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { runAdminGraphql } from './conformance-graphql-client.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { pickCollectionCaptureSeed } from './collection-conformance-lib.mjs';

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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);

async function runGraphql(query, variables = {}) {
  return runAdminGraphql(
    { adminOrigin, apiVersion, headers: buildAdminAuthHeaders(adminAccessToken) },
    query,
    variables,
  );
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
    collections(first: 20) {
      edges {
        node {
          id
          title
          handle
          ruleSet {
            appliedDisjunctively
            rules {
              column
              relation
              condition
            }
          }
          products(first: 1) {
            edges {
              node {
                id
              }
            }
          }
        }
      }
    }
  }
`;

const collectionDetailQuery = `#graphql
  query CollectionDetailRead($customCollectionId: ID!, $smartCollectionId: ID!, $productId: ID!) {
    customCollection: collection(id: $customCollectionId) {
      id
      legacyResourceId
      title
      handle
      updatedAt
      description
      descriptionHtml
      image {
        id
        url
        altText
        width
        height
      }
      productsCount {
        count
        precision
      }
      hasProduct(id: $productId)
      sortOrder
      templateSuffix
      seo {
        title
        description
      }
      ruleSet {
        appliedDisjunctively
        rules {
          column
          relation
          condition
        }
      }
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
    smartCollection: collection(id: $smartCollectionId) {
      id
      legacyResourceId
      title
      handle
      updatedAt
      description
      descriptionHtml
      image {
        id
        url
        altText
        width
        height
      }
      productsCount {
        count
        precision
      }
      hasProduct(id: $productId)
      sortOrder
      templateSuffix
      seo {
        title
        description
      }
      ruleSet {
        appliedDisjunctively
        rules {
          column
          relation
          condition
        }
      }
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
          legacyResourceId
          title
          handle
          updatedAt
          description
          descriptionHtml
          image {
            id
            url
            altText
            width
            height
          }
          productsCount {
            count
            precision
          }
          sortOrder
          templateSuffix
          seo {
            title
            description
          }
          ruleSet {
            appliedDisjunctively
            rules {
              column
              relation
              condition
            }
          }
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
const collectionEdges = collectionSeedCatalog.data.collections.edges;
const customCollection = collectionEdges.map((edge) => edge.node).find((collection) => collection.ruleSet === null);
const smartCollection = collectionEdges.map((edge) => edge.node).find((collection) => collection.ruleSet !== null);

if (!customCollection || !smartCollection) {
  throw new Error(
    JSON.stringify(
      {
        message: 'Could not find both custom and smart collection seeds in the live collection catalog.',
        customCollection: customCollection
          ? { id: customCollection.id, title: customCollection.title, handle: customCollection.handle }
          : null,
        smartCollection: smartCollection
          ? { id: smartCollection.id, title: smartCollection.title, handle: smartCollection.handle }
          : null,
      },
      null,
      2,
    ),
  );
}

const productId =
  customCollection.products.edges[0]?.node?.id ??
  smartCollection.products.edges[0]?.node?.id ??
  'gid://shopify/Product/0';
const collectionDetail = await runGraphql(collectionDetailQuery, {
  customCollectionId: customCollection.id,
  smartCollectionId: smartCollection.id,
  productId,
});
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
      customCollection: {
        id: customCollection.id,
        title: customCollection.title,
        handle: customCollection.handle,
      },
      smartCollection: {
        id: smartCollection.id,
        title: smartCollection.title,
        handle: smartCollection.handle,
      },
      productId,
      files: Object.keys(captures),
    },
    null,
    2,
  ),
);
