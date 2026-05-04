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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-create-with-options-conformance-scope-blocker.md');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createMutation = `#graphql
  mutation ProductCreateWithOptionsConformance($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
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
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query ProductCreateWithOptionsDownstreamRead($id: ID!) {
    product(id: $id) {
      id
      title
      handle
      status
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
    }
  }
`;

const deleteMutation = `#graphql
  mutation ProductCreateWithOptionsConformanceDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function buildCreateVariables(runId: string) {
  return {
    product: {
      title: `Hermes Product Options Conformance ${runId}`,
      status: 'DRAFT',
      productOptions: [
        {
          name: 'Color',
          values: [{ name: 'Red' }, { name: 'Blue' }],
        },
        {
          name: 'Size',
          values: [{ name: 'Small' }],
        },
      ],
    },
  };
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Product create with productOptions conformance blocker',
    whatFailed:
      'Attempted to capture live conformance for `productCreate` invoked with `productOptions` so the proxy can be exercised against real Shopify behavior.',
    operations: ['productCreate'],
    blocker,
    whyBlocked:
      'Without a write-capable token, the repo cannot capture the real Shopify response shape that includes the requested productOptions and variants returned by `productCreate`.',
    completedSteps: [
      'added a focused capture harness for `productCreate` with `productOptions` input plus a downstream product read of the same product',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with `write_products`, then rerun `tsx ./scripts/capture-product-create-with-options-conformance.mts`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const createVariables = buildCreateVariables(runId);
let createdProductId: string | null = null;
let createResponse: unknown = null;
let downstreamRead: unknown = null;

try {
  createResponse = await runGraphql(createMutation, createVariables);
  createdProductId = createResponse.data?.productCreate?.product?.id ?? null;
  if (!createdProductId) {
    throw new Error('productCreate with productOptions capture did not return a product id.');
  }

  downstreamRead = await runGraphql(downstreamReadQuery, { id: createdProductId });

  const captures = {
    'product-create-with-options-parity.json': {
      mutation: {
        variables: createVariables,
        response: createResponse,
      },
      downstreamRead,
      upstreamCalls: [],
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
      await runGraphql(deleteMutation, { input: { id: createdProductId } });
    } catch {
      // Best-effort cleanup only.
    }
  }
}
