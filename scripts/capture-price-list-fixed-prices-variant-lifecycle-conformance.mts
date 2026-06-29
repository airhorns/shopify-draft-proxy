/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase = {
  name: string;
  request: {
    variables: Record<string, unknown>;
  };
  response: ConformanceGraphqlResult;
};

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

const priceListId = process.env['SHOPIFY_CONFORMANCE_PRICE_LIST_ID'] ?? 'gid://shopify/PriceList/31575376178';
const productId = process.env['SHOPIFY_CONFORMANCE_PRODUCT_ID'] ?? 'gid://shopify/Product/9801098789170';
const variantId =
  process.env['SHOPIFY_CONFORMANCE_PRODUCT_VARIANT_ID'] ?? 'gid://shopify/ProductVariant/49875425296690';
const fixedPrice = { amount: '812.34', currencyCode: 'CAD' };
const fixedCompareAtPrice = { amount: '845.67', currencyCode: 'CAD' };
const updatedFixedPrice = { amount: '818.88', currencyCode: 'CAD' };
const updatedCompareAtPrice = { amount: '849.99', currencyCode: 'CAD' };

const addDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-fixed-prices-add.graphql'),
  'utf8',
);
const updateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-fixed-prices-update.graphql'),
  'utf8',
);
const deleteDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-fixed-prices-delete.graphql'),
  'utf8',
);
const readDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-fixed-prices-read.graphql'),
  'utf8',
);

const preflightDocument =
  'query MarketsMutationPreflightHydrate($priceListId: ID!, $variantIds: [ID!]!) { priceList(id: $priceListId) { __typename id name currency fixedPricesCount prices(first: 20, originType: FIXED) { edges { cursor node { price { amount currencyCode } compareAtPrice { amount currencyCode } originType variant { id sku product { id title } } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } productVariants: nodes(ids: $variantIds) { __typename ... on ProductVariant { id title sku price compareAtPrice product { id title handle status variants(first: 10) { nodes { id title sku price compareAtPrice } } } } } }';

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result)}`);
  }
}

async function captureCase(name: string, document: string, variables: Record<string, unknown>): Promise<CapturedCase> {
  const result = await runGraphqlRequest(document, variables);
  assertGraphqlOk(name, result);
  return {
    name,
    request: { variables },
    response: result,
  };
}

async function cleanupFixedPrice(label: string): Promise<ConformanceGraphqlResult> {
  const result = await runGraphqlRequest(deleteDocument, {
    priceListId,
    variantIds: [variantId],
  });
  assertGraphqlOk(label, result);
  return result;
}

function pushUniqueString(values: string[], candidate: unknown): void {
  if (typeof candidate === 'string' && candidate.length > 0 && !values.includes(candidate)) {
    values.push(candidate);
  }
}

function collectInputVariantIds(values: string[], inputs: unknown): void {
  if (!Array.isArray(inputs)) return;
  for (const input of inputs) {
    if (typeof input === 'object' && input !== null && !Array.isArray(input)) {
      pushUniqueString(values, (input as { variantId?: unknown }).variantId);
    }
  }
}

function collectVariantIds(values: string[], ids: unknown): void {
  if (!Array.isArray(ids)) return;
  for (const id of ids) {
    pushUniqueString(values, id);
  }
}

function fixedPriceVariantPreflightVariables(variables: Record<string, unknown>): Record<string, unknown> {
  const variantIds: string[] = [];
  collectInputVariantIds(variantIds, variables['prices']);
  collectInputVariantIds(variantIds, variables['pricesToAdd']);
  collectVariantIds(variantIds, variables['variantIds']);
  collectVariantIds(variantIds, variables['variantIdsToDelete']);
  return {
    ...variables,
    priceListId: variables['priceListId'] ?? null,
    variantIds,
  };
}

function preflightCall(variables: Record<string, unknown>, response: ConformanceGraphqlResult) {
  return {
    operationName: 'MarketsMutationPreflightHydrate',
    variables,
    query: preflightDocument,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

let finalCleanup: ConformanceGraphqlResult | null = null;

try {
  const preCleanup = await cleanupFixedPrice('pre-cleanup variant fixed price');
  const baselineVariables = { priceListId, variantIds: [variantId] };
  const baseline = await runGraphqlRequest(preflightDocument, baselineVariables);
  assertGraphqlOk('preflight baseline', baseline);

  const addVariables = {
    priceListId,
    prices: [
      {
        variantId,
        price: fixedPrice,
        compareAtPrice: fixedCompareAtPrice,
      },
    ],
  };
  const readVariables = { priceListId };
  const updateVariables = {
    priceListId,
    pricesToAdd: [
      {
        variantId,
        price: updatedFixedPrice,
        compareAtPrice: updatedCompareAtPrice,
      },
    ],
    variantIdsToDelete: [],
  };
  const deleteVariables = {
    priceListId,
    variantIds: [variantId],
  };

  const successPath = [
    await captureCase('add variant fixed price', addDocument, addVariables),
    await captureCase('read after add', readDocument, readVariables),
    await captureCase('update variant fixed price', updateDocument, updateVariables),
    await captureCase('read after update', readDocument, readVariables),
    await captureCase('delete variant fixed price', deleteDocument, deleteVariables),
    await captureCase('read after delete', readDocument, readVariables),
  ];

  finalCleanup = await cleanupFixedPrice('final cleanup variant fixed price');

  const fixture = {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    scope: 'Variant-level price-list fixed-price add/update/delete lifecycle parity',
    setup: {
      priceListId,
      productId,
      variantId,
      fixedPrice,
      fixedCompareAtPrice,
      updatedFixedPrice,
      updatedCompareAtPrice,
      cleanup:
        'The capture deletes the variant-level fixed price before and after recording the add/update/delete lifecycle.',
    },
    schemaEvidence: {
      mutationArgs: {
        priceListFixedPricesAdd: ['priceListId', 'prices'],
        priceListFixedPricesUpdate: ['priceListId', 'pricesToAdd', 'variantIdsToDelete'],
        priceListFixedPricesDelete: ['priceListId', 'variantIds'],
      },
      payloadFields: {
        priceListFixedPricesAdd: ['prices', 'userErrors'],
        priceListFixedPricesUpdate: ['priceList', 'pricesAdded', 'deletedFixedPriceVariantIds', 'userErrors'],
        priceListFixedPricesDelete: ['deletedFixedPriceVariantIds', 'userErrors'],
      },
      downstreamReadTargets: ['PriceList.prices(originType: FIXED)'],
    },
    cleanup: {
      preCleanup,
      finalCleanup,
    },
    successPath,
    upstreamCalls: [
      preflightCall(fixedPriceVariantPreflightVariables(addVariables), baseline),
      preflightCall(fixedPriceVariantPreflightVariables(updateVariables), baseline),
      preflightCall(fixedPriceVariantPreflightVariables(deleteVariables), baseline),
    ],
  };

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
  await mkdir(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'price-list-fixed-prices-variant-lifecycle.json');
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        storeDomain,
        apiVersion,
        successPath: successPath.map((entry) => entry.name),
      },
      null,
      2,
    ),
  );
} catch (error) {
  if (!finalCleanup) {
    const cleanupAfterFailure = await cleanupFixedPrice('cleanup after failure');
    console.error(JSON.stringify({ cleanupAfterFailure }, null, 2));
  }
  throw error;
}
