/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
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
  variables: {
    priceListId: string;
    quantityRules: QuantityRuleInput[];
  };
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'markets');
const outputPath = path.join(outputDir, 'quantity-rules-add-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const setupQuery = `#graphql
query QuantityRulesAddValidationSetup {
  priceLists(first: 5) {
    nodes { id name }
  }
  products(first: 10) {
    nodes {
      id
      title
      variants(first: 5) {
        nodes { id }
      }
    }
  }
}`;

const quantityRulesAddValidationMutation = `#graphql
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

function objectValue(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? value : {};
}

function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function firstStringField(value: unknown, field: string): string | null {
  const object = objectValue(value);
  const stringValue = object[field];
  return typeof stringValue === 'string' ? stringValue : null;
}

async function loadSetupIds(): Promise<{ priceListId: string; variantId: string }> {
  const response = await runGraphqlRequest(setupQuery, {});
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`setup query failed: ${JSON.stringify(response.payload)}`);
  }

  const data = objectValue(response.payload.data);
  const priceLists = arrayValue(objectValue(data.priceLists).nodes);
  const products = arrayValue(objectValue(data.products).nodes);
  const priceListId = firstStringField(priceLists[0], 'id');
  const variantId = products
    .flatMap((product) => arrayValue(objectValue(objectValue(product).variants).nodes))
    .map((variant) => firstStringField(variant, 'id'))
    .find((id): id is string => typeof id === 'string');

  if (!priceListId || !variantId) {
    throw new Error(`setup query did not find a price list and product variant: ${JSON.stringify(response.payload)}`);
  }

  return { priceListId, variantId };
}

async function captureCase(
  name: string,
  priceListId: string,
  quantityRules: QuantityRuleInput[],
): Promise<CapturedCase> {
  const variables = { priceListId, quantityRules };
  const response = await runGraphqlRequest(quantityRulesAddValidationMutation, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${name} failed: ${JSON.stringify(response.payload)}`);
  }

  const payload = objectValue(objectValue(response.payload.data).quantityRulesAdd);
  const userErrors = arrayValue(payload.userErrors);
  if (userErrors.length === 0) {
    throw new Error(`${name} unexpectedly returned no userErrors`);
  }

  return { name, variables, response };
}

function firstUserErrorCodes(capture: CapturedCase): string[] {
  const payload = objectValue(objectValue(capture.response.payload.data).quantityRulesAdd);
  return arrayValue(payload.userErrors)
    .map((error) => firstStringField(error, 'code'))
    .filter((code): code is string => typeof code === 'string');
}

const { priceListId, variantId } = await loadSetupIds();
const cases = [
  await captureCase('minimumTooLow', priceListId, [{ variantId, minimum: 0, maximum: 10, increment: 1 }]),
  await captureCase('incrementTooLow', priceListId, [{ variantId, minimum: 1, maximum: 10, increment: 0 }]),
  await captureCase('minimumGreaterThanMaximum', priceListId, [{ variantId, minimum: 10, maximum: 5, increment: 1 }]),
  await captureCase('minimumNotMultipleOfIncrement', priceListId, [
    { variantId, minimum: 5, maximum: 12, increment: 3 },
  ]),
  await captureCase('maximumNotMultipleOfIncrement', priceListId, [
    { variantId, minimum: 6, maximum: 10, increment: 3 },
  ]),
  await captureCase('duplicateVariantId', priceListId, [
    { variantId, minimum: 2, maximum: 10, increment: 2 },
    { variantId, minimum: 4, maximum: 12, increment: 2 },
  ]),
];

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
      },
      query: quantityRulesAddValidationMutation,
      cases,
      upstreamCalls: [],
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
      setup: {
        priceListId,
        variantId,
      },
      cases: cases.map((capture) => ({
        name: capture.name,
        status: capture.response.status,
        codes: firstUserErrorCodes(capture),
      })),
    },
    null,
    2,
  ),
);
