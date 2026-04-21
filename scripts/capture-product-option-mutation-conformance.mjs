import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  // oxlint-disable-next-line no-console -- CLI error output is intentionally written to stderr.
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-option-mutation-conformance-scope-blocker.md');

async function runGraphql(query, variables = {}) {
  const response = await fetch(`${adminOrigin}/admin/api/${apiVersion}/graphql.json`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...buildAdminAuthHeaders(adminAccessToken),
    },
    body: JSON.stringify({ query, variables }),
  });

  const payload = await response.json();
  if (!response.ok || payload.errors) {
    const error = new Error(JSON.stringify({ status: response.status, payload }, null, 2));
    error.result = { status: response.status, payload };
    throw error;
  }

  return payload;
}

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
        id
        options {
          ${optionsSlice}
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const optionUpdateMutation = `#graphql
  mutation ProductOptionUpdateConformance($productId: ID!, $option: OptionUpdateInput!, $optionValuesToAdd: [OptionValueCreateInput!], $optionValuesToUpdate: [OptionValueUpdateInput!]) {
    productOptionUpdate(productId: $productId, option: $option, optionValuesToAdd: $optionValuesToAdd, optionValuesToUpdate: $optionValuesToUpdate) {
      product {
        id
        options {
          ${optionsSlice}
        }
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
        id
        options {
          ${optionsSlice}
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
  query ProductOptionDownstream($id: ID!) {
    product(id: $id) {
      id
      options {
        ${optionsSlice}
      }
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
        values: [{ name: 'Red' }],
      },
    ],
  };
}

function buildOptionUpdateVariables(productId, optionId, redValueId) {
  return {
    productId,
    option: {
      id: optionId,
      name: 'Shade',
      position: 2,
    },
    optionValuesToAdd: [{ name: 'Blue' }],
    optionValuesToUpdate: [{ id: redValueId, name: 'Crimson' }],
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
let optionsCreateResponse = null;
let optionUpdateResponse = null;
let optionsDeleteResponse = null;

try {
  const createProductResponse = await runGraphql(createProductMutation, createProductVariables);
  createdProductId = createProductResponse.data?.productCreate?.product?.id ?? null;
  if (!createdProductId) {
    throw new Error('Product option capture did not return a product id.');
  }

  const optionsCreateVariables = buildOptionsCreateVariables(createdProductId);
  optionsCreateResponse = await runGraphql(optionsCreateMutation, optionsCreateVariables);
  const createdOptions = optionsCreateResponse.data?.productOptionsCreate?.product?.options ?? [];
  const createdOption = Array.isArray(createdOptions)
    ? (createdOptions.find((option) => option?.name === 'Color') ?? null)
    : null;
  const createdOptionId = typeof createdOption?.id === 'string' ? createdOption.id : null;
  const redValueId = Array.isArray(createdOption?.optionValues)
    ? (createdOption.optionValues.find((value) => value?.name === 'Red')?.id ?? null)
    : null;
  if (!createdOptionId || !redValueId) {
    throw new Error('Option create capture did not yield the created option/value ids.');
  }
  const postCreateRead = await runGraphql(downstreamReadQuery, { id: createdProductId });

  const optionUpdateVariables = buildOptionUpdateVariables(createdProductId, createdOptionId, redValueId);
  optionUpdateResponse = await runGraphql(optionUpdateMutation, optionUpdateVariables);
  const postUpdateRead = await runGraphql(downstreamReadQuery, { id: createdProductId });

  const optionsDeleteVariables = { productId: createdProductId, options: [createdOptionId] };
  optionsDeleteResponse = await runGraphql(optionsDeleteMutation, optionsDeleteVariables);
  const postDeleteRead = await runGraphql(downstreamReadQuery, { id: createdProductId });

  const captures = {
    'product-options-create-parity.json': {
      mutation: {
        variables: optionsCreateVariables,
        response: optionsCreateResponse,
      },
      downstreamRead: postCreateRead,
    },
    'product-option-update-parity.json': {
      mutation: {
        variables: optionUpdateVariables,
        response: optionUpdateResponse,
      },
      downstreamRead: postUpdateRead,
    },
    'product-options-delete-parity.json': {
      mutation: {
        variables: optionsDeleteVariables,
        response: optionsDeleteResponse,
      },
      downstreamRead: postDeleteRead,
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
}
