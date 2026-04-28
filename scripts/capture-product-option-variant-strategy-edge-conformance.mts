// @ts-nocheck
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const userErrorsSlice = `
  field
  message
  code
`;

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
  variants(first: 110) {
    nodes {
      id
      title
      sku
      barcode
      price
      inventoryQuantity
      selectedOptions {
        name
        value
      }
      inventoryItem {
        id
        tracked
        requiresShipping
      }
    }
  }
`;

const productSlice = `
  id
  options {
    ${optionsSlice}
  }
  totalInventory
  tracksInventory
  ${variantsSlice}
`;

const createProductMutation = `#graphql
  mutation ProductOptionStrategyEdgeCreateProduct($product: ProductCreateInput!) {
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
  mutation ProductOptionStrategyEdgeDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const productReadQuery = `#graphql
  query ProductOptionStrategyEdgeDownstream($id: ID!) {
    product(id: $id) {
      ${productSlice}
    }
  }
`;

const optionsCreateMutation = `#graphql
  mutation ProductOptionsCreateStrategyEdge(
    $productId: ID!
    $options: [OptionCreateInput!]!
    $variantStrategy: ProductOptionCreateVariantStrategy
  ) {
    productOptionsCreate(productId: $productId, options: $options, variantStrategy: $variantStrategy) {
      product {
        ${productSlice}
      }
      userErrors {
        ${userErrorsSlice}
      }
    }
  }
`;

const bulkCreateMutation = `#graphql
  mutation ProductVariantsBulkCreateStrategyEdge(
    $productId: ID!
    $variants: [ProductVariantsBulkInput!]!
    $strategy: ProductVariantsBulkCreateStrategy
  ) {
    productVariantsBulkCreate(productId: $productId, variants: $variants, strategy: $strategy) {
      product {
        ${productSlice}
      }
      productVariants {
        id
        title
        sku
        barcode
        price
        inventoryQuantity
        selectedOptions {
          name
          value
        }
        inventoryItem {
          id
          tracked
          requiresShipping
        }
      }
      userErrors {
        ${userErrorsSlice}
      }
    }
  }
`;

function buildCreateProductVariables(runId, suffix) {
  return {
    product: {
      title: `Hermes Product Option Strategy Edge ${runId} ${suffix}`,
      status: 'DRAFT',
    },
  };
}

function optionValues(prefix, count) {
  return Array.from({ length: count }, (_, index) => ({
    name: `${prefix}-${String(index + 1).padStart(3, '0')}`,
  }));
}

function optionCreateVariables(productId, variantStrategy) {
  return {
    productId,
    variantStrategy,
    options: [
      {
        name: 'Color',
        position: 1,
        values: [{ name: 'Red' }, { name: 'Green' }],
      },
    ],
  };
}

function bulkCreateVariables(productId, strategy, valueName, sku) {
  return {
    productId,
    strategy,
    variants: [
      {
        optionValues: [{ optionName: 'Title', name: valueName }],
        price: '25.00',
        inventoryItem: {
          sku,
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  };
}

function bulkCreateCustomOptionVariables(productId, strategy, valueName, sku) {
  return {
    productId,
    strategy,
    variants: [
      {
        optionValues: [{ optionName: 'Color', name: valueName }],
        price: '25.00',
        inventoryItem: {
          sku,
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  };
}

async function createProduct(runId, suffix, productIds) {
  const response = await runGraphql(createProductMutation, buildCreateProductVariables(runId, suffix));
  const productId = response.data?.productCreate?.product?.id ?? null;
  if (!productId) {
    throw new Error(`Product create for ${suffix} did not return a product id.`);
  }
  productIds.push(productId);
  return productId;
}

async function captureOptionStrategyScenario(runId, suffix, variantStrategy) {
  const productIds = [];
  const productId = await createProduct(runId, suffix, productIds);
  const variables = optionCreateVariables(productId, variantStrategy);
  const preMutationRead = await runGraphql(productReadQuery, { id: productId });
  const response = await runGraphql(optionsCreateMutation, variables);
  const downstreamRead = await runGraphql(productReadQuery, { id: productId });
  return {
    productIds,
    payload: {
      preMutationRead,
      mutation: { variables, response },
      downstreamRead,
    },
  };
}

async function captureCreateOverLimitScenario(runId) {
  const productIds = [];
  const productId = await createProduct(runId, 'create-over-limit', productIds);
  const setupVariables = {
    productId,
    variantStrategy: 'CREATE',
    options: [
      {
        name: 'Seed',
        position: 1,
        values: optionValues('Seed', 10),
      },
    ],
  };
  const setupResponse = await runGraphql(optionsCreateMutation, setupVariables);
  const setupErrors = setupResponse.data?.productOptionsCreate?.userErrors ?? [];
  if (setupErrors.length > 0) {
    throw new Error(`CREATE limit setup returned userErrors: ${JSON.stringify(setupErrors)}`);
  }

  const variables = {
    productId,
    variantStrategy: 'CREATE',
    options: [
      {
        name: 'Second',
        position: 2,
        values: optionValues('Second', 11),
      },
    ],
  };
  const preMutationRead = await runGraphql(productReadQuery, { id: productId });
  const response = await runGraphql(optionsCreateMutation, variables);
  const downstreamRead = await runGraphql(productReadQuery, { id: productId });
  return {
    productIds,
    payload: {
      setup: {
        variables: setupVariables,
        response: setupResponse,
      },
      preMutationRead,
      mutation: { variables, response },
      downstreamRead,
    },
  };
}

async function captureBulkDefaultStandaloneScenario(runId, suffix, strategy) {
  const productIds = [];
  const productId = await createProduct(runId, suffix, productIds);
  const variables = bulkCreateVariables(
    productId,
    strategy,
    `${strategy === 'REMOVE_STANDALONE_VARIANT' ? 'Remove' : 'Default'} Blue`,
    `HERMES-${runId}-${suffix}`.toUpperCase(),
  );
  const preMutationRead = await runGraphql(productReadQuery, { id: productId });
  const response = await runGraphql(bulkCreateMutation, variables);
  const downstreamRead = await runGraphql(productReadQuery, { id: productId });
  return {
    productIds,
    payload: {
      preMutationRead,
      mutation: { variables, response },
      downstreamRead,
    },
  };
}

async function captureBulkCustomStandaloneScenario(runId, suffix, strategy) {
  const productIds = [];
  const productId = await createProduct(runId, suffix, productIds);
  const setupVariables = {
    productId,
    variantStrategy: 'LEAVE_AS_IS',
    options: [
      {
        name: 'Color',
        position: 1,
        values: [{ name: 'Red' }],
      },
    ],
  };
  const setupResponse = await runGraphql(optionsCreateMutation, setupVariables);
  const setupErrors = setupResponse.data?.productOptionsCreate?.userErrors ?? [];
  if (setupErrors.length > 0) {
    throw new Error(`custom standalone setup returned userErrors: ${JSON.stringify(setupErrors)}`);
  }

  const variables = bulkCreateCustomOptionVariables(
    productId,
    strategy,
    `${strategy === 'REMOVE_STANDALONE_VARIANT' ? 'Remove' : 'Default'} Blue`,
    `HERMES-${runId}-${suffix}`.toUpperCase(),
  );
  const preMutationRead = await runGraphql(productReadQuery, { id: productId });
  const response = await runGraphql(bulkCreateMutation, variables);
  const downstreamRead = await runGraphql(productReadQuery, { id: productId });
  return {
    productIds,
    payload: {
      setup: {
        variables: setupVariables,
        response: setupResponse,
      },
      preMutationRead,
      mutation: { variables, response },
      downstreamRead,
    },
  };
}

async function cleanupProducts(productIds) {
  for (const productId of productIds) {
    try {
      await runGraphql(deleteProductMutation, { input: { id: productId } });
    } catch {
      // Best-effort cleanup. The capture result should preserve the original failure.
    }
  }
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const productIds = [];

try {
  const captures = {};
  const optionLeaveAsIs = await captureOptionStrategyScenario(runId, 'option-leave-as-is', 'LEAVE_AS_IS');
  productIds.push(...optionLeaveAsIs.productIds);
  captures['product-options-create-variant-strategy-leave-as-is-parity.json'] = optionLeaveAsIs.payload;

  const optionNull = await captureOptionStrategyScenario(runId, 'option-null', null);
  productIds.push(...optionNull.productIds);
  captures['product-options-create-variant-strategy-null-parity.json'] = optionNull.payload;

  const createOverLimit = await captureCreateOverLimitScenario(runId);
  productIds.push(...createOverLimit.productIds);
  captures['product-options-create-variant-strategy-create-over-default-limit.json'] = createOverLimit.payload;

  const bulkDefaultDefault = await captureBulkDefaultStandaloneScenario(runId, 'bulk-default-default', 'DEFAULT');
  productIds.push(...bulkDefaultDefault.productIds);
  captures['productVariantsBulkCreate-strategy-default-default-standalone.json'] = bulkDefaultDefault.payload;

  const bulkRemoveDefault = await captureBulkDefaultStandaloneScenario(
    runId,
    'bulk-remove-default',
    'REMOVE_STANDALONE_VARIANT',
  );
  productIds.push(...bulkRemoveDefault.productIds);
  captures['productVariantsBulkCreate-strategy-remove-default-standalone.json'] = bulkRemoveDefault.payload;

  const bulkDefaultCustom = await captureBulkCustomStandaloneScenario(runId, 'bulk-default-custom', 'DEFAULT');
  productIds.push(...bulkDefaultCustom.productIds);
  captures['productVariantsBulkCreate-strategy-default-custom-standalone.json'] = bulkDefaultCustom.payload;

  const bulkRemoveCustom = await captureBulkCustomStandaloneScenario(
    runId,
    'bulk-remove-custom',
    'REMOVE_STANDALONE_VARIANT',
  );
  productIds.push(...bulkRemoveCustom.productIds);
  captures['productVariantsBulkCreate-strategy-remove-custom-standalone.json'] = bulkRemoveCustom.payload;

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
        productIds,
      },
      null,
      2,
    ),
  );
} finally {
  await cleanupProducts(productIds);
}
