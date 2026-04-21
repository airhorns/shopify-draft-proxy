import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);

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
              inventoryLevels(first: 5) {
                edges {
                  cursor
                  node {
                    id
                    location {
                      id
                      name
                    }
                    quantities(names: ["available", "on_hand", "incoming"]) {
                      name
                      quantity
                      updatedAt
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

const searchGrammarVariables = {
  phraseQuery: '"flat peak cap" accessories -vendor:VANS -tag:vans',
};

const searchGrammarQuery = `#graphql
  query ProductSearchGrammarConformance($phraseQuery: String!) {
    phraseCount: productsCount(query: $phraseQuery) {
      count
      precision
    }
    phraseMatches: products(first: 5, query: $phraseQuery) {
      edges {
        node {
          id
          title
          vendor
          productType
          tags
        }
      }
    }
  }
`;

const advancedSearchQuery = `#graphql
  query ProductAdvancedSearchConformance {
    prefixCount: productsCount(query: "swoo* status:active") {
      count
      precision
    }
    prefix: products(first: 3, query: "swoo* status:active", sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          title
          handle
          vendor
          productType
          tags
          updatedAt
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    orCount: productsCount(query: "(vendor:NIKE OR vendor:VANS) tag:egnition-sample-data product_type:ACCESSORIES") {
      count
      precision
    }
    orMatches: products(first: 5, query: "(vendor:NIKE OR vendor:VANS) tag:egnition-sample-data product_type:ACCESSORIES", sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          title
          handle
          vendor
          productType
          tags
          updatedAt
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    groupedExclusionCount: productsCount(query: "tag:egnition-sample-data product_type:ACCESSORIES -(vendor:VANS)") {
      count
      precision
    }
    groupedExclusion: products(first: 5, query: "tag:egnition-sample-data product_type:ACCESSORIES -(vendor:VANS)", sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          title
          handle
          vendor
          productType
          tags
          updatedAt
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

const searchPaginationBaseVariables = {
  query: 'tag:egnition-sample-data product_type:ACCESSORIES',
};

const searchPaginationFirstPageQuery = `#graphql
  query ProductSearchPaginationFirstPage($query: String!) {
    count: productsCount(query: $query) {
      count
      precision
    }
    firstPage: products(first: 1, query: $query, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          title
          handle
          vendor
          productType
          tags
          updatedAt
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

const searchPaginationNextPageQuery = `#graphql
  query ProductSearchPaginationNextPage($query: String!, $afterCursor: String!) {
    nextPage: products(first: 1, after: $afterCursor, query: $query, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          title
          handle
          vendor
          productType
          tags
          updatedAt
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

const searchPaginationPreviousPageQuery = `#graphql
  query ProductSearchPaginationPreviousPage($query: String!, $beforeCursor: String!) {
    previousPage: products(last: 1, before: $beforeCursor, query: $query, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          title
          handle
          vendor
          productType
          tags
          updatedAt
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

const orPrecedenceVariables = {
  precedenceQuery: 'vendor:NIKE OR vendor:VANS tag:egnition-sample-data product_type:ACCESSORIES',
};

const orPrecedenceQuery = `#graphql
  query ProductOrPrecedenceConformance($precedenceQuery: String!) {
    precedenceCount: productsCount(query: $precedenceQuery) {
      count
      precision
    }
    precedenceMatches: products(first: 10, query: $precedenceQuery, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          title
          handle
          vendor
          productType
          tags
          updatedAt
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

const sortKeysQuery = `#graphql
  query ProductSortKeysConformance($query: String!) {
    titleOrder: products(first: 5, query: $query, sortKey: TITLE) {
      edges {
        cursor
        node {
          id
          title
          handle
          status
          vendor
          productType
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    vendorOrder: products(first: 5, query: $query, sortKey: VENDOR) {
      edges {
        cursor
        node {
          id
          title
          handle
          status
          vendor
          productType
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    productTypeOrder: products(first: 5, query: $query, sortKey: PRODUCT_TYPE, reverse: true) {
      edges {
        cursor
        node {
          id
          title
          handle
          status
          vendor
          productType
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    publishedAtOrder: products(first: 5, query: $query, sortKey: PUBLISHED_AT, reverse: true) {
      edges {
        cursor
        node {
          id
          title
          publishedAt
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    idOrder: products(first: 5, query: $query, sortKey: ID, reverse: true) {
      edges {
        cursor
        node {
          id
          legacyResourceId
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

const relevanceSearchQuery = `#graphql
  query ProductRelevanceSearchConformance($first: Int!, $query: String!) {
    products(first: $first, query: $query, sortKey: RELEVANCE) {
      edges {
        cursor
        node {
          id
          legacyResourceId
          title
          handle
          vendor
          productType
          tags
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

const emptyStateQuery = `#graphql
  query ProductEmptyStateConformance($missingId: ID!, $emptyQuery: String!) {
    missingProduct: product(id: $missingId) {
      id
      title
    }
    emptyCount: productsCount(query: $emptyQuery) {
      count
      precision
    }
    emptyProducts: products(first: 3, query: $emptyQuery, sortKey: UPDATED_AT, reverse: true) {
      edges {
        cursor
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

await mkdir(outputDir, { recursive: true });

const catalog = await runGraphql(catalogQuery);
const sampleProductId = catalog.data?.products?.edges?.[0]?.node?.id;
if (!sampleProductId) {
  throw new Error('Could not find a sample product id from ProductCatalogPage');
}

const detail = await runGraphql(detailQuery, { id: sampleProductId });
const variants = await runGraphql(variantsQuery, { id: sampleProductId });
const search = await runGraphql(searchQuery);
const searchGrammar = {
  variables: searchGrammarVariables,
  response: await runGraphql(searchGrammarQuery, searchGrammarVariables),
};
const advancedSearch = await runGraphql(advancedSearchQuery);
const searchPaginationFirstPage = await runGraphql(searchPaginationFirstPageQuery, searchPaginationBaseVariables);
const searchPaginationAfterCursor = searchPaginationFirstPage.data?.firstPage?.pageInfo?.endCursor;
if (typeof searchPaginationAfterCursor !== 'string' || searchPaginationAfterCursor.length === 0) {
  throw new Error('Could not derive the filtered search after cursor for pagination conformance capture.');
}
const searchPaginationNextPage = await runGraphql(searchPaginationNextPageQuery, {
  ...searchPaginationBaseVariables,
  afterCursor: searchPaginationAfterCursor,
});
const searchPaginationBeforeCursor = searchPaginationNextPage.data?.nextPage?.pageInfo?.startCursor;
if (typeof searchPaginationBeforeCursor !== 'string' || searchPaginationBeforeCursor.length === 0) {
  throw new Error('Could not derive the filtered search before cursor for pagination conformance capture.');
}
const searchPaginationPreviousPage = await runGraphql(searchPaginationPreviousPageQuery, {
  ...searchPaginationBaseVariables,
  beforeCursor: searchPaginationBeforeCursor,
});
const searchPagination = {
  variables: {
    ...searchPaginationBaseVariables,
    afterCursor: searchPaginationAfterCursor,
    beforeCursor: searchPaginationBeforeCursor,
  },
  response: {
    data: {
      count: searchPaginationFirstPage.data?.count ?? null,
      firstPage: searchPaginationFirstPage.data?.firstPage ?? null,
      nextPage: searchPaginationNextPage.data?.nextPage ?? null,
      previousPage: searchPaginationPreviousPage.data?.previousPage ?? null,
    },
  },
};
const orPrecedence = {
  variables: orPrecedenceVariables,
  response: await runGraphql(orPrecedenceQuery, orPrecedenceVariables),
};
const sortKeysVariables = {
  query: 'egnition-sample-data status:active',
};
const sortKeys = {
  variables: sortKeysVariables,
  response: await runGraphql(sortKeysQuery, sortKeysVariables),
};
const relevanceSearchVariables = {
  first: 3,
  query: 'swoo* status:active',
};
const relevanceSearch = {
  variables: relevanceSearchVariables,
  response: await runGraphql(relevanceSearchQuery, relevanceSearchVariables),
};
const variantNodes = variants.data?.product?.variants?.edges?.map((edge) => edge?.node).filter(Boolean) ?? [];
const sampleSku =
  variantNodes.find((variant) => typeof variant?.sku === 'string' && variant.sku.length > 0)?.sku ?? null;
const sampleBarcode =
  variantNodes.find((variant) => typeof variant?.barcode === 'string' && variant.barcode.length > 0)?.barcode ?? null;
const variantSearch = {
  sku: sampleSku
    ? {
        value: sampleSku,
        response: await runGraphql(buildVariantSearchQuery('sku', sampleSku)),
      }
    : null,
  barcode: sampleBarcode
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
const emptyStateVariables = {
  missingId: 'gid://shopify/Product/999999999999999',
  emptyQuery: 'title:__hermes_empty_catalog_probe__',
};
const emptyState = {
  variables: emptyStateVariables,
  response: await runGraphql(emptyStateQuery, emptyStateVariables),
};

const captures = {
  'products-catalog-page.json': catalog,
  'product-detail.json': detail,
  'product-empty-state.json': emptyState,
  'product-variants-matrix.json': variants,
  'products-search.json': search,
  'products-search-grammar.json': searchGrammar,
  'products-advanced-search.json': advancedSearch,
  'products-search-pagination.json': searchPagination,
  'products-or-precedence.json': orPrecedence,
  'products-sort-keys.json': sortKeys,
  'products-relevance-search.json': relevanceSearch,
  'products-variant-search.json': variantSearch,
  'product-metafields.json': {
    connection: metafieldsConnection,
    singular: singularMetafield,
  },
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
      sampleProductId,
      files: Object.keys(captures),
    },
    null,
    2,
  ),
);
