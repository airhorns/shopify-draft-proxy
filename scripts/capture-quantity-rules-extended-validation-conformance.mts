/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type QuantityRuleInput = {
  variantId: string;
  minimum: number;
  maximum?: number;
  increment: number;
};

type CapturedCase = {
  name: string;
  variables: Record<string, unknown>;
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
const missingVariantId = 'gid://shopify/ProductVariant/999999999999999';

const setupQuery = `#graphql
query QuantityRulesExtendedValidationSetup($priceListId: ID!, $productId: ID!) {
  priceList(id: $priceListId) {
    id
    name
    currency
  }
  product(id: $productId) {
    id
    title
    variants(first: 10) {
      nodes { id title sku }
    }
  }
}`;

const preflightQuery = `#graphql
query MarketsMutationPreflightHydrate($priceListId: ID!) {
  priceList(id: $priceListId) {
    __typename
    id
    name
    currency
    fixedPricesCount
    quantityRules(first: 20) {
      edges {
        cursor
        node {
          minimum
          maximum
          increment
          isDefault
          originType
          productVariant { id }
        }
      }
    }
    prices(first: 20) {
      edges {
        cursor
        node {
          price { amount currencyCode }
          compareAtPrice { amount currencyCode }
          originType
          variant { id sku product { id title } }
          quantityPriceBreaks(first: 20) {
            edges {
              cursor
              node {
                minimumQuantity
                price { amount currencyCode }
              }
            }
          }
        }
      }
    }
  }
  products(first: 10) {
    nodes {
      id
      title
      variants(first: 20) {
        nodes { id title sku }
      }
    }
  }
}`;

const quantityRulesAddMutation = `#graphql
mutation QuantityRulesAddValidation($priceListId: ID!, $quantityRules: [QuantityRuleInput!]!) {
  quantityRulesAdd(priceListId: $priceListId, quantityRules: $quantityRules) {
    quantityRules {
      minimum
      maximum
      increment
      productVariant { id }
    }
    userErrors { __typename field message code }
  }
}`;

const quantityRulesDeleteMutation = `#graphql
mutation QuantityRulesDelete($priceListId: ID!, $variantIds: [ID!]!) {
  quantityRulesDelete(priceListId: $priceListId, variantIds: $variantIds) {
    deletedQuantityRulesVariantIds
    userErrors { field message code }
  }
}`;

const quantityPricingByVariantUpdateMutation = `#graphql
mutation QuantityPricingByVariantUpdate($priceListId: ID!, $input: QuantityPricingByVariantUpdateInput!) {
  quantityPricingByVariantUpdate(priceListId: $priceListId, input: $input) {
    productVariants { id }
    userErrors { __typename field message code }
  }
}`;

function objectValue(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? value : {};
}

function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function firstStringField(value: unknown, field: string): string | null {
  const stringValue = objectValue(value)[field];
  return typeof stringValue === 'string' ? stringValue : null;
}

function assertGraphqlOk(label: string, result: ConformanceGraphqlResult): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertUserErrors(label: string, payload: unknown): void {
  const userErrors = arrayValue(objectValue(payload).userErrors);
  if (userErrors.length === 0) {
    throw new Error(`${label} unexpectedly returned no userErrors: ${JSON.stringify(payload, null, 2)}`);
  }
}

async function captureCase(
  name: string,
  document: string,
  variables: Record<string, unknown>,
  payloadRoot: string,
): Promise<CapturedCase> {
  const response = await runGraphqlRequest(document, variables);
  assertGraphqlOk(name, response);
  assertUserErrors(name, objectValue(objectValue(response.payload.data)[payloadRoot]));
  return { name, variables, response };
}

async function cleanupQuantityPricing(label: string): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlRequest(quantityPricingByVariantUpdateMutation, {
    priceListId,
    input: {
      pricesToAdd: [],
      pricesToDeleteByVariantId: [variantId],
      quantityRulesToAdd: [],
      quantityRulesToDeleteByVariantId: [variantId],
      quantityPriceBreaksToAdd: [],
      quantityPriceBreaksToDelete: [],
      quantityPriceBreaksToDeleteByVariantId: [variantId],
    },
  });
  assertGraphqlOk(label, response);
  return response;
}

async function capturePreflight(variables: Record<string, unknown>): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlRequest(preflightQuery, variables);
  assertGraphqlOk('preflight hydrate', response);
  return response;
}

function upstreamCall(variables: Record<string, unknown>, response: ConformanceGraphqlResult) {
  return {
    operationName: 'MarketsMutationPreflightHydrate',
    variables,
    query: 'captured preflight used by quantityRulesAdd/quantityRulesDelete local validation',
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

const setup = await runGraphqlRequest(setupQuery, { priceListId, productId });
assertGraphqlOk('setup query', setup);

const priceList = objectValue(objectValue(setup.payload.data).priceList);
const product = objectValue(objectValue(setup.payload.data).product);
const productVariants = arrayValue(objectValue(product.variants).nodes);
const setupVariantIds = productVariants.map((variant) => firstStringField(variant, 'id'));
if (firstStringField(priceList, 'id') !== priceListId || firstStringField(product, 'id') !== productId) {
  throw new Error(`Configured price list or product was not found: ${JSON.stringify(setup.payload, null, 2)}`);
}
if (!setupVariantIds.includes(variantId)) {
  throw new Error(`Configured variant ${variantId} was not found on product ${productId}.`);
}

const currency = firstStringField(priceList, 'currency') ?? 'USD';
const deleteNoExistingVariables = { priceListId, variantIds: [variantId] };
const missingVariantVariables = {
  priceListId,
  quantityRules: [{ variantId: missingVariantId, minimum: 1, maximum: 5, increment: 1 }],
};
const maxBelowBreakRule: QuantityRuleInput = {
  variantId,
  minimum: 1,
  maximum: 5,
  increment: 1,
};
const maxBelowBreakVariables = {
  priceListId,
  quantityRules: [maxBelowBreakRule],
};
const seedInput = {
  pricesToAdd: [
    {
      variantId,
      price: { amount: '20.00', currencyCode: currency },
    },
  ],
  pricesToDeleteByVariantId: [],
  quantityRulesToAdd: [
    {
      variantId,
      minimum: 1,
      maximum: 20,
      increment: 1,
    },
  ],
  quantityRulesToDeleteByVariantId: [],
  quantityPriceBreaksToAdd: [
    {
      variantId,
      minimumQuantity: 10,
      price: { amount: '18.00', currencyCode: currency },
    },
  ],
  quantityPriceBreaksToDelete: [],
  quantityPriceBreaksToDeleteByVariantId: [],
};

const preCleanup = await cleanupQuantityPricing('pre-cleanup quantity pricing');
const deleteNoExistingPreflight = await capturePreflight(deleteNoExistingVariables);
const missingVariantPreflight = await capturePreflight(missingVariantVariables);
const deleteNoExisting = await captureCase(
  'quantityRulesDelete no existing rule',
  quantityRulesDeleteMutation,
  deleteNoExistingVariables,
  'quantityRulesDelete',
);
const missingVariant = await captureCase(
  'quantityRulesAdd missing variant',
  quantityRulesAddMutation,
  missingVariantVariables,
  'quantityRulesAdd',
);

const seed = await runGraphqlRequest(quantityPricingByVariantUpdateMutation, {
  priceListId,
  input: seedInput,
});
assertGraphqlOk('seed quantity price break', seed);
const seedPayload = objectValue(objectValue(seed.payload.data).quantityPricingByVariantUpdate);
if (arrayValue(seedPayload.userErrors).length > 0) {
  throw new Error(`seed quantity price break returned userErrors: ${JSON.stringify(seed.payload, null, 2)}`);
}

let maxBelowBreakPreflight: ConformanceGraphqlResult | null = null;
let maxBelowBreak: CapturedCase | null = null;
let finalCleanup: ConformanceGraphqlResult | null = null;
try {
  maxBelowBreakPreflight = await capturePreflight(maxBelowBreakVariables);
  maxBelowBreak = await captureCase(
    'quantityRulesAdd maximum lower than existing price break',
    quantityRulesAddMutation,
    maxBelowBreakVariables,
    'quantityRulesAdd',
  );
} finally {
  finalCleanup = await cleanupQuantityPricing('final cleanup quantity pricing');
}

if (!maxBelowBreakPreflight || !maxBelowBreak || !finalCleanup) {
  throw new Error('maximum-below-break capture did not complete.');
}

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b');
const outputPath = path.join(outputDir, 'quantity-rules-extended-validation.json');
await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      setup: {
        priceListId,
        productId,
        variantId,
        missingVariantId,
        currency,
        seedInput,
        seed,
      },
      cleanup: {
        preCleanup,
        finalCleanup,
      },
      cases: [deleteNoExisting, missingVariant, maxBelowBreak],
      upstreamCalls: [
        upstreamCall(deleteNoExistingVariables, deleteNoExistingPreflight),
        upstreamCall(missingVariantVariables, missingVariantPreflight),
        upstreamCall(maxBelowBreakVariables, maxBelowBreakPreflight),
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain,
      apiVersion,
      cases: [deleteNoExisting, missingVariant, maxBelowBreak].map((capture) => ({
        name: capture.name,
        status: capture.response.status,
      })),
    },
    null,
    2,
  ),
);
