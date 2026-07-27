// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { captureProductMutationPreflight } from './product-mutation-preflight-capture.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-variants-bulk-reorder-validation-resequence.json');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createProductMutation = `#graphql
  mutation ProductVariantsBulkReorderCreateProduct($product: ProductCreateInput!) {
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

const setupOptionsMutation = `#graphql
  mutation ProductVariantsBulkReorderSetupOptions($productId: ID!, $options: [OptionCreateInput!]!) {
    productOptionsCreate(productId: $productId, options: $options) {
      product {
        id
        options {
          id
          name
          position
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
            position
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

const bulkCreateMutation = `#graphql
  mutation ProductVariantsBulkReorderCreateVariants($productId: ID!, $variants: [ProductVariantsBulkInput!]!) {
    productVariantsBulkCreate(productId: $productId, variants: $variants) {
      product {
        id
        variants(first: 10) {
          nodes {
            id
            title
            position
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
        position
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

const deleteProductMutation = `#graphql
  mutation ProductVariantsBulkReorderCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const reorderMutation = await readFile(
  'config/parity-requests/products/productVariantsBulkReorder-validation-resequence.graphql',
  'utf8',
);
const downstreamReadQuery = await readFile(
  'config/parity-requests/products/productVariantsBulkReorder-position-read.graphql',
  'utf8',
);
const legacyReorderMutation = await readFile(
  'config/parity-requests/products/productVariantsBulkReorder-parity.graphql',
  'utf8',
);
const legacyDownstreamReadQuery = await readFile(
  'config/parity-requests/products/productVariantsBulkReorder-downstream-read.graphql',
  'utf8',
);

function expectNoUserErrors(label, userErrors) {
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function variantByTitle(variants, title) {
  const variant = variants.find((entry) => entry?.title === title);
  if (!variant?.id) {
    throw new Error(`Expected ${title} variant in ${JSON.stringify(variants, null, 2)}`);
  }
  return variant;
}

async function captureCase(name, query, variables) {
  return {
    name,
    request: {
      variables,
    },
    response: await runGraphqlRaw(query, variables),
  };
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const createProductResponse = await runGraphql(createProductMutation, {
  product: {
    title: `Hermes Variant Reorder Branches ${runId}`,
    status: 'DRAFT',
  },
});
expectNoUserErrors('productCreate (variant reorder seed)', createProductResponse.data?.productCreate?.userErrors);
const productId = createProductResponse.data?.productCreate?.product?.id ?? null;
if (!productId) {
  throw new Error(`Could not create product variant reorder seed: ${JSON.stringify(createProductResponse, null, 2)}`);
}

try {
  const setupOptionsResponse = await runGraphql(setupOptionsMutation, {
    productId,
    options: [{ name: 'Color', values: [{ name: 'Red' }, { name: 'Blue' }, { name: 'Green' }] }],
  });
  expectNoUserErrors(
    'productOptionsCreate (variant reorder seed)',
    setupOptionsResponse.data?.productOptionsCreate?.userErrors,
  );

  const bulkCreateResponse = await runGraphql(bulkCreateMutation, {
    productId,
    variants: [
      { optionValues: [{ optionName: 'Color', name: 'Blue' }] },
      { optionValues: [{ optionName: 'Color', name: 'Green' }] },
    ],
  });
  expectNoUserErrors(
    'productVariantsBulkCreate (variant reorder seed)',
    bulkCreateResponse.data?.productVariantsBulkCreate?.userErrors,
  );
  const seededVariants = bulkCreateResponse.data?.productVariantsBulkCreate?.product?.variants?.nodes ?? [];
  const red = variantByTitle(seededVariants, 'Red');
  const blue = variantByTitle(seededVariants, 'Blue');
  const green = variantByTitle(seededVariants, 'Green');
  const unknownVariantId = 'gid://shopify/ProductVariant/999999999999999999';
  const upstreamCalls = await captureProductMutationPreflight(runGraphqlRaw, productId);

  const cases = [];
  cases.push(
    await captureCase('invalid-position', reorderMutation, {
      productId,
      positions: [
        { id: green.id, position: 0 },
        { id: red.id, position: 2 },
      ],
    }),
  );
  cases.push(
    await captureCase('duplicate-variant-id', reorderMutation, {
      productId,
      positions: [
        { id: blue.id, position: 1 },
        { id: blue.id, position: 2 },
      ],
    }),
  );
  cases.push(
    await captureCase('unknown-variant', reorderMutation, {
      productId,
      positions: [{ id: unknownVariantId, position: 1 }],
    }),
  );
  cases.push(
    await captureCase('success-resequence', reorderMutation, {
      productId,
      positions: [
        { id: green.id, position: 1 },
        { id: red.id, position: 2 },
      ],
    }),
  );

  const reorderDownstreamVariables = { productId };
  const reorderDownstreamRead = await runGraphql(downstreamReadQuery, reorderDownstreamVariables);
  const legacyMutationVariables = {
    productId,
    positions: [
      { id: green.id, position: 1 },
      { id: red.id, position: 2 },
    ],
  };
  const legacyMutationResponse = await runGraphql(legacyReorderMutation, legacyMutationVariables);
  const legacyDownstreamRead = await runGraphql(legacyDownstreamReadQuery, reorderDownstreamVariables);

  const payload = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    setup: {
      createProduct: createProductResponse,
      setupOptions: setupOptionsResponse,
      bulkCreate: bulkCreateResponse,
    },
    notes:
      'Live product variant bulk reorder validation and resequence capture. Rejected reorder branches do not change the product; successful reorder resequences all variant position values contiguously.',
    cases,
    reorderDownstreamRead: {
      request: {
        variables: reorderDownstreamVariables,
      },
      response: reorderDownstreamRead,
    },
    upstreamCalls,
  };

  await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  await writeFile(
    path.join(outputDir, 'product-variants-bulk-reorder-parity.json'),
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        setup: {
          productCreate: createProductResponse,
          productOptionsCreate: setupOptionsResponse,
          productVariantsBulkCreate: bulkCreateResponse,
        },
        mutation: {
          variables: legacyMutationVariables,
          response: legacyMutationResponse,
        },
        downstreamRead: legacyDownstreamRead,
        upstreamCalls,
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
        productId,
        variants: {
          red: red.id,
          blue: blue.id,
          green: green.id,
        },
        cases: cases.map((entry) => entry.name),
      },
      null,
      2,
    ),
  );
} finally {
  await runGraphql(deleteProductMutation, { input: { id: productId } }).catch((error) => {
    console.error(`Cleanup failed for ${productId}: ${String(error)}`);
  });
}
