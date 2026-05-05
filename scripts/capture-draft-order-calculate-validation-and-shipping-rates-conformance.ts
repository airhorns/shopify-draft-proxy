import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'orders',
  'draftOrderCalculate-validation-and-shipping-rates.json',
);

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown, key: string): unknown[] {
  const fieldValue = asRecord(value)?.[key];
  return Array.isArray(fieldValue) ? fieldValue : [];
}

const variantLookupDocument = `#graphql
  query Har580ShippingVariant {
    products(first: 10) {
      nodes {
        variants(first: 10) {
          nodes {
            id
            inventoryItem {
              requiresShipping
            }
          }
        }
      }
    }
  }
`;

const variantLookupResponse = await runGraphql(variantLookupDocument, {});
const productNodes = readArray(asRecord(variantLookupResponse.data)?.['products'], 'nodes');
const variantNodes = productNodes.flatMap((productValue) => readArray(asRecord(productValue)?.['variants'], 'nodes'));
const shippingVariant = variantNodes
  .map(asRecord)
  .find((variant) => variant && asRecord(variant['inventoryItem'])?.['requiresShipping'] !== false);
const shippingVariantId = typeof shippingVariant?.['id'] === 'string' ? shippingVariant['id'] : null;

if (!shippingVariantId) {
  throw new Error('Unable to find a product variant with requiresShipping != false for HAR-580 capture.');
}

const variantHydrateDocument =
  'query OrdersDraftOrderVariantHydrate($id: ID!) {\n  productVariant(id: $id) { id title sku taxable price inventoryItem { requiresShipping } product { title } }\n}\n';
const variantHydrateVariables = { id: shippingVariantId };
const variantHydrateResponse = await runGraphql(variantHydrateDocument, variantHydrateVariables);

const document = `#graphql
  mutation DraftOrderCalculateValidationAndShippingRates(
    $emptyLineItems: DraftOrderInput!
    $invalidEmail: DraftOrderInput!
    $availableShippingRatesEmpty: DraftOrderInput!
    $paymentTermsTemplateId: DraftOrderInput!
  ) {
    emptyLineItems: draftOrderCalculate(input: $emptyLineItems) {
      calculatedDraftOrder { currencyCode }
      userErrors { field message }
    }
    invalidEmail: draftOrderCalculate(input: $invalidEmail) {
      calculatedDraftOrder { currencyCode }
      userErrors { field message }
    }
    availableShippingRatesEmpty: draftOrderCalculate(input: $availableShippingRatesEmpty) {
      calculatedDraftOrder {
        availableShippingRates {
          handle
          title
          price { amount currencyCode }
        }
      }
      userErrors { field message }
    }
    paymentTermsTemplateId: draftOrderCalculate(input: $paymentTermsTemplateId) {
      calculatedDraftOrder { currencyCode }
      userErrors { field message }
    }
  }
`;

const variables = {
  emptyLineItems: {
    lineItems: [],
  },
  invalidEmail: {
    email: 'bad email',
    lineItems: [
      {
        title: 'HAR-580 invalid email',
        quantity: 1,
        originalUnitPrice: '1.00',
      },
    ],
  },
  availableShippingRatesEmpty: {
    lineItems: [
      {
        variantId: shippingVariantId,
        quantity: 1,
      },
    ],
  },
  paymentTermsTemplateId: {
    paymentTerms: {
      paymentTermsTemplateId: 'gid://shopify/PaymentTermsTemplate/4',
      paymentSchedules: [{ issuedAt: '2026-01-01T00:00:00Z' }],
    },
    lineItems: [
      {
        title: 'HAR-580 payment terms',
        quantity: 1,
        originalUnitPrice: '1.00',
      },
    ],
  },
};

const response = await runGraphql(document, variables);

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  document,
  variables,
  setup: {
    variantLookup: {
      document: variantLookupDocument,
      variables: {},
      response: variantLookupResponse,
      selectedVariantId: shippingVariantId,
    },
    variantHydrate: {
      document: variantHydrateDocument,
      variables: variantHydrateVariables,
      response: variantHydrateResponse,
    },
  },
  mutation: {
    response,
  },
  upstreamCalls: [
    {
      operationName: 'OrdersDraftOrderVariantHydrate',
      variables: variantHydrateVariables,
      query: variantHydrateDocument,
      response: {
        status: 200,
        body: {
          data: variantHydrateResponse.data,
        },
      },
    },
  ],
};

const absoluteFixturePath = path.join(repoRoot, fixturePath);
await mkdir(path.dirname(absoluteFixturePath), { recursive: true });
await writeFile(absoluteFixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

process.stdout.write(`${JSON.stringify({ fixturePath, response }, null, 2)}\n`);
