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

const priceListId = process.env['SHOPIFY_CONFORMANCE_PRICE_LIST_ID'] ?? 'gid://shopify/PriceList/31575376178';
const missingVariantIds = ['gid://shopify/ProductVariant/9999991936001', 'gid://shopify/ProductVariant/9999991936002'];

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'quantity-rules-delete-validation.json');

const setupQuery = `query QuantityRulesDeleteValidationSetup($priceListId: ID!) {
  priceList(id: $priceListId) {
    id
    name
    currency
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

const quantityRulesDeleteMutation = `mutation QuantityRulesDelete($priceListId: ID!, $variantIds: [ID!]!) {
  quantityRulesDelete(priceListId: $priceListId, variantIds: $variantIds) {
    deletedQuantityRulesVariantIds
    userErrors { field message code }
  }
}`;

function compactGraphql(document: string): string {
  return document.replace(/\s+/gu, ' ').trim();
}

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

function userErrors(result: ConformanceGraphqlResult): JsonRecord[] {
  return arrayValue(objectValue(objectValue(result.payload.data).quantityRulesDelete).userErrors).filter(
    (error): error is JsonRecord => typeof error === 'object' && error !== null && !Array.isArray(error),
  );
}

async function captureCase(name: string, variables: JsonRecord): Promise<CapturedCase> {
  const response = await runGraphqlRequest(quantityRulesDeleteMutation, variables);
  assertGraphqlOk(name, response);
  const errors = userErrors(response);
  const laterIndexError = errors.find(
    (error) =>
      firstStringField(error, 'code') === 'PRODUCT_VARIANT_DOES_NOT_EXIST' &&
      JSON.stringify(error['field']) === JSON.stringify(['variantIds', '1']),
  );
  if (!laterIndexError) {
    throw new Error(
      `${name} did not report PRODUCT_VARIANT_DOES_NOT_EXIST at variantIds[1]: ${JSON.stringify(response.payload)}`,
    );
  }
  return { name, variables, response };
}

function upstreamCall(variables: JsonRecord, response: ConformanceGraphqlResult) {
  return {
    operationName: 'MarketsMutationPreflightHydrate',
    variables,
    query: compactGraphql(preflightQuery),
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

const setup = await runGraphqlRequest(setupQuery, { priceListId });
assertGraphqlOk('setup query', setup);
const priceList = objectValue(objectValue(setup.payload.data).priceList);
if (firstStringField(priceList, 'id') !== priceListId) {
  throw new Error(`Configured price list was not found: ${JSON.stringify(setup.payload, null, 2)}`);
}

const missingVariantsVariables = { priceListId, variantIds: missingVariantIds };
const missingVariantsPreflight = await runGraphqlRequest(preflightQuery, missingVariantsVariables);
assertGraphqlOk('missing variants preflight', missingVariantsPreflight);
const missingVariants = await captureCase('quantityRulesDelete missing variants indexes', missingVariantsVariables);

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
        priceList,
        missingVariantIds,
      },
      cases: {
        missingVariants,
      },
      upstreamCalls: [upstreamCall(missingVariantsVariables, missingVariantsPreflight)],
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
      cases: {
        missingVariants: {
          status: missingVariants.response.status,
          userErrors: userErrors(missingVariants.response).map((error) => ({
            field: error['field'],
            code: error['code'],
          })),
        },
      },
    },
    null,
    2,
  ),
);
