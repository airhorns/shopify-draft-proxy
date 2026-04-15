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

const catalogQuery = `#graphql
  query ProductCatalogPage {
    productsCount(limit: null) {
      count
      precision
    }
    products(first: 3, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          legacyResourceId
          title
          handle
          status
          vendor
          productType
          tags
          totalInventory
          tracksInventory
          createdAt
          updatedAt
        }
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
`;

const detailQuery = `#graphql
  query ProductDetail($id: ID!) {
    product(id: $id) {
      id
      title
      handle
      status
      descriptionHtml
      onlineStorePreviewUrl
      templateSuffix
      seo {
        title
        description
      }
      category {
        id
        fullName
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
      media(first: 5) {
        edges {
          node {
            mediaContentType
            alt
            preview {
              image {
                url
              }
            }
          }
        }
      }
    }
  }
`;

const variantsQuery = `#graphql
  query ProductVariantsMatrix($id: ID!) {
    product(id: $id) {
      id
      title
      options(first: 10) {
        id
        name
        position
        optionValues {
          id
          name
          hasVariants
        }
      }
      variants(first: 10) {
        edges {
          node {
            id
            title
            sku
            barcode
            price
            compareAtPrice
            taxable
            inventoryPolicy
            inventoryQuantity
            selectedOptions {
              name
              value
            }
            inventoryItem {
              id
              tracked
              requiresShipping
              measurement {
                weight {
                  unit
                  value
                }
              }
              countryCodeOfOrigin
              provinceCodeOfOrigin
              harmonizedSystemCode
            }
          }
        }
      }
    }
  }
`;

const searchQuery = `#graphql
  query ProductSearchConformance {
    total: productsCount(limit: null) {
      count
      precision
    }
    nike: products(first: 2, query: "vendor:NIKE status:active", sortKey: TITLE) {
      edges {
        node {
          id
          title
          vendor
          status
          totalInventory
        }
      }
      pageInfo {
        hasNextPage
      }
    }
    lowInventory: products(first: 2, query: "inventory_total:<=5 status:active", sortKey: INVENTORY_TOTAL) {
      edges {
        node {
          id
          title
          vendor
          totalInventory
        }
      }
    }
  }
`;

function buildVariantSearchQuery(field, value) {
  return `#graphql
    query ProductVariantSearchConformance {
      matches: productsCount(query: "${field}:${value}") {
        count
        precision
      }
      products(first: 5, query: "${field}:${value}") {
        edges {
          node {
            id
            title
            handle
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
}

const metafieldsConnectionQuery = `#graphql
  query ProductMetafieldsConnection($id: ID!) {
    product(id: $id) {
      id
      title
      metafields(first: 5) {
        edges {
          cursor
          node {
            id
            namespace
            key
            type
            value
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

const singularMetafieldQuery = `#graphql
  query ProductSingularMetafield($id: ID!, $namespace: String!, $key: String!) {
    product(id: $id) {
      id
      title
      metafield(namespace: $namespace, key: $key) {
        id
        namespace
        key
        type
        value
      }
    }
  }
`;

await mkdir(outputDir, { recursive: true });

const catalog = await runGraphql(catalogQuery);
const sampleProductId = catalog.data?.products?.edges?.[0]?.node?.id;
if (!sampleProductId) {
  throw new Error('Could not find a sample product id from ProductCatalogPage');
}

const detail = await runGraphql(detailQuery, { id: sampleProductId });
const variants = await runGraphql(variantsQuery, { id: sampleProductId });
const search = await runGraphql(searchQuery);
const variantNodes = variants.data?.product?.variants?.edges?.map((edge) => edge?.node).filter(Boolean) ?? [];
const sampleSku = variantNodes.find((variant) => typeof variant?.sku === 'string' && variant.sku.length > 0)?.sku ?? null;
const sampleBarcode = variantNodes.find((variant) => typeof variant?.barcode === 'string' && variant.barcode.length > 0)?.barcode ?? null;
const variantSearch = {
  sku:
    sampleSku
      ? {
          value: sampleSku,
          response: await runGraphql(buildVariantSearchQuery('sku', sampleSku)),
        }
      : null,
  barcode:
    sampleBarcode
      ? {
          value: sampleBarcode,
          response: await runGraphql(buildVariantSearchQuery('barcode', sampleBarcode)),
        }
      : null,
};
const metafieldsConnection = await runGraphql(metafieldsConnectionQuery, { id: sampleProductId });
const firstMetafield = metafieldsConnection.data?.product?.metafields?.edges?.[0]?.node;
const singularMetafield =
  firstMetafield?.namespace && firstMetafield?.key
    ? await runGraphql(singularMetafieldQuery, {
        id: sampleProductId,
        namespace: firstMetafield.namespace,
        key: firstMetafield.key,
      })
    : null;

const captures = {
  'products-catalog-page.json': catalog,
  'product-detail.json': detail,
  'product-variants-matrix.json': variants,
  'products-search.json': search,
  'products-variant-search.json': variantSearch,
  'product-metafields.json': {
    connection: metafieldsConnection,
    singular: singularMetafield,
  },
};

for (const [filename, payload] of Object.entries(captures)) {
  await writeFile(path.join(outputDir, filename), `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      sampleProductId,
      files: Object.keys(captures),
    },
    null,
    2,
  ),
);
