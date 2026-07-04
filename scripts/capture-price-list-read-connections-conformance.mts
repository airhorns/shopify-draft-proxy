/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const requestDir = path.join('config', 'parity-requests', 'markets');

const priceListsFirstPageOutputPath = path.join(outputDir, 'price-lists-window-first-page.json');
const priceListsAfterOutputPath = path.join(outputDir, 'price-lists-window-after.json');
const priceListPricesOutputPath = path.join(outputDir, 'price-list-prices-window-filtered.json');

const priceListsFirstPageVariablesPath = path.join(requestDir, 'price-lists-window-first-page.variables.json');
const priceListsAfterVariablesPath = path.join(requestDir, 'price-lists-window-after.variables.json');
const priceListPricesVariablesPath = path.join(requestDir, 'price-list-prices-window-filtered.variables.json');

const priceListsFirstPageDocument = `query PriceListsWindowFirstPage($first: Int!) {
  priceLists(first: $first) {
    edges {
      cursor
      node {
        __typename
        id
        name
        currency
        fixedPricesCount
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

const priceListsAfterDocument = `query PriceListsWindowAfter($first: Int!, $after: String!) {
  priceLists(first: $first, after: $after) {
    edges {
      cursor
      node {
        __typename
        id
        name
        currency
        fixedPricesCount
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

const priceListPricesDocument = `query PriceListPricesWindowFiltered(
  $priceListId: ID!
  $first: Int!
  $priceQuery: String
  $originType: PriceListPriceOriginType
) {
  priceList(id: $priceListId) {
    __typename
    id
    name
    currency
    prices(first: $first, query: $priceQuery, originType: $originType) {
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

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function objectValue(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label} was not an object`);
  }
  return value as Record<string, unknown>;
}

function arrayValue(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} was not an array`);
  }
  return value;
}

function stringValue(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was not a non-empty string`);
  }
  return value;
}

function boolValue(value: unknown, label: string): boolean {
  if (typeof value !== 'boolean') {
    throw new Error(`${label} was not a boolean`);
  }
  return value;
}

function priceListsConnection(result: ConformanceGraphqlResult, label: string): Record<string, unknown> {
  const data = objectValue(result.payload.data, `${label}.data`);
  return objectValue(data['priceLists'], `${label}.data.priceLists`);
}

function priceListPricesConnection(result: ConformanceGraphqlResult, label: string): Record<string, unknown> {
  const data = objectValue(result.payload.data, `${label}.data`);
  const priceList = objectValue(data['priceList'], `${label}.data.priceList`);
  return objectValue(priceList['prices'], `${label}.data.priceList.prices`);
}

function upstreamCall(
  operationName: string,
  variables: Record<string, unknown>,
  query: string,
  result: ConformanceGraphqlResult,
) {
  return {
    operationName,
    variables,
    query,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

function captureEnvelope(
  operationName: string,
  variables: Record<string, unknown>,
  query: string,
  result: ConformanceGraphqlResult,
) {
  return {
    ...result.payload,
    upstreamCalls: [upstreamCall(operationName, variables, query, result)],
  };
}

async function writeJson(filePath: string, value: unknown): Promise<void> {
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

const priceListId = process.env['SHOPIFY_CONFORMANCE_PRICE_LIST_ID'] ?? 'gid://shopify/PriceList/31575408946';
const productLegacyId = process.env['SHOPIFY_CONFORMANCE_PRICE_LIST_PRODUCT_LEGACY_ID'] ?? '9801098821938';

const firstPageVariables = { first: 2 };
const firstPage = await runGraphqlRequest(priceListsFirstPageDocument, firstPageVariables);
assertGraphqlOk('priceLists first page', firstPage);
const firstPageConnection = priceListsConnection(firstPage, 'priceLists first page');
const firstPageEdges = arrayValue(firstPageConnection['edges'], 'priceLists first page edges');
if (firstPageEdges.length !== 2) {
  throw new Error(`priceLists first page expected exactly 2 edges, got ${firstPageEdges.length}`);
}
const firstPageInfo = objectValue(firstPageConnection['pageInfo'], 'priceLists first page pageInfo');
if (!boolValue(firstPageInfo['hasNextPage'], 'priceLists first page hasNextPage')) {
  throw new Error('priceLists first page should have a next page for the configured conformance store');
}
const afterCursor = stringValue(firstPageInfo['endCursor'], 'priceLists first page endCursor');

const afterVariables = { first: 1, after: afterCursor };
const afterPage = await runGraphqlRequest(priceListsAfterDocument, afterVariables);
assertGraphqlOk('priceLists after page', afterPage);
const afterConnection = priceListsConnection(afterPage, 'priceLists after page');
const afterEdges = arrayValue(afterConnection['edges'], 'priceLists after page edges');
if (afterEdges.length !== 1) {
  throw new Error(`priceLists after page expected exactly 1 edge, got ${afterEdges.length}`);
}
const afterPageInfo = objectValue(afterConnection['pageInfo'], 'priceLists after page pageInfo');
if (!boolValue(afterPageInfo['hasPreviousPage'], 'priceLists after page hasPreviousPage')) {
  throw new Error('priceLists after page should report a previous page');
}

const priceVariables = {
  priceListId,
  first: 1,
  priceQuery: `product_id:${productLegacyId}`,
  originType: 'RELATIVE',
};
const priceListPrices = await runGraphqlRequest(priceListPricesDocument, priceVariables);
assertGraphqlOk('priceList prices windowed filtered read', priceListPrices);
const pricesConnection = priceListPricesConnection(priceListPrices, 'priceList prices windowed filtered read');
const priceEdges = arrayValue(pricesConnection['edges'], 'priceList prices windowed filtered read edges');
if (priceEdges.length !== 1) {
  throw new Error(`priceList prices filtered read expected exactly 1 edge, got ${priceEdges.length}`);
}
const pricesPageInfo = objectValue(pricesConnection['pageInfo'], 'priceList prices windowed filtered read pageInfo');
if (!boolValue(pricesPageInfo['hasNextPage'], 'priceList prices windowed filtered read hasNextPage')) {
  throw new Error('priceList prices filtered read should have a next page for the configured product_id filter');
}

await mkdir(outputDir, { recursive: true });
await mkdir(requestDir, { recursive: true });
await writeJson(priceListsFirstPageVariablesPath, firstPageVariables);
await writeJson(priceListsAfterVariablesPath, afterVariables);
await writeJson(priceListPricesVariablesPath, priceVariables);
await writeJson(
  priceListsFirstPageOutputPath,
  captureEnvelope('PriceListsWindowFirstPage', firstPageVariables, priceListsFirstPageDocument, firstPage),
);
await writeJson(
  priceListsAfterOutputPath,
  captureEnvelope('PriceListsWindowAfter', afterVariables, priceListsAfterDocument, afterPage),
);
await writeJson(
  priceListPricesOutputPath,
  captureEnvelope('PriceListPricesWindowFiltered', priceVariables, priceListPricesDocument, priceListPrices),
);

console.log(`Wrote price-list connection captures under ${outputDir}`);
