// @ts-nocheck
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-option-mutation-conformance-scope-blocker.md');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const optionsSlice = `
  id
  name
  position
  values
  optionValues {
    id
    name
    hasVariants
  }
`;

const variantsSlice = `
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

const productOptionLifecycleSlice = `
  id
  options {
    ${optionsSlice}
  }
  ${variantsSlice}
`;

const createProductMutation = `#graphql
  mutation ProductOptionConformanceCreateProduct($product: ProductCreateInput!) {
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
  mutation ProductOptionConformanceDeleteProduct($input: ProductDeleteInput!) {
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
  mutation ProductOptionsCreateConformance($productId: ID!, $options: [OptionCreateInput!]!) {
    productOptionsCreate(productId: $productId, options: $options) {
      product {
        ${productOptionLifecycleSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const optionsCreateVariantStrategyMutation = `#graphql
  mutation ProductOptionsCreateVariantStrategyConformance(
    $productId: ID!
    $options: [OptionCreateInput!]!
    $variantStrategy: ProductOptionCreateVariantStrategy
  ) {
    productOptionsCreate(productId: $productId, options: $options, variantStrategy: $variantStrategy) {
      product {
        ${productOptionLifecycleSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const optionUpdateMutation = `#graphql
  mutation ProductOptionUpdateConformance(
    $productId: ID!
    $option: OptionUpdateInput!
    $optionValuesToAdd: [OptionValueCreateInput!]
    $optionValuesToUpdate: [OptionValueUpdateInput!]
    $optionValuesToDelete: [ID!]
  ) {
    productOptionUpdate(
      productId: $productId
      option: $option
      optionValuesToAdd: $optionValuesToAdd
      optionValuesToUpdate: $optionValuesToUpdate
      optionValuesToDelete: $optionValuesToDelete
    ) {
      product {
        ${productOptionLifecycleSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const optionsDeleteMutation = `#graphql
  mutation ProductOptionsDeleteConformance($productId: ID!, $options: [ID!]!) {
    productOptionsDelete(productId: $productId, options: $options) {
      deletedOptionsIds
      product {
        ${productOptionLifecycleSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query ProductOptionDownstream($id: ID!) {
    product(id: $id) {
      ${productOptionLifecycleSlice}
    }
  }
`;

function buildCreateProductVariables(runId) {
  return {
    product: {
      title: `Hermes Product Option Conformance ${runId}`,
      status: 'DRAFT',
    },
  };
}

function buildOptionsCreateVariables(productId) {
  return {
    productId,
    options: [
      {
        name: 'Color',
        position: 1,
        values: [{ name: 'Red' }, { name: 'Green' }],
      },
      {
        name: 'Size',
        position: 2,
        values: [{ name: 'Small' }],
      },
    ],
  };
}

function buildOptionsCreateVariantStrategyVariables(productId) {
  return {
    productId,
    variantStrategy: 'CREATE',
    options: [
      {
        name: 'Color',
        position: 1,
        values: [{ name: 'Blue' }, { name: 'Green' }],
      },
    ],
  };
}

function buildOptionUpdateVariables(productId, optionId, redValueId, greenValueId) {
  return {
    productId,
    option: {
      id: optionId,
      name: 'Shade',
      position: 2,
    },
    optionValuesToAdd: [{ name: 'Blue' }],
    optionValuesToUpdate: [{ id: redValueId, name: 'Crimson' }],
    optionValuesToDelete: [greenValueId],
  };
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Product option mutation conformance blocker',
    whatFailed:
      'Attempted to capture live conformance for the staged product option mutation family (`productOptionsCreate`, `productOptionUpdate`, `productOptionsDelete`).',
    operations: ['productOptionsCreate', 'productOptionUpdate', 'productOptionsDelete'],
    blocker,
    whyBlocked:
      'Without a write-capable token, the repo cannot capture successful live option-mutation payload shape, userErrors behavior, or immediate downstream `product.options` parity for this family.',
    completedSteps: [
      'added a reusable live-write capture harness for staged product option mutations',
      'aligned the option mutation and downstream read slices with the existing parity-request scaffolds so future runs capture the same merchant-facing option fields directly',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with product write permissions, then rerun `corepack pnpm conformance:capture-product-option-mutations`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const createProductVariables = buildCreateProductVariables(runId);
let createdProductId = null;
let variantStrategyProductId = null;
let optionsCreateResponse = null;
let optionUpdateResponse = null;
let optionsDeleteResponse = null;

try {
  const createProductResponse = await runGraphql(createProductMutation, createProductVariables);
  createdProductId = createProductResponse.data?.productCreate?.product?.id ?? null;
  if (!createdProductId) {
    throw new Error('Product option capture did not return a product id.');
  }

  const preCreateRead = await runGraphql(downstreamReadQuery, { id: createdProductId });
  const optionsCreateVariables = buildOptionsCreateVariables(createdProductId);
  optionsCreateResponse = await runGraphql(optionsCreateMutation, optionsCreateVariables);
  const createdOptions = optionsCreateResponse.data?.productOptionsCreate?.product?.options ?? [];
  const createdOption = Array.isArray(createdOptions)
    ? (createdOptions.find((option) => option?.name === 'Color') ?? null)
    : null;
  const sizeOption = Array.isArray(createdOptions)
    ? (createdOptions.find((option) => option?.name === 'Size') ?? null)
    : null;
  const createdOptionId = typeof createdOption?.id === 'string' ? createdOption.id : null;
  const sizeOptionId = typeof sizeOption?.id === 'string' ? sizeOption.id : null;
  const redValueId = Array.isArray(createdOption?.optionValues)
    ? (createdOption.optionValues.find((value) => value?.name === 'Red')?.id ?? null)
    : null;
  const greenValueId = Array.isArray(createdOption?.optionValues)
    ? (createdOption.optionValues.find((value) => value?.name === 'Green')?.id ?? null)
    : null;
  if (!createdOptionId || !sizeOptionId || !redValueId || !greenValueId) {
    throw new Error('Option create capture did not yield the created option/value ids.');
  }
  const postCreateRead = await runGraphql(downstreamReadQuery, { id: createdProductId });

  const preUpdateRead = await runGraphql(downstreamReadQuery, { id: createdProductId });
  const optionUpdateVariables = buildOptionUpdateVariables(createdProductId, createdOptionId, redValueId, greenValueId);
  optionUpdateResponse = await runGraphql(optionUpdateMutation, optionUpdateVariables);
  const postUpdateRead = await runGraphql(downstreamReadQuery, { id: createdProductId });

  const preDeleteRead = await runGraphql(downstreamReadQuery, { id: createdProductId });
  const optionsDeleteVariables = { productId: createdProductId, options: [sizeOptionId, createdOptionId] };
  optionsDeleteResponse = await runGraphql(optionsDeleteMutation, optionsDeleteVariables);
  const postDeleteRead = await runGraphql(downstreamReadQuery, { id: createdProductId });
  const validation = {
    createUnknownProduct: await runGraphql(optionsCreateMutation, {
      productId: 'gid://shopify/Product/0',
      options: [{ name: 'Material', values: [{ name: 'Cotton' }] }],
    }),
    updateUnknownOption: await runGraphql(optionUpdateMutation, {
      productId: createdProductId,
      option: { id: 'gid://shopify/ProductOption/0', name: 'Missing Option' },
      optionValuesToAdd: [{ name: 'Ghost' }],
      optionValuesToUpdate: [],
      optionValuesToDelete: [],
    }),
    deleteUnknownOption: await runGraphql(optionsDeleteMutation, {
      productId: createdProductId,
      options: ['gid://shopify/ProductOption/0'],
    }),
  };

  const variantStrategyCreateProductResponse = await runGraphql(
    createProductMutation,
    buildCreateProductVariables(`${runId}-variant-strategy-create`),
  );
  variantStrategyProductId = variantStrategyCreateProductResponse.data?.productCreate?.product?.id ?? null;
  if (!variantStrategyProductId) {
    throw new Error('Product option variantStrategy CREATE capture did not return a product id.');
  }
  const variantStrategyPreCreateRead = await runGraphql(downstreamReadQuery, { id: variantStrategyProductId });
  const variantStrategyCreateVariables = buildOptionsCreateVariantStrategyVariables(variantStrategyProductId);
  const variantStrategyCreateResponse = await runGraphql(
    optionsCreateVariantStrategyMutation,
    variantStrategyCreateVariables,
  );
  const variantStrategyPostCreateRead = await runGraphql(downstreamReadQuery, { id: variantStrategyProductId });

  const captures = {
    'product-options-create-parity.json': {
      preMutationRead: preCreateRead,
      mutation: {
        variables: optionsCreateVariables,
        response: optionsCreateResponse,
      },
      downstreamRead: postCreateRead,
      validation: {
        createUnknownProduct: validation.createUnknownProduct,
      },
    },
    'product-option-update-parity.json': {
      preMutationRead: preUpdateRead,
      mutation: {
        variables: optionUpdateVariables,
        response: optionUpdateResponse,
      },
      downstreamRead: postUpdateRead,
      validation: {
        updateUnknownOption: validation.updateUnknownOption,
      },
    },
    'product-options-delete-parity.json': {
      preMutationRead: preDeleteRead,
      mutation: {
        variables: optionsDeleteVariables,
        response: optionsDeleteResponse,
      },
      downstreamRead: postDeleteRead,
      validation: {
        deleteUnknownOption: validation.deleteUnknownOption,
      },
    },
    'product-options-create-variant-strategy-create-parity.json': {
      preMutationRead: variantStrategyPreCreateRead,
      mutation: {
        variables: variantStrategyCreateVariables,
        response: variantStrategyCreateResponse,
      },
      downstreamRead: variantStrategyPostCreateRead,
    },
  };

  for (const [filename, payload] of Object.entries(captures)) {
    await writeFile(path.join(outputDir, filename), `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  }

  // oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: Object.keys(captures),
        productId: createdProductId,
        variantStrategyProductId,
        createdOptionId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const blocker = parseWriteScopeBlocker(error?.result ?? null);
  if (blocker) {
    await writeScopeBlocker(blocker);
    // oxlint-disable-next-line no-console -- CLI blocker result is intentionally written to stdout.
    console.log(
      JSON.stringify(
        {
          ok: false,
          blocked: true,
          blockerPath,
          blocker,
        },
        null,
        2,
      ),
    );
    process.exit(1);
  }

  throw error;
} finally {
  if (createdProductId) {
    try {
      await runGraphql(deleteProductMutation, { input: { id: createdProductId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }
  if (variantStrategyProductId) {
    try {
      await runGraphql(deleteProductMutation, { input: { id: variantStrategyProductId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }
}
