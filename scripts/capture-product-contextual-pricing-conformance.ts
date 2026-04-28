/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const PRICE_LIST_ID = 'gid://shopify/PriceList/31575408946';
const PRODUCT_ID = 'gid://shopify/Product/9801098789170';
const PRODUCT_HANDLE = 'the-inventory-not-tracked-snowboard';
const VARIANT_ID = 'gid://shopify/ProductVariant/49875425296690';
const COUNTRY = 'MX';
const PRICE_QUERY = 'product_id:9801098789170';
const FIXED_PRICE = { amount: '777.77', currencyCode: 'MXN' };
const FIXED_COMPARE_AT_PRICE = { amount: '888.88', currencyCode: 'MXN' };

const fixedPriceMutation = `#graphql
  mutation ProductContextualPricingFixedPriceSetup(
    $priceListId: ID!
    $pricesToAdd: [PriceListProductPriceInput!]!
    $pricesToDeleteByProductIds: [ID!]!
    $priceQuery: String
  ) {
    priceListFixedPricesByProductUpdate(
      priceListId: $priceListId
      pricesToAdd: $pricesToAdd
      pricesToDeleteByProductIds: $pricesToDeleteByProductIds
    ) {
      priceList {
        id
        name
        currency
        fixedPricesCount
        prices(first: 5, query: $priceQuery, originType: FIXED) {
          edges {
            cursor
            node {
              price {
                amount
                currencyCode
              }
              compareAtPrice {
                amount
                currencyCode
              }
              originType
              variant {
                id
                sku
                product {
                  id
                  title
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
      pricesToAddProducts {
        id
        title
      }
      pricesToDeleteProducts {
        id
        title
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const contextualPricingRead = `#graphql
  query ProductContextualPricingRead($productId: ID!, $variantId: ID!, $country: CountryCode!) {
    product(id: $productId) {
      id
      title
      contextualPricing(context: { country: $country }) {
        fixedQuantityRulesCount
        priceRange {
          minVariantPrice {
            amount
            currencyCode
          }
          maxVariantPrice {
            amount
            currencyCode
          }
        }
        minVariantPricing {
          price {
            amount
            currencyCode
          }
          compareAtPrice {
            amount
            currencyCode
          }
          quantityRule {
            minimum
            maximum
            increment
          }
          quantityPriceBreaks(first: 3) {
            edges {
              cursor
              node {
                minimumQuantity
                price {
                  amount
                  currencyCode
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
        maxVariantPricing {
          price {
            amount
            currencyCode
          }
          compareAtPrice {
            amount
            currencyCode
          }
          quantityRule {
            minimum
            maximum
            increment
          }
          quantityPriceBreaks(first: 3) {
            edges {
              cursor
              node {
                minimumQuantity
                price {
                  amount
                  currencyCode
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
    productVariant(id: $variantId) {
      id
      title
      sku
      price
      compareAtPrice
      product {
        id
        title
      }
      contextualPricing(context: { country: $country }) {
        price {
          amount
          currencyCode
        }
        compareAtPrice {
          amount
          currencyCode
        }
        unitPrice {
          amount
          currencyCode
        }
        quantityRule {
          minimum
          maximum
          increment
        }
        quantityPriceBreaks(first: 3) {
          edges {
            cursor
            node {
              minimumQuantity
              price {
                amount
                currencyCode
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
`;

const priceListRead = `#graphql
  query ProductContextualPricingPriceListRead(
    $priceListId: ID!
    $priceQuery: String
    $originType: PriceListPriceOriginType
  ) {
    priceList(id: $priceListId) {
      id
      name
      currency
      fixedPricesCount
      prices(first: 5, query: $priceQuery, originType: $originType) {
        edges {
          cursor
          node {
            price {
              amount
              currencyCode
            }
            compareAtPrice {
              amount
              currencyCode
            }
            originType
            variant {
              id
              sku
              product {
                id
                title
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
  }
`;

const contextualVariables = {
  productId: PRODUCT_ID,
  variantId: VARIANT_ID,
  country: COUNTRY,
};

const fixedPriceReadVariables = {
  priceListId: PRICE_LIST_ID,
  priceQuery: PRICE_QUERY,
  originType: 'FIXED',
};

function fixedPriceMutationVariables(add: boolean) {
  return {
    priceListId: PRICE_LIST_ID,
    priceQuery: PRICE_QUERY,
    pricesToAdd: add
      ? [
          {
            productId: PRODUCT_ID,
            price: FIXED_PRICE,
            compareAtPrice: FIXED_COMPARE_AT_PRICE,
          },
        ]
      : [],
    pricesToDeleteByProductIds: add ? [] : [PRODUCT_ID],
  };
}

function seedProductsFromContextualRead(data: Record<string, unknown>): unknown[] {
  const product = data['product'];
  const variant = data['productVariant'];
  if (!product || typeof product !== 'object' || !variant || typeof variant !== 'object') {
    return [];
  }

  return [
    {
      ...product,
      handle: PRODUCT_HANDLE,
      variants: {
        nodes: [variant],
      },
    },
  ];
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

let cleanupResponse: unknown = null;
try {
  const preCleanup = await runGraphql(fixedPriceMutation, fixedPriceMutationVariables(false));
  const beforeRead = await runGraphql(contextualPricingRead, contextualVariables);
  const addFixedPrice = await runGraphql(fixedPriceMutation, fixedPriceMutationVariables(true));
  const contextualReadAfterAdd = await runGraphql(contextualPricingRead, contextualVariables);
  const fixedPriceReadAfterAdd = await runGraphql(priceListRead, fixedPriceReadVariables);

  cleanupResponse = await runGraphql(fixedPriceMutation, fixedPriceMutationVariables(false));
  const contextualReadAfterCleanup = await runGraphql(contextualPricingRead, contextualVariables);
  const fixedPriceReadAfterCleanup = await runGraphql(priceListRead, fixedPriceReadVariables);

  const data = contextualReadAfterAdd.data as Record<string, unknown>;
  const fixture = {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    scope: 'HAR-412 product and variant contextual pricing read parity for Markets price lists',
    setup: {
      priceListId: PRICE_LIST_ID,
      productId: PRODUCT_ID,
      variantId: VARIANT_ID,
      country: COUNTRY,
      priceQuery: PRICE_QUERY,
      fixedPrice: FIXED_PRICE,
      fixedCompareAtPrice: FIXED_COMPARE_AT_PRICE,
      cleanup: 'The capture deletes the product-level fixed price after reading contextual pricing.',
    },
    schemaEvidence: {
      contextualPricingContextFields: ['country', 'companyLocationId', 'locationId'],
      productContextualPricingFields: [
        'fixedQuantityRulesCount',
        'priceRange',
        'minVariantPricing',
        'maxVariantPricing',
      ],
      variantContextualPricingFields: ['price', 'compareAtPrice', 'unitPrice', 'quantityRule', 'quantityPriceBreaks'],
      downstreamReadTargets: [
        'Product.contextualPricing(context: country)',
        'ProductVariant.contextualPricing(context: country)',
      ],
    },
    seedProducts: seedProductsFromContextualRead(data),
    data,
    successPath: [
      {
        name: 'pre-cleanup product fixed price',
        request: { variables: fixedPriceMutationVariables(false) },
        response: { payload: preCleanup },
      },
      {
        name: 'contextual pricing before fixed price add',
        request: { variables: contextualVariables },
        response: { payload: beforeRead },
      },
      {
        name: 'add product fixed price',
        request: { variables: fixedPriceMutationVariables(true) },
        response: { payload: addFixedPrice },
      },
      {
        name: 'contextual pricing after fixed price add',
        request: { variables: contextualVariables },
        response: { payload: contextualReadAfterAdd },
      },
      {
        name: 'price-list fixed row after fixed price add',
        request: { variables: fixedPriceReadVariables },
        response: { payload: fixedPriceReadAfterAdd },
      },
      {
        name: 'cleanup delete product fixed price',
        request: { variables: fixedPriceMutationVariables(false) },
        response: { payload: cleanupResponse },
      },
      {
        name: 'contextual pricing after cleanup',
        request: { variables: contextualVariables },
        response: { payload: contextualReadAfterCleanup },
      },
      {
        name: 'price-list fixed row after cleanup',
        request: { variables: fixedPriceReadVariables },
        response: { payload: fixedPriceReadAfterCleanup },
      },
    ],
  };

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
  await mkdir(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'product-contextual-pricing-price-list-parity.json');
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  if (!cleanupResponse) {
    cleanupResponse = await runGraphql(fixedPriceMutation, fixedPriceMutationVariables(false));
    console.error(JSON.stringify({ cleanupAfterFailure: cleanupResponse }, null, 2));
  }
  throw error;
}
