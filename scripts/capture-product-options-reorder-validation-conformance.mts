// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-options-reorder-validation.json');
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const productSlice = `
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
      selectedOptions {
        name
        value
      }
    }
  }
`;

const userErrorsSlice = `
  field
  message
  code
`;

const createProductMutation = `#graphql
  mutation ProductOptionsReorderValidationCreateProduct($product: ProductCreateInput!) {
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
  mutation ProductOptionsReorderValidationDeleteProduct($input: ProductDeleteInput!) {
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
  mutation ProductOptionsReorderValidationSetupOptions($productId: ID!, $options: [OptionCreateInput!]!) {
    productOptionsCreate(productId: $productId, options: $options) {
      product {
        ${productSlice}
      }
      userErrors {
        ${userErrorsSlice}
      }
    }
  }
`;

const variantsBulkCreateMutation = `#graphql
  mutation ProductOptionsReorderValidationSetupVariants(
    $productId: ID!
    $variants: [ProductVariantsBulkInput!]!
  ) {
    productVariantsBulkCreate(productId: $productId, variants: $variants) {
      product {
        ${productSlice}
      }
      productVariants {
        id
        title
        selectedOptions {
          name
          value
        }
      }
      userErrors {
        ${userErrorsSlice}
      }
    }
  }
`;

const productOptionsReorderMutation = `#graphql
  mutation ProductOptionsReorderValidation($productId: ID!, $options: [OptionReorderInput!]!) {
    productOptionsReorder(productId: $productId, options: $options) {
      product {
        ${productSlice}
      }
      userErrors {
        ${userErrorsSlice}
      }
    }
  }
`;

const productReadQuery = `#graphql
  query ProductOptionsReorderValidationProductRead($productId: ID!) {
    product(id: $productId) {
      ${productSlice}
    }
  }
`;

function readObject(value, label) {
  if (typeof value === 'object' && value !== null && !Array.isArray(value)) return value;
  throw new Error(`${label} was not an object.`);
}

function readArray(value, label) {
  if (Array.isArray(value)) return value;
  throw new Error(`${label} was not an array.`);
}

function readString(value, label) {
  if (typeof value === 'string' && value.length > 0) return value;
  throw new Error(`${label} was not a non-empty string.`);
}

function readMutationPayload(capture, mutationName) {
  return readObject(readObject(capture.result.data, `${mutationName}.data`)[mutationName], mutationName);
}

function readOptionByName(product, name) {
  const option = readArray(product.options, 'product.options').find((candidate) => candidate?.name === name);
  return readObject(option, `option ${name}`);
}

function readOptionValueByName(option, name) {
  const value = readArray(option.optionValues, `${option.name}.optionValues`).find(
    (candidate) => candidate?.name === name,
  );
  return readObject(value, `option value ${option.name}/${name}`);
}

function userErrors(capture, mutationName) {
  return readArray(readMutationPayload(capture, mutationName).userErrors, `${mutationName}.userErrors`);
}

function assertNoUserErrors(capture, mutationName) {
  const errors = userErrors(capture, mutationName);
  if (errors.length > 0) {
    throw new Error(`${mutationName} unexpectedly returned userErrors: ${JSON.stringify(errors)}`);
  }
}

async function captureRequest(query, variables) {
  return {
    query,
    variables,
    result: await runGraphql(query, variables),
  };
}

async function captureRawRequest(query, variables) {
  return {
    query,
    variables,
    result: await runGraphqlRequest(query, variables),
  };
}

await mkdir(outputDir, { recursive: true });

const runId = `product-options-reorder-validation-${Date.now().toString(36)}`;
const productIds = [];

try {
  const productCreate = await captureRequest(createProductMutation, {
    product: {
      title: `${runId} productOptionsReorder`,
      status: 'DRAFT',
    },
  });
  const productId = readString(readMutationPayload(productCreate, 'productCreate').product?.id, 'product id');
  productIds.push(productId);

  const setupOptions = await captureRequest(optionsCreateMutation, {
    productId,
    options: [
      { name: 'Color', values: [{ name: 'Red' }, { name: 'Green' }] },
      { name: 'Size', values: [{ name: 'Small' }] },
    ],
  });
  assertNoUserErrors(setupOptions, 'productOptionsCreate');

  const setupVariants = await captureRequest(variantsBulkCreateMutation, {
    productId,
    variants: [
      {
        price: '1.00',
        optionValues: [
          { optionName: 'Color', name: 'Green' },
          { optionName: 'Size', name: 'Small' },
        ],
      },
    ],
  });
  assertNoUserErrors(setupVariants, 'productVariantsBulkCreate');

  const preMutationRead = await captureRequest(productReadQuery, { productId });
  const preMutationProduct = readObject(preMutationRead.result.data?.product, 'preMutationRead.product');
  const colorOption = readOptionByName(preMutationProduct, 'Color');
  const sizeOption = readOptionByName(preMutationProduct, 'Size');
  const redValue = readOptionValueByName(colorOption, 'Red');
  const greenValue = readOptionValueByName(colorOption, 'Green');
  const smallValue = readOptionValueByName(sizeOption, 'Small');

  const unknownOptionName = await captureRequest(productOptionsReorderMutation, {
    productId,
    options: [{ name: 'Missing option' }],
  });
  const unknownOptionId = await captureRequest(productOptionsReorderMutation, {
    productId,
    options: [{ id: 'gid://shopify/ProductOption/999999999' }],
  });
  const unknownValueName = await captureRequest(productOptionsReorderMutation, {
    productId,
    options: [
      { name: 'Color', values: [{ name: 'Missing value' }] },
      { name: 'Size', values: [{ name: 'Small' }] },
    ],
  });
  const unknownValueId = await captureRequest(productOptionsReorderMutation, {
    productId,
    options: [
      {
        id: readString(colorOption.id, 'color option id'),
        values: [{ id: 'gid://shopify/ProductOptionValue/999999999' }],
      },
      {
        id: readString(sizeOption.id, 'size option id'),
        values: [{ id: readString(smallValue.id, 'small value id') }],
      },
    ],
  });
  const duplicateOptionName = await captureRequest(productOptionsReorderMutation, {
    productId,
    options: [{ name: 'Color' }, { name: 'Color' }],
  });
  const duplicateValueName = await captureRequest(productOptionsReorderMutation, {
    productId,
    options: [
      { name: 'Color', values: [{ name: 'Red' }, { name: 'Red' }] },
      { name: 'Size', values: [{ name: 'Small' }] },
    ],
  });
  const mixedOptionSelectors = await captureRequest(productOptionsReorderMutation, {
    productId,
    options: [{ id: readString(colorOption.id, 'color option id') }, { name: 'Size' }],
  });
  const mixedValueSelectors = await captureRequest(productOptionsReorderMutation, {
    productId,
    options: [
      {
        id: readString(colorOption.id, 'color option id'),
        values: [{ id: readString(redValue.id, 'red value id') }, { name: 'Green' }],
      },
      {
        id: readString(sizeOption.id, 'size option id'),
        values: [{ id: readString(smallValue.id, 'small value id') }],
      },
    ],
  });

  const successReorder = await captureRequest(productOptionsReorderMutation, {
    productId,
    options: [
      {
        id: readString(sizeOption.id, 'size option id'),
        values: [{ id: readString(smallValue.id, 'small value id') }],
      },
      {
        id: readString(colorOption.id, 'color option id'),
        values: [{ id: readString(greenValue.id, 'green value id') }, { id: readString(redValue.id, 'red value id') }],
      },
    ],
  });
  assertNoUserErrors(successReorder, 'productOptionsReorder');
  const downstreamRead = await captureRequest(productReadQuery, { productId });

  const missingOptionSelectorSchemaError = await captureRawRequest(productOptionsReorderMutation, {
    productId,
    options: [{ values: [] }],
  });
  const missingValueSelectorSchemaError = await captureRawRequest(productOptionsReorderMutation, {
    productId,
    options: [{ name: 'Color', values: [{}] }],
  });
  const positionSchemaError = await captureRawRequest(productOptionsReorderMutation, {
    productId,
    options: [{ id: readString(colorOption.id, 'color option id'), position: 2 }],
  });

  const capture = {
    notes: [
      'Captured public Admin API productOptionsReorder validation branches plus a successful option/value reorder.',
      'Public schema rejects missing oneOf selectors and the internal position key before ProductOptionsReorderPayload is returned; those raw responses are recorded as schemaErrorCaptures while local inline-parser guardrails are covered by runtime tests.',
    ],
    run: {
      runId,
      storeDomain,
      apiVersion,
      productId,
    },
    captures: {
      productCreate,
      setupOptions,
      setupVariants,
      preMutationRead,
      unknownOptionName,
      unknownOptionId,
      unknownValueName,
      unknownValueId,
      duplicateOptionName,
      duplicateValueName,
      mixedOptionSelectors,
      mixedValueSelectors,
      successReorder,
      downstreamRead,
    },
    schemaErrorCaptures: {
      missingOptionSelectorSchemaError,
      missingValueSelectorSchemaError,
      positionSchemaError,
    },
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        productId,
        capturedBranches: Object.keys(capture.captures).length,
        schemaErrorBranches: Object.keys(capture.schemaErrorCaptures).length,
      },
      null,
      2,
    ),
  );
} finally {
  for (const productId of productIds.reverse()) {
    try {
      await runGraphql(deleteProductMutation, { input: { id: productId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }
}
