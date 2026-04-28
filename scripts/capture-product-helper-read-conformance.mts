// @ts-nocheck
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const helperRootNames = [
  'productByIdentifier',
  'productVariantByIdentifier',
  'productVariants',
  'productVariantsCount',
  'productTags',
  'productTypes',
  'productVendors',
  'productSavedSearches',
  'productOperation',
  'productDuplicateJob',
  'productResourceFeedback',
  'bulkProductResourceFeedbackCreate',
  'shopResourceFeedbackCreate',
];

const schemaQuery = `#graphql
  query ProductHelperRootSchema {
    queryRoot: __type(name: "QueryRoot") {
      fields {
        name
        args {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                }
              }
            }
          }
          defaultValue
        }
        type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
            }
          }
        }
      }
    }
    mutationRoot: __type(name: "Mutation") {
      fields {
        name
        args {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                }
              }
            }
          }
          defaultValue
        }
        type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
            }
          }
        }
      }
    }
  }
`;

const seedQuery = `#graphql
  query ProductHelperSeed {
    products(first: 3, sortKey: UPDATED_AT, reverse: true) {
      nodes {
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
        publishedAt
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
        variants(first: 3) {
          nodes {
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

const helperQuery = `#graphql
  query ProductHelperRoots(
    $productId: ID!
    $productHandle: String!
    $variantId: ID!
    $missingProductId: ID!
    $missingVariantId: ID!
    $missingJobId: ID!
    $missingOperationId: ID!
  ) {
    byId: productByIdentifier(identifier: { id: $productId }) {
      id
      handle
      title
    }
    byHandle: productByIdentifier(identifier: { handle: $productHandle }) {
      id
      handle
      title
    }
    missingProduct: productByIdentifier(identifier: { id: $missingProductId }) {
      id
      handle
      title
    }
    variantById: productVariantByIdentifier(identifier: { id: $variantId }) {
      id
      title
      sku
      product {
        id
      }
    }
    missingVariant: productVariantByIdentifier(identifier: { id: $missingVariantId }) {
      id
      title
    }
    productVariants(first: 2, sortKey: ID) {
      nodes {
        id
        title
        sku
        product {
          id
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    productVariantsCount {
      count
      precision
    }
    productTags(first: 3) {
      nodes
      edges {
        cursor
        node
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    productTypes(first: 3) {
      nodes
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    productVendors(first: 3) {
      nodes
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    productSavedSearches(first: 3) {
      nodes {
        id
        name
        query
        searchTerms
        resourceType
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    productResourceFeedback(id: $productId) {
      productId
      state
      messages
      feedbackGeneratedAt
      productUpdatedAt
    }
    productOperation(id: $missingOperationId) {
      __typename
      status
      product {
        id
      }
      ... on ProductSetOperation {
        id
        userErrors {
          field
          message
        }
      }
    }
    productDuplicateJob(id: $missingJobId) {
      id
      done
    }
  }
`;

function readFirstSeedProduct(seedPayload) {
  const products = seedPayload?.data?.products?.nodes;
  if (!Array.isArray(products) || products.length === 0) {
    throw new Error('Product helper capture requires at least one product in the conformance shop.');
  }

  const product = products.find(
    (candidate) => Array.isArray(candidate?.variants?.nodes) && candidate.variants.nodes[0],
  );
  if (!product?.id || !product?.handle || !product?.variants?.nodes?.[0]?.id) {
    throw new Error('Product helper capture requires a product with handle and at least one variant.');
  }

  return product;
}

function filterHelperFields(fields) {
  return fields.filter((field) => helperRootNames.includes(field.name));
}

const schemaResponse = await runGraphqlRequest(schemaQuery);
const seedResponse = await runGraphqlRequest(seedQuery);
const seedProduct = readFirstSeedProduct(seedResponse.payload);
const variables = {
  productId: seedProduct.id,
  productHandle: seedProduct.handle,
  variantId: seedProduct.variants.nodes[0].id,
  missingProductId: 'gid://shopify/Product/999999999999',
  missingVariantId: 'gid://shopify/ProductVariant/999999999999',
  missingJobId: 'gid://shopify/ProductDuplicateJob/999999999999',
  missingOperationId: 'gid://shopify/ProductSetOperation/999999999999',
};
const helperResponse = await runGraphqlRequest(helperQuery, variables);

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  roots: helperRootNames,
  schema: {
    queryRoot: {
      fields: filterHelperFields(schemaResponse.payload?.data?.queryRoot?.fields ?? []),
    },
    mutationRoot: {
      fields: filterHelperFields(schemaResponse.payload?.data?.mutationRoot?.fields ?? []),
    },
  },
  seed: {
    query: seedQuery,
    response: seedResponse.payload,
  },
  seedProducts: seedResponse.payload?.data?.products?.nodes ?? [],
  request: {
    query: helperQuery,
    variables,
  },
  response: {
    status: helperResponse.status,
    payload: helperResponse.payload,
  },
};

await mkdir(outputDir, { recursive: true });
const outputPath = path.join(outputDir, 'product-helper-roots-read.json');
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

// oxlint-disable-next-line no-console -- CLI capture output is intentionally written to stdout.
console.log(JSON.stringify({ ok: true, outputPath, storeDomain, apiVersion }, null, 2));
