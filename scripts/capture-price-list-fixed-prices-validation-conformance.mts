/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase = {
  name: string;
  operation: 'priceListFixedPricesAdd' | 'priceListFixedPricesUpdate' | 'priceListFixedPricesDelete';
  request: {
    variables: Record<string, unknown>;
  };
  response: ConformanceGraphqlResult;
};

type SetupProduct = {
  id: string;
  title?: string;
  variants?: {
    nodes?: Array<{
      id?: string;
      title?: string;
      sku?: string | null;
    } | null>;
  } | null;
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

const addDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-fixed-prices-add-validation.graphql'),
  'utf8',
);
const addShortCircuitDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price_list_fixed_prices_add_short_circuit.graphql'),
  'utf8',
);
const updateDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-fixed-prices-update-validation.graphql'),
  'utf8',
);
const deleteDocument = await readFile(
  path.join('config', 'parity-requests', 'markets', 'price-list-fixed-prices-delete-validation.graphql'),
  'utf8',
);

const setupQuery = `#graphql
  query PriceListFixedPricesValidationSetup($first: Int!) {
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
          status
          variants(first: 5) {
            nodes {
              id
              title
              sku
              price
              compareAtPrice
            }
          }
        }
      }
    }
  }
`;

const preflightDocument =
  'query MarketsMutationPreflightHydrate($priceListId: ID!, $variantIds: [ID!]!) { priceList(id: $priceListId) { __typename id name currency fixedPricesCount prices(first: 20, originType: FIXED) { edges { cursor node { price { amount currencyCode } compareAtPrice { amount currencyCode } originType variant { id sku product { id title } } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } productVariants: nodes(ids: $variantIds) { __typename ... on ProductVariant { id title sku price compareAtPrice product { id title handle status variants(first: 10) { nodes { id title sku price compareAtPrice } } } } } }';

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function nodesFromConnection(connection: unknown): Array<Record<string, unknown>> {
  const edges = (connection as { edges?: Array<{ node?: Record<string, unknown> | null } | null> } | null)?.edges ?? [];
  return edges.map((edge) => edge?.node).filter((node): node is Record<string, unknown> => Boolean(node));
}

function productWithVariant(product: Record<string, unknown>): product is SetupProduct {
  const variants = (product as SetupProduct).variants?.nodes ?? [];
  return typeof product.id === 'string' && variants.some((variant) => typeof variant?.id === 'string');
}

function alternateCurrency(currency: string): string {
  return currency === 'USD' ? 'CAD' : 'USD';
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

const upstreamCalls: Array<ReturnType<typeof preflightCall>> = [];

async function captureCase(
  operation: CapturedCase['operation'],
  name: string,
  document: string,
  variables: Record<string, unknown>,
): Promise<CapturedCase> {
  const preflightVariables = fixedPriceVariantPreflightVariables(variables);
  const preflight = await runGraphqlRequest(preflightDocument, preflightVariables);
  assertGraphqlOk(`${name} preflight`, preflight);
  upstreamCalls.push(preflightCall(preflightVariables, preflight));

  const result = await runGraphqlRequest(document, variables);
  assertGraphqlOk(name, result);
  return {
    name,
    operation,
    request: { variables },
    response: result,
  };
}

async function cleanupFixedPrice(
  label: string,
  priceListId: string,
  variantId: string,
): Promise<ConformanceGraphqlResult> {
  const result = await runGraphqlRequest(deleteDocument, {
    priceListId,
    variantIds: [variantId],
  });
  assertGraphqlOk(label, result);
  return result;
}

async function seedFixedPrice(
  label: string,
  priceListId: string,
  variantId: string,
  price: { amount: string; currencyCode: string },
): Promise<ConformanceGraphqlResult> {
  const result = await runGraphqlRequest(addDocument, {
    priceListId,
    prices: [{ variantId, price }],
  });
  assertGraphqlOk(label, result);
  const userErrors = result.payload.data?.priceListFixedPricesAdd?.userErrors ?? [];
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
  return result;
}

const setup = await runGraphqlRequest(setupQuery, { first: 20 });
assertGraphqlOk('setup query', setup);

const priceList = nodesFromConnection(setup.payload.data?.priceLists).find(
  (node) => typeof node['id'] === 'string' && typeof node['currency'] === 'string',
);
const product = nodesFromConnection(setup.payload.data?.products).find(productWithVariant);
const variant = product?.variants?.nodes?.find((node) => typeof node?.id === 'string') ?? null;

if (!priceList) {
  throw new Error(`No price list was available for validation capture in ${storeDomain}.`);
}
if (!product || !variant?.id) {
  throw new Error(`No product variant was available for validation capture in ${storeDomain}.`);
}

const priceListId = priceList['id'] as string;
const priceListName = priceList['name'] as string | undefined;
const priceListCurrency = priceList['currency'] as string;
const productId = product.id;
const productTitle = product.title;
const variantId = variant.id;
const variantTitle = variant.title;
const secondVariant =
  product.variants?.nodes?.find((node) => typeof node?.id === 'string' && node.id !== variantId) ?? variant;
const secondVariantId = secondVariant.id as string;
const secondVariantTitle = secondVariant.title;
const missingPriceListId = 'gid://shopify/PriceList/999999999999999';
const missingVariantId = 'gid://shopify/ProductVariant/999999999999999';
const mismatchCurrency = alternateCurrency(priceListCurrency);
const matchingPrice = { amount: '812.34', currencyCode: priceListCurrency };
const matchingPriceSecond = { amount: '813.45', currencyCode: priceListCurrency };
const mismatchedCompareAtPrice = { amount: '912.34', currencyCode: mismatchCurrency };
const mismatchedCompareAtPriceSecond = { amount: '913.45', currencyCode: mismatchCurrency };
const updatedPrice = { amount: '818.88', currencyCode: priceListCurrency };
const updatedPriceSecond = { amount: '819.99', currencyCode: priceListCurrency };
const mismatchedPrice = { amount: '712.34', currencyCode: mismatchCurrency };
const seedPrice = { amount: '801.23', currencyCode: priceListCurrency };

let finalCleanup: ConformanceGraphqlResult | null = null;

try {
  const preCleanup = await cleanupFixedPrice('pre-cleanup variant fixed price', priceListId, variantId);

  const cases = {
    addCurrencyMismatch: await captureCase('priceListFixedPricesAdd', 'add currency mismatch', addDocument, {
      priceListId,
      prices: [{ variantId, price: mismatchedPrice }],
    }),
    addCompareAtCurrencyMismatch: await captureCase(
      'priceListFixedPricesAdd',
      'add compare-at currency mismatch',
      addDocument,
      {
        priceListId,
        prices: [
          { variantId, price: matchingPrice, compareAtPrice: mismatchedCompareAtPrice },
          {
            variantId: secondVariantId,
            price: mismatchedPrice,
            compareAtPrice: mismatchedCompareAtPriceSecond,
          },
        ],
      },
    ),
    addVariantNotFound: await captureCase('priceListFixedPricesAdd', 'add variant not found', addDocument, {
      priceListId,
      prices: [{ variantId: missingVariantId, price: matchingPrice }],
    }),
    addMissingVariantCurrencyMismatchShortCircuit: await captureCase(
      'priceListFixedPricesAdd',
      'add missing variant currency mismatch short circuit',
      addShortCircuitDocument,
      {
        priceListId,
        prices: [{ variantId: missingVariantId, price: mismatchedPrice }],
      },
    ),
    addPriceListNotFound: await captureCase('priceListFixedPricesAdd', 'add price list not found', addDocument, {
      priceListId: missingPriceListId,
      prices: [{ variantId, price: matchingPrice }],
    }),
    addDuplicateVariantId: await captureCase('priceListFixedPricesAdd', 'add duplicate variant id', addDocument, {
      priceListId,
      prices: [
        { variantId, price: matchingPrice },
        { variantId, price: matchingPriceSecond },
      ],
    }),
  };

  const postAddDuplicateCleanup = await cleanupFixedPrice('post-add-duplicate cleanup', priceListId, variantId);

  const updateValidationCases = {
    updatePriceListNotFound: await captureCase(
      'priceListFixedPricesUpdate',
      'update price list not found',
      updateDocument,
      {
        priceListId: missingPriceListId,
        pricesToAdd: [{ variantId, price: updatedPrice }],
        variantIdsToDelete: [],
      },
    ),
    updateVariantNotFound: await captureCase('priceListFixedPricesUpdate', 'update variant not found', updateDocument, {
      priceListId,
      pricesToAdd: [{ variantId: missingVariantId, price: updatedPrice }],
      variantIdsToDelete: [],
    }),
    updatePriceNotFixed: await captureCase('priceListFixedPricesUpdate', 'update price not fixed', updateDocument, {
      priceListId,
      pricesToAdd: [{ variantId, price: updatedPrice }],
      variantIdsToDelete: [],
    }),
  };

  const seed = await seedFixedPrice('seed fixed price for update validation', priceListId, variantId, seedPrice);

  const updateSeededCases = {
    updateCurrencyMismatch: await captureCase(
      'priceListFixedPricesUpdate',
      'update currency mismatch',
      updateDocument,
      {
        priceListId,
        pricesToAdd: [{ variantId, price: mismatchedPrice }],
        variantIdsToDelete: [],
      },
    ),
    updateCompareAtCurrencyMismatch: await captureCase(
      'priceListFixedPricesUpdate',
      'update compare-at currency mismatch',
      updateDocument,
      {
        priceListId,
        pricesToAdd: [
          { variantId, price: updatedPrice, compareAtPrice: mismatchedCompareAtPrice },
          {
            variantId: secondVariantId,
            price: mismatchedPrice,
            compareAtPrice: mismatchedCompareAtPriceSecond,
          },
        ],
        variantIdsToDelete: [],
      },
    ),
    updateDuplicateVariantId: await captureCase(
      'priceListFixedPricesUpdate',
      'update duplicate variant id',
      updateDocument,
      {
        priceListId,
        pricesToAdd: [
          { variantId, price: updatedPrice },
          { variantId, price: updatedPriceSecond },
        ],
        variantIdsToDelete: [],
      },
    ),
  };

  const postUpdateCleanup = await cleanupFixedPrice('post-update cleanup', priceListId, variantId);

  const deleteCases = {
    deletePriceListNotFound: await captureCase(
      'priceListFixedPricesDelete',
      'delete price list not found',
      deleteDocument,
      {
        priceListId: missingPriceListId,
        variantIds: [variantId],
      },
    ),
    deleteVariantNotFound: await captureCase('priceListFixedPricesDelete', 'delete variant not found', deleteDocument, {
      priceListId,
      variantIds: [missingVariantId],
    }),
    deletePriceNotFixed: await captureCase('priceListFixedPricesDelete', 'delete price not fixed', deleteDocument, {
      priceListId,
      variantIds: [variantId],
    }),
  };

  finalCleanup = await cleanupFixedPrice('final cleanup variant fixed price', priceListId, variantId);

  const fixture = {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    scope: 'Variant-level price-list fixed-price validation parity',
    setup: {
      priceListId,
      priceListName,
      priceListCurrency,
      productId,
      productTitle,
      variantId,
      variantTitle,
      secondVariantId,
      secondVariantTitle,
      missingPriceListId,
      missingVariantId,
      mismatchCurrency,
      cleanup:
        'The capture deletes the target variant fixed price before recording, after duplicate add, after seeded update validation, and at final cleanup.',
    },
    cleanup: {
      preCleanup,
      postAddDuplicateCleanup,
      seed,
      postUpdateCleanup,
      finalCleanup,
    },
    cases: {
      ...cases,
      ...updateValidationCases,
      ...updateSeededCases,
      ...deleteCases,
    },
    upstreamCalls,
  };

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
  await mkdir(outputDir, { recursive: true });
  const outputPath = path.join(outputDir, 'price-list-fixed-prices-validation.json');
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        storeDomain,
        apiVersion,
        priceListId,
        variantId,
        cases: Object.keys(fixture.cases),
      },
      null,
      2,
    ),
  );
} catch (error) {
  if (!finalCleanup) {
    const cleanupAfterFailure = await cleanupFixedPrice('cleanup after failure', priceListId, variantId);
    console.error(JSON.stringify({ cleanupAfterFailure }, null, 2));
  }
  throw error;
}
