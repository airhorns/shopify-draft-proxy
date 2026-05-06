// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'price-list-fixed-prices-by-product-update-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const setupQuery = `#graphql
  query PriceListFixedPricesByProductUpdateValidationSetup($first: Int!) {
    priceLists(first: $first) {
      edges {
        node {
          id
          name
          currency
          fixedPricesCount
        }
      }
    }
    products(first: $first) {
      edges {
        node {
          id
          title
          handle
          variants(first: 1) {
            nodes {
              id
              title
              sku
            }
          }
        }
      }
    }
  }
`;

const validationMutation = `#graphql
  mutation PriceListFixedPricesByProductUpdateValidation(
    $priceListId: ID!
    $pricesToAdd: [PriceListProductPriceInput!]!
    $pricesToDeleteByProductIds: [ID!]!
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
        __typename
        field
        message
        code
      }
    }
  }
`;

function assertGraphqlOk(result, context) {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function nodesFromConnection(connection) {
  return (connection?.edges ?? []).map((edge) => edge?.node).filter(Boolean);
}

function alternateCurrency(currency) {
  return currency === 'USD' ? 'CAD' : 'USD';
}

async function runCase(name, variables) {
  const result = await runGraphqlRequest(validationMutation, variables);
  assertGraphqlOk(result, name);
  return {
    name,
    request: { variables },
    response: {
      status: result.status,
      payload: result.payload,
    },
  };
}

const setup = await runGraphqlRequest(setupQuery, { first: 20 });
assertGraphqlOk(setup, 'setup query');

const priceList = nodesFromConnection(setup.payload?.data?.priceLists).find((node) => node?.id && node?.currency);
const product = nodesFromConnection(setup.payload?.data?.products).find((node) => node?.id);

if (!priceList) {
  throw new Error(`No price list was available for validation capture in ${storeDomain}.`);
}
if (!product) {
  throw new Error(`No product was available for validation capture in ${storeDomain}.`);
}

const priceListId = priceList.id;
const productId = product.id;
const priceListCurrency = priceList.currency;
const mismatchCurrency = alternateCurrency(priceListCurrency);
const matchingPrice = { amount: '12.00', currencyCode: priceListCurrency };
const mismatchedPrice = { amount: '12.00', currencyCode: mismatchCurrency };
const mismatchedCompareAtPrice = { amount: '15.00', currencyCode: mismatchCurrency };

const cases = {
  noOp: await runCase('no update operations specified', {
    priceListId,
    pricesToAdd: [],
    pricesToDeleteByProductIds: [],
  }),
  currencyMismatch: await runCase('pricesToAdd currency mismatch', {
    priceListId,
    pricesToAdd: [
      {
        productId,
        price: mismatchedPrice,
        compareAtPrice: mismatchedCompareAtPrice,
      },
    ],
    pricesToDeleteByProductIds: [],
  }),
  duplicateAdd: await runCase('duplicate pricesToAdd product id', {
    priceListId,
    pricesToAdd: [
      {
        productId,
        price: matchingPrice,
      },
      {
        productId,
        price: matchingPrice,
      },
    ],
    pricesToDeleteByProductIds: [],
  }),
  duplicateDelete: await runCase('duplicate pricesToDeleteByProductIds product id', {
    priceListId,
    pricesToAdd: [],
    pricesToDeleteByProductIds: [productId, productId],
  }),
  mutuallyExclusive: await runCase('product id appears in add and delete inputs', {
    priceListId,
    pricesToAdd: [
      {
        productId,
        price: matchingPrice,
      },
    ],
    pricesToDeleteByProductIds: [productId],
  }),
};

const capture = {
  storeDomain,
  apiVersion,
  capturedAt: new Date().toISOString(),
  scope: 'priceListFixedPricesByProductUpdate validation parity',
  setup: {
    priceListId,
    priceListName: priceList.name,
    priceListCurrency,
    productId,
    productTitle: product.title,
    mismatchCurrency,
    cleanup: 'All captured mutations are validation failures and do not stage fixed prices in Shopify.',
  },
  cases,
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
