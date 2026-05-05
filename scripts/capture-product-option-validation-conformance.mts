// @ts-nocheck
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createProductMutation = `#graphql
  mutation ProductOptionValidationCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteProductMutation = `#graphql
  mutation ProductOptionValidationDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const optionsCreateMutation = `#graphql
  mutation ProductOptionsCreateValidation(
    $productId: ID!
    $options: [OptionCreateInput!]!
    $variantStrategy: ProductOptionCreateVariantStrategy
  ) {
    productOptionsCreate(productId: $productId, options: $options, variantStrategy: $variantStrategy) {
      product {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const variantsBulkCreateMutation = `#graphql
  mutation ProductOptionValidationBulkCreateVariants(
    $productId: ID!
    $variants: [ProductVariantsBulkInput!]!
  ) {
    productVariantsBulkCreate(productId: $productId, variants: $variants) {
      productVariants {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function repeatedText(item, count) {
  return Array.from({ length: count }, () => item).join('');
}

function optionValues(prefix, count) {
  return Array.from({ length: count }, (_, index) => ({ name: `${prefix}${index + 1}` }));
}

function largeVariantInputs() {
  const variants = [];
  for (let color = 1; color <= 50; color++) {
    for (let size = 1; size <= 30; size++) {
      if (color === 1 && size === 1) continue;
      variants.push({
        price: '1.00',
        optionValues: [
          { optionName: 'Color', name: `Color${color}` },
          { optionName: 'Size', name: `Size${size}` },
        ],
      });
    }
  }
  return variants;
}

async function captureRequest(query, variables) {
  return {
    query,
    variables,
    result: await runGraphql(query, variables),
  };
}

await mkdir(outputDir, { recursive: true });

const runId = `har-590-${Date.now()}`;
const createdProductIds = [];

try {
  const productCreateVariables = {
    product: {
      title: `${runId} Product Option Validation`,
      status: 'DRAFT',
    },
  };
  const productCreate = await captureRequest(createProductMutation, productCreateVariables);
  const productId = productCreate.result.data?.productCreate?.product?.id ?? null;
  if (!productId) {
    throw new Error('Product option validation capture did not create the primary product.');
  }
  createdProductIds.push(productId);

  const duplicateOptionName = await captureRequest(optionsCreateMutation, {
    productId,
    variantStrategy: null,
    options: [
      { name: 'Color', values: [{ name: 'Red' }] },
      { name: 'Color', values: [{ name: 'Blue' }] },
    ],
  });
  const emptyValues = await captureRequest(optionsCreateMutation, {
    productId,
    variantStrategy: null,
    options: [{ name: 'Color', values: [] }],
  });
  const optionNameTooLong = await captureRequest(optionsCreateMutation, {
    productId,
    variantStrategy: null,
    options: [{ name: repeatedText('N', 256), values: [{ name: 'Red' }] }],
  });

  const setupColorOptionVariables = {
    productId,
    variantStrategy: null,
    options: [{ name: 'Color', values: [{ name: 'Red' }] }],
  };
  const setupColorOption = await captureRequest(optionsCreateMutation, setupColorOptionVariables);
  const optionAlreadyExists = await captureRequest(optionsCreateMutation, {
    productId,
    variantStrategy: null,
    options: [{ name: 'Color', values: [{ name: 'Blue' }] }],
  });

  const setupRemainingOptionsVariables = {
    productId,
    variantStrategy: null,
    options: [
      { name: 'Size', values: [{ name: 'Small' }] },
      { name: 'Material', values: [{ name: 'Cotton' }] },
    ],
  };
  const setupRemainingOptions = await captureRequest(optionsCreateMutation, setupRemainingOptionsVariables);
  const optionsOverLimit = await captureRequest(optionsCreateMutation, {
    productId,
    variantStrategy: null,
    options: [{ name: 'Finish', values: [{ name: 'Matte' }] }],
  });

  const largeProductCreateVariables = {
    product: {
      title: `${runId} Product Option Variant Limit`,
      status: 'DRAFT',
    },
  };
  const largeProductCreate = await captureRequest(createProductMutation, largeProductCreateVariables);
  const largeProductId = largeProductCreate.result.data?.productCreate?.product?.id ?? null;
  if (!largeProductId) {
    throw new Error('Product option validation capture did not create the large variant product.');
  }
  createdProductIds.push(largeProductId);

  const setupLargeOptionsVariables = {
    productId: largeProductId,
    variantStrategy: null,
    options: [
      { name: 'Color', values: optionValues('Color', 50) },
      { name: 'Size', values: optionValues('Size', 30) },
    ],
  };
  const setupLargeOptions = await captureRequest(optionsCreateMutation, setupLargeOptionsVariables);
  const setupLargeVariantsVariables = {
    productId: largeProductId,
    variants: largeVariantInputs(),
  };
  const setupLargeVariants = await captureRequest(variantsBulkCreateMutation, setupLargeVariantsVariables);
  const tooManyVariantsCreated = await captureRequest(optionsCreateMutation, {
    productId: largeProductId,
    variantStrategy: 'CREATE',
    options: [{ name: 'Material', values: [{ name: 'Cotton' }, { name: 'Wool' }] }],
  });

  const capture = {
    notes: [
      'HAR-590 productOptionsCreate validation parity capture.',
      'The primary product is reused for atomic validation branches, then configured with options for collision and limit checks.',
      'The large product is configured with 1500 variants so adding a two-value third option with variantStrategy CREATE would create 3000 variants.',
    ],
    run: {
      runId,
      storeDomain,
      apiVersion,
      productId,
      largeProductId,
    },
    captures: {
      productCreate,
      duplicateOptionName,
      emptyValues,
      optionNameTooLong,
      setupColorOption,
      optionAlreadyExists,
      setupRemainingOptions,
      optionsOverLimit,
      largeProductCreate,
      setupLargeOptions,
      setupLargeVariants,
      tooManyVariantsCreated,
    },
    upstreamCalls: [],
  };

  const filename = 'product-options-create-limits-and-duplicates-parity.json';
  await writeFile(path.join(outputDir, filename), `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: [filename],
        productId,
        largeProductId,
        largeVariantInputs: setupLargeVariantsVariables.variants.length,
      },
      null,
      2,
    ),
  );
} finally {
  for (const productId of createdProductIds.reverse()) {
    try {
      await runGraphql(deleteProductMutation, { input: { id: productId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }
}
