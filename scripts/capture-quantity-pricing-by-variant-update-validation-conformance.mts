/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type CapturedCase = {
  name: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  env: { ...process.env, SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'quantity-pricing-by-variant-update-validation.json');
const missingPriceListId = 'gid://shopify/PriceList/0';
const missingVariantId = 'gid://shopify/ProductVariant/999999999999999';
const missingQuantityPriceBreakId = 'gid://shopify/QuantityPriceBreak/999999999999999';

const setupQuery = `#graphql
query QuantityPricingByVariantUpdateValidationSetup {
  priceLists(first: 5) {
    nodes { id name currency }
  }
  products(first: 10) {
    nodes {
      id
      title
      variants(first: 5) {
        nodes { id title sku }
      }
    }
  }
}`;

const preflightQuery = `query MarketsMutationPreflightHydrate($priceListId: ID!) {
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
    prices(first: 20, originType: FIXED) {
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
                id
                minimumQuantity
                price { amount currencyCode }
                variant { id }
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

const quantityPricingByVariantUpdateMutation = `#graphql
mutation QuantityPricingByVariantUpdate($priceListId: ID!, $input: QuantityPricingByVariantUpdateInput!) {
  quantityPricingByVariantUpdate(priceListId: $priceListId, input: $input) {
    productVariants { id }
    userErrors { __typename field message code }
  }
}`;

function objectValue(value: unknown): JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : {};
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

function rootPayload(result: ConformanceGraphqlResult): JsonRecord {
  const root = objectValue(objectValue(result.payload.data).quantityPricingByVariantUpdate);
  if (Object.keys(root).length === 0) {
    throw new Error(`Expected quantityPricingByVariantUpdate payload: ${JSON.stringify(result.payload)}`);
  }
  return root;
}

function userErrors(result: ConformanceGraphqlResult): JsonRecord[] {
  return arrayValue(rootPayload(result).userErrors).filter(
    (error): error is JsonRecord => typeof error === 'object' && error !== null && !Array.isArray(error),
  );
}

function assertHasUserErrors(label: string, result: ConformanceGraphqlResult): void {
  const errors = userErrors(result);
  if (errors.length === 0) {
    throw new Error(`${label} unexpectedly returned no userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertNoUserErrors(label: string, result: ConformanceGraphqlResult): void {
  const errors = userErrors(result);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function emptyInput(): JsonRecord {
  return {
    pricesToAdd: [],
    pricesToDeleteByVariantId: [],
    quantityRulesToAdd: [],
    quantityRulesToDeleteByVariantId: [],
    quantityPriceBreaksToAdd: [],
    quantityPriceBreaksToDelete: [],
    quantityPriceBreaksToDeleteByVariantId: [],
  };
}

function inputWith(partial: JsonRecord): JsonRecord {
  return { ...emptyInput(), ...partial };
}

async function capturePreflight(variables: JsonRecord): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlRequest(preflightQuery, variables);
  assertGraphqlOk('preflight hydrate', response);
  return response;
}

async function captureValidationCase(name: string, variables: JsonRecord): Promise<CapturedCase> {
  const response = await runGraphqlRequest(quantityPricingByVariantUpdateMutation, variables);
  assertGraphqlOk(name, response);
  assertHasUserErrors(name, response);
  return { name, variables, response };
}

async function runUpdate(label: string, variables: JsonRecord): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlRequest(quantityPricingByVariantUpdateMutation, variables);
  assertGraphqlOk(label, response);
  return response;
}

function upstreamCall(variables: JsonRecord, response: ConformanceGraphqlResult) {
  return {
    operationName: 'MarketsMutationPreflightHydrate',
    variables,
    query: preflightQuery.replace(/\s+/gu, ' ').trim(),
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

function caseCodes(capture: CapturedCase): string[] {
  return userErrors(capture.response)
    .map((error) => firstStringField(error, 'code'))
    .filter((code): code is string => typeof code === 'string');
}

const setup = await runGraphqlRequest(setupQuery, {});
assertGraphqlOk('setup query', setup);
const setupData = objectValue(setup.payload.data);
const priceList = arrayValue(objectValue(setupData.priceLists).nodes)[0];
const product = arrayValue(objectValue(setupData.products).nodes).find(
  (candidate) => arrayValue(objectValue(objectValue(candidate).variants).nodes).length > 0,
);
const variant = arrayValue(objectValue(objectValue(product).variants).nodes)[0];
const priceListId = firstStringField(priceList, 'id');
const variantId = firstStringField(variant, 'id');
const currency = firstStringField(priceList, 'currency');
if (!priceListId || !variantId || !currency) {
  throw new Error(`Setup did not find a price list, currency, and variant: ${JSON.stringify(setup.payload)}`);
}
const mismatchCurrency = currency === 'USD' ? 'CAD' : 'USD';

const preCleanupVariables = {
  priceListId,
  input: inputWith({
    pricesToDeleteByVariantId: [variantId],
    quantityRulesToDeleteByVariantId: [variantId],
    quantityPriceBreaksToDeleteByVariantId: [variantId],
  }),
};
const preCleanup = await runUpdate('pre-cleanup quantity pricing', preCleanupVariables);
assertNoUserErrors('pre-cleanup quantity pricing', preCleanup);

const validationInputs = {
  unknownPriceList: {
    priceListId: missingPriceListId,
    input: inputWith({
      pricesToAdd: [{ variantId, price: { amount: '12.00', currencyCode: currency } }],
    }),
  },
  currencyMismatch: {
    priceListId,
    input: inputWith({
      pricesToAdd: [{ variantId, price: { amount: '12.00', currencyCode: mismatchCurrency } }],
    }),
  },
  duplicatePricesToAdd: {
    priceListId,
    input: inputWith({
      pricesToAdd: [
        { variantId, price: { amount: '12.00', currencyCode: currency } },
        { variantId, price: { amount: '13.00', currencyCode: currency } },
      ],
    }),
  },
  pricesToDeleteMissingVariant: {
    priceListId,
    input: inputWith({ pricesToDeleteByVariantId: [missingVariantId] }),
  },
  quantityRulesToDeleteMissingVariant: {
    priceListId,
    input: inputWith({ quantityRulesToDeleteByVariantId: [missingVariantId] }),
  },
  quantityPriceBreaksToDeleteByVariantMissingVariant: {
    priceListId,
    input: inputWith({ quantityPriceBreaksToDeleteByVariantId: [missingVariantId] }),
  },
  quantityPriceBreaksToDeleteMissingId: {
    priceListId,
    input: inputWith({ quantityPriceBreaksToDelete: [missingQuantityPriceBreakId] }),
  },
  quantityRuleMinimumTooLow: {
    priceListId,
    input: inputWith({
      quantityRulesToAdd: [{ variantId, minimum: 0, maximum: 10, increment: 1 }],
    }),
  },
  quantityRuleIncrementTooLow: {
    priceListId,
    input: inputWith({
      quantityRulesToAdd: [{ variantId, minimum: 1, maximum: 10, increment: 0 }],
    }),
  },
  quantityRuleMinimumGreaterThanMaximum: {
    priceListId,
    input: inputWith({
      quantityRulesToAdd: [{ variantId, minimum: 10, maximum: 5, increment: 1 }],
    }),
  },
  quantityRuleMinimumNotMultiple: {
    priceListId,
    input: inputWith({
      quantityRulesToAdd: [{ variantId, minimum: 5, maximum: 12, increment: 3 }],
    }),
  },
  quantityRuleMaximumNotMultiple: {
    priceListId,
    input: inputWith({
      quantityRulesToAdd: [{ variantId, minimum: 6, maximum: 10, increment: 3 }],
    }),
  },
};

const preflightByCase: Record<string, ConformanceGraphqlResult> = {};
for (const [name, variables] of Object.entries(validationInputs)) {
  preflightByCase[name] = await capturePreflight(variables);
}

const cases: Record<string, CapturedCase> = {};
for (const [name, variables] of Object.entries(validationInputs)) {
  cases[name] = await captureValidationCase(name, variables);
}

const acceptedNoopInputs = {
  duplicatePricesToDeleteAccepted: {
    priceListId,
    input: inputWith({ pricesToDeleteByVariantId: [variantId, variantId] }),
  },
  quantityRulesToDeleteWithoutRuleAccepted: {
    priceListId,
    input: inputWith({ quantityRulesToDeleteByVariantId: [variantId] }),
  },
};
const acceptedNoopPreflights: Record<string, ConformanceGraphqlResult> = {};
const acceptedNoopCases: Record<string, CapturedCase> = {};
for (const [name, variables] of Object.entries(acceptedNoopInputs)) {
  acceptedNoopPreflights[name] = await capturePreflight(variables);
  const response = await runUpdate(name, variables);
  assertNoUserErrors(name, response);
  acceptedNoopCases[name] = { name, variables, response };
}

const seedVariables = {
  priceListId,
  input: inputWith({
    pricesToAdd: [{ variantId, price: { amount: '22.00', currencyCode: currency } }],
    quantityRulesToAdd: [{ variantId, minimum: 1, maximum: 20, increment: 1 }],
    quantityPriceBreaksToAdd: [
      {
        variantId,
        minimumQuantity: 5,
        price: { amount: '20.00', currencyCode: currency },
      },
    ],
  }),
};
const seed = await runUpdate('seed quantity pricing', seedVariables);
assertNoUserErrors('seed quantity pricing', seed);

const quantityPriceBreaksToDeleteByVariantExistingVariables = {
  priceListId,
  input: inputWith({ quantityPriceBreaksToDeleteByVariantId: [variantId] }),
};
const quantityPriceBreaksToDeleteByVariantExistingPreflight = await capturePreflight(
  quantityPriceBreaksToDeleteByVariantExistingVariables,
);
const quantityPriceBreaksToDeleteByVariantExisting = await runUpdate(
  'quantity price breaks delete by existing variant',
  quantityPriceBreaksToDeleteByVariantExistingVariables,
);
assertNoUserErrors('quantity price breaks delete by existing variant', quantityPriceBreaksToDeleteByVariantExisting);

const finalCleanupVariables = {
  priceListId,
  input: inputWith({
    pricesToDeleteByVariantId: [variantId],
    quantityRulesToDeleteByVariantId: [variantId],
    quantityPriceBreaksToDeleteByVariantId: [variantId],
  }),
};
const finalCleanup = await runUpdate('final cleanup quantity pricing', finalCleanupVariables);
assertNoUserErrors('final cleanup quantity pricing', finalCleanup);

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
        variantId,
        currency,
        mismatchCurrency,
        missingPriceListId,
        missingVariantId,
        missingQuantityPriceBreakId,
      },
      query: quantityPricingByVariantUpdateMutation,
      cases,
      acceptedNoopCases,
      cleanup: {
        preCleanup,
        seed,
        quantityPriceBreaksToDeleteByVariantExisting,
        finalCleanup,
      },
      upstreamCalls: [
        ...Object.entries(validationInputs).map(([name, variables]) =>
          upstreamCall(variables, preflightByCase[name] as ConformanceGraphqlResult),
        ),
        ...Object.entries(acceptedNoopInputs).map(([name, variables]) =>
          upstreamCall(variables, acceptedNoopPreflights[name] as ConformanceGraphqlResult),
        ),
        upstreamCall(
          quantityPriceBreaksToDeleteByVariantExistingVariables,
          quantityPriceBreaksToDeleteByVariantExistingPreflight,
        ),
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
      setup: { priceListId, variantId, currency, mismatchCurrency },
      cases: Object.values(cases).map((capture) => ({
        name: capture.name,
        status: capture.response.status,
        codes: caseCodes(capture),
      })),
      acceptedNoopCases: Object.values(acceptedNoopCases).map((capture) => ({
        name: capture.name,
        status: capture.response.status,
      })),
    },
    null,
    2,
  ),
);
