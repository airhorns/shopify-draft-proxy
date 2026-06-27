import 'dotenv/config';
/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-variants-bulk-update-allow-partial-and-error-order.json');

const allowPartialDocument = await readFile(
  path.join('config', 'parity-requests', 'products', 'productVariantsBulkUpdate-allow-partial.graphql'),
  'utf8',
);
const downstreamReadDocument = await readFile(
  path.join('config', 'parity-requests', 'products', 'productVariantsBulkUpdate-allow-partial-downstream-read.graphql'),
  'utf8',
);
const errorOrderDocument = await readFile(
  path.join('config', 'parity-requests', 'products', 'productVariantsBulkUpdate-error-order.graphql'),
  'utf8',
);
const productsHydrateNodesObservationQuery = await readFile(
  path.join('config', 'parity-requests', 'products', 'products-hydrate-nodes-observation.graphql'),
  'utf8',
);

const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createProductMutation = `#graphql
  mutation ProductVariantsBulkUpdatePartialCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteProductMutation = `#graphql
  mutation ProductVariantsBulkUpdatePartialDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const setupOptionsMutation = `#graphql
  mutation ProductVariantsBulkUpdatePartialSetupOptions($productId: ID!, $options: [OptionCreateInput!]!) {
    productOptionsCreate(productId: $productId, options: $options) {
      product {
        id
        options {
          id
          name
          values
          optionValues {
            id
            name
            hasVariants
          }
        }
        variants(first: 10) {
          nodes {
            id
            title
            sku
            price
            selectedOptions {
              name
              value
            }
          }
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

const setupSecondVariantMutation = `#graphql
  mutation ProductVariantsBulkUpdatePartialSetupSecondVariant(
    $productId: ID!
    $variants: [ProductVariantsBulkInput!]!
  ) {
    productVariantsBulkCreate(productId: $productId, variants: $variants) {
      product {
        id
        variants(first: 10) {
          nodes {
            id
            title
            sku
            price
            selectedOptions {
              name
              value
            }
          }
        }
      }
      productVariants {
        id
        title
        sku
        price
        selectedOptions {
          name
          value
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

function asRecord(value: unknown, label: string): JsonRecord {
  if (typeof value === 'object' && value !== null && !Array.isArray(value)) return value as JsonRecord;
  throw new Error(`${label} was not an object: ${JSON.stringify(value)}`);
}

function getPath(value: unknown, pathParts: readonly string[]): unknown {
  let cursor = value;
  for (const part of pathParts) {
    if (typeof cursor !== 'object' || cursor === null) return undefined;
    cursor = (cursor as JsonRecord)[part];
  }
  return cursor;
}

function stringAt(value: unknown, pathParts: readonly string[], label: string): string {
  const found = getPath(value, pathParts);
  if (typeof found === 'string' && found.length > 0) return found;
  throw new Error(`${label} missing string at ${pathParts.join('.')}: ${JSON.stringify(value, null, 2)}`);
}

function userErrorsAt(value: unknown, pathParts: readonly string[], label: string): unknown[] {
  const found = getPath(value, pathParts);
  if (Array.isArray(found)) return found;
  throw new Error(`${label} missing userErrors array at ${pathParts.join('.')}: ${JSON.stringify(value, null, 2)}`);
}

function assertNoUserErrors(value: unknown, pathParts: readonly string[], label: string): void {
  const userErrors = userErrorsAt(value, pathParts, label);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function productHydrateIdsForVariables(variables: JsonRecord): string[] {
  const productId = typeof variables['productId'] === 'string' ? variables['productId'] : null;
  if (!productId) return [];
  const variantIds = Array.isArray(variables['variants'])
    ? variables['variants'].flatMap((variant) => {
        if (typeof variant !== 'object' || variant === null) return [];
        const variantId = (variant as JsonRecord)['id'];
        return typeof variantId === 'string' ? [variantId] : [];
      })
    : [];
  return [productId, ...[...new Set(variantIds)].sort()];
}

async function captureProductHydrateCall(variables: JsonRecord): Promise<JsonRecord | null> {
  const ids = productHydrateIdsForVariables(variables);
  if (ids.length === 0) return null;
  const response = (await runGraphqlRaw(productsHydrateNodesObservationQuery, { ids })) as JsonRecord;
  return {
    operationName: 'ProductsHydrateNodes',
    variables: { ids },
    query: productsHydrateNodesObservationQuery,
    response: {
      status: response['status'],
      body: response['payload'],
    },
  };
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const skuPrefix = `PVBU-${runId.slice(-6)}`;
const createProductResponse = (await runGraphql(createProductMutation, {
  product: {
    title: `Bulk Variant Partial Update ${runId}`,
    status: 'DRAFT',
  },
})) as JsonRecord;
assertNoUserErrors(createProductResponse, ['data', 'productCreate', 'userErrors'], 'productCreate setup');
const productId = stringAt(createProductResponse, ['data', 'productCreate', 'product', 'id'], 'productCreate setup');

try {
  const setupOptionsResponse = (await runGraphql(setupOptionsMutation, {
    productId,
    options: [
      { name: 'Color', values: [{ name: 'Red' }, { name: 'Blue' }] },
      { name: 'Size', values: [{ name: 'Small' }, { name: 'Large' }] },
    ],
  })) as JsonRecord;
  assertNoUserErrors(
    setupOptionsResponse,
    ['data', 'productOptionsCreate', 'userErrors'],
    'productOptionsCreate setup',
  );
  const redVariantId = stringAt(
    setupOptionsResponse,
    ['data', 'productOptionsCreate', 'product', 'variants', 'nodes', '0', 'id'],
    'default variant setup',
  );

  const setupSecondVariantResponse = (await runGraphql(setupSecondVariantMutation, {
    productId,
    variants: [
      {
        optionValues: [
          { optionName: 'Color', name: 'Blue' },
          { optionName: 'Size', name: 'Large' },
        ],
        inventoryItem: { sku: `${skuPrefix}-BLUE` },
        price: '11.00',
      },
    ],
  })) as JsonRecord;
  assertNoUserErrors(
    setupSecondVariantResponse,
    ['data', 'productVariantsBulkCreate', 'userErrors'],
    'productVariantsBulkCreate setup',
  );
  const blueVariantId = stringAt(
    setupSecondVariantResponse,
    ['data', 'productVariantsBulkCreate', 'productVariants', '0', 'id'],
    'second variant setup',
  );

  const partialVariables: JsonRecord = {
    productId,
    variants: [
      {
        id: redVariantId,
        inventoryItem: { sku: `${skuPrefix}-RED-PARTIAL` },
        price: '9.99',
      },
      {
        id: blueVariantId,
        price: '-1.00',
      },
    ],
  };
  const partialHydrateCall = await captureProductHydrateCall(partialVariables);
  const partialResponse = (await runGraphqlRaw(allowPartialDocument, partialVariables)) as JsonRecord;

  const partialReadVariables: JsonRecord = { productId, redVariantId, blueVariantId };
  const partialReadResponse = (await runGraphqlRaw(downstreamReadDocument, partialReadVariables)) as JsonRecord;

  const errorOrderingVariables: JsonRecord = {
    productId,
    variants: [
      {
        id: redVariantId,
        price: '-1.00',
        compareAtPrice: '1000000000000000000',
      },
    ],
  };
  const errorOrderingResponse = (await runGraphqlRaw(errorOrderDocument, errorOrderingVariables)) as JsonRecord;

  const payload = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    setup: {
      createProductResponse,
      setupOptionsResponse,
      setupSecondVariantResponse,
      productId,
      redVariantId,
      blueVariantId,
    },
    notes:
      'Live productVariantsBulkUpdate capture for allowPartialUpdates true with one valid and one invalid existing variant, downstream read-after-write, and same-input userErrors field/code ordering.',
    upstreamCalls: partialHydrateCall ? [partialHydrateCall] : [],
    partialUpdate: {
      request: { variables: partialVariables },
      response: partialResponse,
    },
    partialRead: {
      request: { variables: partialReadVariables },
      response: partialReadResponse,
    },
    errorOrdering: {
      request: { variables: errorOrderingVariables },
      response: errorOrderingResponse,
    },
  };

  await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');

  const partialPayload = asRecord(
    asRecord(partialResponse['payload'], 'partial response payload')['data'],
    'partial data',
  )['productVariantsBulkUpdate'];
  const errorOrderingPayload = asRecord(
    asRecord(errorOrderingResponse['payload'], 'error ordering response payload')['data'],
    'error ordering data',
  )['productVariantsBulkUpdate'];
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        productId,
        redVariantId,
        blueVariantId,
        partialPayload,
        errorOrderingPayload,
      },
      null,
      2,
    ),
  );
} finally {
  await runGraphql(deleteProductMutation, { input: { id: productId } }).catch((error: unknown) => {
    console.error(`Cleanup failed for ${productId}: ${String(error)}`);
  });
}
