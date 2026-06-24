/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'draft-order-variant-custom-only-fields.json');
const requestDir = path.join('config', 'parity-requests', 'orders');

function readRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`Missing required capture value: ${label}`);
  }
  return value;
}

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(requestDir, name), 'utf8');
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

async function run(query: string, variables: JsonRecord, label: string): Promise<JsonRecord> {
  const result: ConformanceGraphqlResult<JsonRecord> = await runGraphqlRequest<JsonRecord>(query, variables);
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return result.payload as JsonRecord;
}

async function runRaw(query: string, variables: JsonRecord): Promise<ConformanceGraphqlResult<JsonRecord>> {
  return runGraphqlRequest<JsonRecord>(query, variables);
}

function mutationPayload(payload: JsonRecord, rootName: string, label: string): JsonRecord {
  const root = readRecord(readRecord(payload['data'])?.[rootName]);
  if (!root) {
    throw new Error(`${label} missing ${rootName}: ${JSON.stringify(payload, null, 2)}`);
  }
  const userErrors = readArray(root['userErrors']);
  if (userErrors.length > 0) {
    throw new Error(`${label} ${rootName} userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
  return root;
}

const productCreateMutation = `#graphql
  mutation DraftOrderVariantCustomFieldsProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            title
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productVariantUpdateMutation = `#graphql
  mutation DraftOrderVariantCustomFieldsVariantUpdate($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkUpdate(productId: $productId, variants: $variants) {
      productVariants {
        id
        title
        sku
        taxable
        price
        inventoryItem {
          requiresShipping
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation DraftOrderVariantCustomFieldsProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderDeleteMutation = `#graphql
  mutation DraftOrderVariantCustomFieldsDraftDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const variantHydrateDocument =
  'query OrdersDraftOrderVariantHydrate($id: ID!) {\n  productVariant(id: $id) { id title sku taxable price inventoryItem { requiresShipping } product { title } }\n}\n';

const createDocument = await readRequest('draft-order-variant-custom-only-create.graphql');
const calculateDocument = await readRequest('draft-order-variant-custom-only-calculate.graphql');

const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/gu, '')
  .slice(0, 14);
const productCreateVariables = {
  product: {
    title: `Draft order catalog source ${stamp}`,
    status: 'DRAFT',
  },
};

const productCreatePayload = await run(productCreateMutation, productCreateVariables, 'productCreate');
const product = readRecord(mutationPayload(productCreatePayload, 'productCreate', 'productCreate')['product']);
const productId = requireString(product?.['id'], 'product id');
const defaultVariant = readRecord(readArray(readRecord(product?.['variants'])?.['nodes'])[0]);
const defaultVariantId = requireString(defaultVariant?.['id'], 'default variant id');

const sku = `DRAFT-CATALOG-${stamp}`;
const productVariantUpdateVariables = {
  productId,
  variants: [
    {
      id: defaultVariantId,
      price: '19.95',
      taxable: true,
      inventoryItem: {
        sku,
        requiresShipping: true,
      },
    },
  ],
};
const productVariantUpdatePayload = await run(
  productVariantUpdateMutation,
  productVariantUpdateVariables,
  'productVariantsBulkUpdate',
);
mutationPayload(productVariantUpdatePayload, 'productVariantsBulkUpdate', 'productVariantsBulkUpdate');

const variantHydrateVariables = { id: defaultVariantId };
const variantHydratePayload = await run(variantHydrateDocument, variantHydrateVariables, 'variant hydrate');

const lineItemInput = {
  variantId: defaultVariantId,
  title: 'Bogus custom title',
  sku: 'BOGUS-CUSTOM-SKU',
  quantity: 2,
  originalUnitPrice: '0.01',
  taxable: false,
  requiresShipping: false,
};

const createVariables = {
  input: {
    email: `draft-order-variant-custom-fields-${stamp}@example.com`,
    lineItems: [lineItemInput],
  },
};
const draftOrderCreatePayload = await run(createDocument, createVariables, 'draftOrderCreate');
const draftOrderCreateRoot = mutationPayload(draftOrderCreatePayload, 'draftOrderCreate', 'draftOrderCreate');
const draftOrderId = requireString(readRecord(draftOrderCreateRoot['draftOrder'])?.['id'], 'draft order id');

const calculateVariables = {
  input: {
    lineItems: [lineItemInput],
  },
};
const draftOrderCalculatePayload = await run(calculateDocument, calculateVariables, 'draftOrderCalculate');
mutationPayload(draftOrderCalculatePayload, 'draftOrderCalculate', 'draftOrderCalculate');

const draftCleanup = await runRaw(draftOrderDeleteMutation, { input: { id: draftOrderId } });
const productCleanup = await runRaw(productDeleteMutation, { input: { id: productId } });

await writeJson(fixturePath, {
  scenarioId: 'draft-order-variant-custom-only-fields',
  apiVersion,
  storeDomain,
  recordedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  setup: {
    productCreate: {
      query: productCreateMutation,
      variables: productCreateVariables,
      response: productCreatePayload,
    },
    productVariantsBulkUpdate: {
      query: productVariantUpdateMutation,
      variables: productVariantUpdateVariables,
      response: productVariantUpdatePayload,
    },
    variantHydrate: {
      query: variantHydrateDocument,
      variables: variantHydrateVariables,
      response: variantHydratePayload,
    },
  },
  draftOrderCreate: {
    query: createDocument,
    variables: createVariables,
    response: draftOrderCreatePayload,
  },
  draftOrderCalculate: {
    query: calculateDocument,
    variables: calculateVariables,
    response: draftOrderCalculatePayload,
  },
  upstreamCalls: [
    {
      operationName: 'OrdersDraftOrderVariantHydrate',
      variables: variantHydrateVariables,
      query: variantHydrateDocument,
      response: {
        status: 200,
        body: variantHydratePayload,
      },
    },
  ],
  cleanup: {
    draftOrderDelete: {
      query: draftOrderDeleteMutation,
      variables: { input: { id: draftOrderId } },
      response: draftCleanup.payload,
      status: draftCleanup.status,
    },
    productDelete: {
      query: productDeleteMutation,
      variables: { input: { id: productId } },
      response: productCleanup.payload,
      status: productCleanup.status,
    },
  },
});

console.log(
  JSON.stringify(
    {
      fixturePath,
      productId,
      variantId: defaultVariantId,
      draftOrderId,
      draftCleanupStatus: draftCleanup.status,
      productCleanupStatus: productCleanup.status,
    },
    null,
    2,
  ),
);
