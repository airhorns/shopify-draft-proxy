import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'products-common-search-filters.json');
const requestPath = path.join('config', 'parity-requests', 'products', 'products-common-search-filters-read.graphql');
const document = await readFile(requestPath, 'utf8');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const seedQuery = `#graphql
  query ProductsCommonSearchFilterSeed {
    products(first: 50, query: "status:ACTIVE", sortKey: UPDATED_AT, reverse: true) {
      nodes {
        id
        title
        status
        vendor
        productType
      }
    }
  }
`;

function quoteSearchValue(value: string): string {
  const escaped = value.replaceAll('\\', '\\\\').replaceAll('"', '\\"');
  return `"${escaped}"`;
}

const seedResponse = await runGraphql(seedQuery);
const seedData = seedResponse.data as
  | {
      products?: {
        nodes?: Array<{
          id?: string;
          title?: string;
          status?: string;
          vendor?: string;
          productType?: string;
        }>;
      };
    }
  | undefined;
const seedProducts = seedData?.products?.nodes ?? [];
const vendorTypeProduct = seedProducts.find(
  (
    product,
  ): product is {
    id?: string;
    title?: string;
    status?: string;
    vendor: string;
    productType: string;
  } => Boolean(product.status === 'ACTIVE' && product.vendor && product.productType),
);

if (!vendorTypeProduct) {
  throw new Error('Could not find an ACTIVE product with both vendor and productType for search filter capture.');
}

const variables = {
  statusQuery: 'status:ACTIVE',
  vendorTypeQuery: `vendor:${quoteSearchValue(vendorTypeProduct.vendor)} product_type:${quoteSearchValue(
    vendorTypeProduct.productType,
  )}`,
};
const response = await runGraphql(document, variables);
const upstreamCalls = [
  {
    operationName: 'ProductsCommonSearchFiltersRead',
    variables,
    query: document,
    response: {
      status: 200,
      body: response,
    },
  },
];

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      variables,
      preconditionRead: {
        query: seedQuery,
        variables: {},
        selectedProduct: vendorTypeProduct,
        response: seedResponse,
      },
      response,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

// oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
console.log(JSON.stringify({ ok: true, outputPath, variables }, null, 2));
