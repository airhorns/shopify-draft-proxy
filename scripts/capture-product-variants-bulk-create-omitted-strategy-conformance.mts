// @ts-nocheck
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputFilename = 'productVariantsBulkCreate-omitted-strategy-default-standalone.json';
const requestDir = path.join('config', 'parity-requests', 'products');
const createProductMutation = await readFile(
  path.join(requestDir, 'productVariantCompatibility-setup-product.graphql'),
  'utf8',
);
const bulkCreateMutation = await readFile(
  path.join(requestDir, 'productVariantsBulkCreate-omitted-strategy.graphql'),
  'utf8',
);
const productReadQuery = await readFile(
  path.join(requestDir, 'product-option-variant-strategy-edge-downstream-read.graphql'),
  'utf8',
);

const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const deleteProductMutation = `#graphql
  mutation ProductVariantsBulkCreateOmittedStrategyDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function expectNoUserErrors(label, userErrors) {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
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
  const setupVariables = {
    product: {
      title: `Hermes Bulk Create Omitted Strategy ${runId}`,
      status: 'DRAFT',
    },
  };
  const create = await runGraphql(createProductMutation, setupVariables);
  expectNoUserErrors('productCreate setup', create.data?.productCreate?.userErrors ?? null);
  const productId = create.data?.productCreate?.product?.id ?? null;
  if (!productId) {
    throw new Error('productCreate setup did not return a product id.');
  }
  productIds.push(productId);

  const variables = {
    productId,
    variants: [
      {
        optionValues: [{ optionName: 'Title', name: 'Default Blue' }],
        price: '25.00',
        inventoryItem: {
          sku: `HERMES-${runId}-BULK-OMITTED-DEFAULT`,
          tracked: true,
          requiresShipping: true,
        },
      },
    ],
  };
  const preMutationRead = await runGraphql(productReadQuery, { id: productId });
  const response = await runGraphql(bulkCreateMutation, variables);
  expectNoUserErrors(
    'productVariantsBulkCreate omitted strategy',
    response.data?.productVariantsBulkCreate?.userErrors ?? null,
  );
  const downstreamRead = await runGraphql(productReadQuery, { id: productId });

  const payload = {
    setup: {
      variables: setupVariables,
      response: create,
    },
    preMutationRead,
    mutation: {
      variables,
      response,
    },
    downstreamRead,
  };
  await writeFile(path.join(outputDir, outputFilename), `${JSON.stringify(payload, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: [outputFilename],
        productIds,
      },
      null,
      2,
    ),
  );
} finally {
  await cleanupProducts(productIds);
}
