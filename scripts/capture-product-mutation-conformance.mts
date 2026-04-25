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
const blockerPath = path.join(pendingDir, 'product-mutation-conformance-scope-blocker.md');
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function runGraphqlAllowGraphqlErrors(query, variables = {}) {
  const result = await runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300) {
    const error = new Error(JSON.stringify(result, null, 2));
    error.result = result;
    throw error;
  }

  return result.payload;
}

const productDetailQuery = `#graphql
  query ProductMutationConformanceDetail($id: ID!) {
    product(id: $id) {
      id
      title
      handle
      status
      vendor
      productType
      tags
      descriptionHtml
      templateSuffix
      seo {
        title
        description
      }
    }
  }
`;

const deletedProductLookupQuery = `#graphql
  query ProductMutationConformanceDeletedLookup($id: ID!, $query: String!) {
    product(id: $id) {
      id
      title
    }
    products(first: 5, query: $query) {
      edges {
        node {
          id
          title
          status
        }
      }
    }
  }
`;

const createMutation = `#graphql
  mutation ProductCreateConformance($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        vendor
        productType
        tags
        descriptionHtml
        templateSuffix
        seo {
          title
          description
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const updateMutation = `#graphql
  mutation ProductUpdateConformance($product: ProductUpdateInput!) {
    productUpdate(product: $product) {
      product {
        id
        title
        handle
        status
        vendor
        productType
        tags
        descriptionHtml
        templateSuffix
        seo {
          title
          description
        }
        onlineStorePreviewUrl
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const updateMissingIdMutation = `#graphql
  mutation ProductUpdateConformanceMissingId($product: ProductUpdateInput!) {
    productUpdate(product: $product) {
      product {
        id
        title
        handle
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation ProductDeleteConformance($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function buildCreateVariables(runId) {
  return {
    product: {
      title: `Hermes Product Conformance ${runId}`,
      status: 'DRAFT',
      vendor: 'HERMES',
      productType: 'ACCESSORIES',
      tags: ['conformance', 'product-mutation', runId],
      descriptionHtml: `<p>Hermes product mutation conformance ${runId}</p>`,
      templateSuffix: 'product-mutation-parity',
      seo: {
        title: `Hermes Product ${runId}`,
        description: `Hermes product mutation conformance ${runId}`,
      },
    },
  };
}

function buildUpdateVariables(productId, runId) {
  return {
    product: {
      id: productId,
      title: `Hermes Product Conformance ${runId} Updated`,
      vendor: 'HERMES-LABS',
      productType: 'TEST-GOODS',
      tags: ['conformance', 'product-mutation', `${runId}-updated`],
      descriptionHtml: `<p>Updated Hermes product mutation conformance ${runId}</p>`,
      templateSuffix: 'product-mutation-updated',
      seo: {
        title: `Hermes Product ${runId} Updated`,
        description: `Updated Hermes product mutation conformance ${runId}`,
      },
    },
  };
}

function buildCreateValidationVariables() {
  return {
    product: {
      title: '',
    },
  };
}

function buildUpdateValidationVariables() {
  return {
    product: {
      id: 'gid://shopify/Product/999999999999999',
      title: 'Ghost Product',
    },
  };
}

function buildUpdateMissingIdValidationVariables() {
  return {
    product: {
      title: 'Ghost Product Missing Id',
    },
  };
}

function buildUpdateBlankTitleValidationVariables(productId) {
  return {
    product: {
      id: productId,
      title: '',
    },
  };
}

function buildDeleteValidationVariables() {
  return {
    input: {
      id: 'gid://shopify/Product/999999999999999',
    },
  };
}

function buildDeleteMissingIdValidationVariables() {
  return {
    input: {},
  };
}

const deleteInlineMissingIdMutation = `#graphql
  mutation {
    productDelete(input: {}) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteInlineNullIdMutation = `#graphql
  mutation {
    productDelete(input: { id: null }) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function buildCreateHandleCollisionVariables(handle, runId) {
  return {
    product: {
      title: `Hermes Product Handle Collision ${runId}`,
      handle,
    },
  };
}

function buildCreateHandleNormalizationVariables(runId) {
  return {
    product: {
      title: `Normalized Handle Probe ${runId}`,
      handle: '  Weird Handle / 100%  ',
    },
  };
}

function buildCreateWhitespaceHandleVariables(runId) {
  return {
    product: {
      title: `Whitespace Handle Probe ${runId}`,
      handle: '   ',
    },
  };
}

function buildCreatePunctuationHandleNormalizationVariables(runId) {
  return {
    product: {
      title: `Hermes Product Handle Punctuation ${runId}`,
      handle: '%%%',
    },
  };
}

function buildUpdateHandleCollisionSeedVariables(runId) {
  return {
    product: {
      title: `Hermes Product Handle Challenger ${runId}`,
      status: 'DRAFT',
    },
  };
}

function buildUpdateHandleNormalizationVariables(productId) {
  return {
    product: {
      id: productId,
      handle: '  Mixed CASE/ Weird 200 % ',
    },
  };
}

function buildUpdateWhitespaceHandleVariables(productId) {
  return {
    product: {
      id: productId,
      handle: '   ',
    },
  };
}

function buildUpdatePunctuationHandleNormalizationVariables(productId) {
  return {
    product: {
      id: productId,
      handle: '%%%',
    },
  };
}

function buildUpdateHandleCollisionVariables(productId, handle) {
  return {
    product: {
      id: productId,
      handle,
    },
  };
}

function buildUpdateTitleOnlyVariables(productId, runId) {
  return {
    product: {
      id: productId,
      title: `Hermes Title Only Handle Probe ${runId} Updated`,
    },
  };
}

function buildUpdateTitleOnlySeedVariables(runId) {
  return {
    product: {
      title: `Hermes Title Only Handle Probe ${runId}`,
      handle: `title-only-handle-probe-${runId}`,
      status: 'DRAFT',
    },
  };
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Product mutation conformance blocker',
    whatFailed:
      'Attempted to capture live conformance for the staged product mutation family (`productCreate`, `productUpdate`, `productDelete`).',
    operations: ['productCreate', 'productUpdate', 'productDelete'],
    blocker,
    whyBlocked:
      'Without a write-capable token, the repo cannot capture successful live mutation payload shape, userErrors behavior for safe writes, or immediate downstream read-after-write parity for `productCreate`, `productUpdate`, and `productDelete`.',
    completedSteps: [
      'added a reusable live-write capture harness for the staged create/update/delete family',
      'kept the rich create/update payload slice aligned with the existing parity-request scaffolds so a future write-capable token can capture the same shapes directly',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with `write_products`, then rerun `corepack pnpm conformance:capture-product-mutations`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const createVariables = buildCreateVariables(runId);
let createdProductId = null;
let createNormalizationProductId = null;
let whitespaceHandleSeedProductId = null;
let createPunctuationNormalizationProductId = null;
let updateCollisionSeedProductId = null;
let updateTitleOnlySeedProductId = null;
let createResponse = null;
let updateResponse = null;
let deleteResponse = null;

try {
  const createValidationVariables = buildCreateValidationVariables();
  const createValidationResponse = await runGraphql(createMutation, createValidationVariables);

  createResponse = await runGraphql(createMutation, createVariables);
  createdProductId = createResponse.data?.productCreate?.product?.id ?? null;
  if (!createdProductId) {
    throw new Error('Product create capture did not return a product id.');
  }

  const createdHandle = createResponse.data?.productCreate?.product?.handle ?? null;
  if (typeof createdHandle !== 'string' || createdHandle.length === 0) {
    throw new Error('Product create capture did not return a handle.');
  }

  const createHandleNormalizationVariables = buildCreateHandleNormalizationVariables(runId);
  const createHandleNormalizationResponse = await runGraphql(createMutation, createHandleNormalizationVariables);
  createNormalizationProductId = createHandleNormalizationResponse.data?.productCreate?.product?.id ?? null;
  if (!createNormalizationProductId) {
    throw new Error('Product create handle normalization capture did not return a product id.');
  }

  const createWhitespaceHandleVariables = buildCreateWhitespaceHandleVariables(runId);
  const createWhitespaceHandleResponse = await runGraphql(createMutation, createWhitespaceHandleVariables);
  whitespaceHandleSeedProductId = createWhitespaceHandleResponse.data?.productCreate?.product?.id ?? null;
  if (!whitespaceHandleSeedProductId) {
    throw new Error('Product create whitespace-handle capture did not return a product id.');
  }

  const createPunctuationHandleNormalizationVariables = buildCreatePunctuationHandleNormalizationVariables(runId);
  const createPunctuationHandleNormalizationResponse = await runGraphql(
    createMutation,
    createPunctuationHandleNormalizationVariables,
  );
  createPunctuationNormalizationProductId =
    createPunctuationHandleNormalizationResponse.data?.productCreate?.product?.id ?? null;
  if (!createPunctuationNormalizationProductId) {
    throw new Error('Product create punctuation-handle normalization capture did not return a product id.');
  }
  await runGraphql(deleteMutation, { input: { id: createPunctuationNormalizationProductId } });
  createPunctuationNormalizationProductId = null;

  const createHandleCollisionVariables = buildCreateHandleCollisionVariables(createdHandle, runId);
  const createHandleCollisionResponse = await runGraphql(createMutation, createHandleCollisionVariables);
  const postCreateDetail = await runGraphql(productDetailQuery, { id: createdProductId });

  const updateCollisionSeedVariables = buildUpdateHandleCollisionSeedVariables(runId);
  const updateCollisionSeedResponse = await runGraphql(createMutation, updateCollisionSeedVariables);
  updateCollisionSeedProductId = updateCollisionSeedResponse.data?.productCreate?.product?.id ?? null;
  if (!updateCollisionSeedProductId) {
    throw new Error('Product update handle collision seed create did not return a product id.');
  }

  const updateTitleOnlySeedVariables = buildUpdateTitleOnlySeedVariables(runId);
  const updateTitleOnlySeedResponse = await runGraphql(createMutation, updateTitleOnlySeedVariables);
  updateTitleOnlySeedProductId = updateTitleOnlySeedResponse.data?.productCreate?.product?.id ?? null;
  if (!updateTitleOnlySeedProductId) {
    throw new Error('Product update title-only handle seed create did not return a product id.');
  }

  const updateVariables = buildUpdateVariables(createdProductId, runId);
  const updateValidationVariables = buildUpdateValidationVariables();
  const updateValidationResponse = await runGraphql(updateMutation, updateValidationVariables);
  const updateMissingIdValidationVariables = buildUpdateMissingIdValidationVariables();
  const updateMissingIdValidationResponse = await runGraphql(
    updateMissingIdMutation,
    updateMissingIdValidationVariables,
  );
  const updateBlankTitleValidationVariables = buildUpdateBlankTitleValidationVariables(createdProductId);
  const updateBlankTitleValidationResponse = await runGraphql(updateMutation, updateBlankTitleValidationVariables);
  const updateHandleNormalizationVariables = buildUpdateHandleNormalizationVariables(updateCollisionSeedProductId);
  const updateHandleNormalizationResponse = await runGraphql(updateMutation, updateHandleNormalizationVariables);
  const updateWhitespaceHandleVariables = buildUpdateWhitespaceHandleVariables(whitespaceHandleSeedProductId);
  const updateWhitespaceHandleResponse = await runGraphql(updateMutation, updateWhitespaceHandleVariables);
  const updatePunctuationHandleNormalizationVariables =
    buildUpdatePunctuationHandleNormalizationVariables(updateCollisionSeedProductId);
  const updatePunctuationHandleNormalizationResponse = await runGraphql(
    updateMutation,
    updatePunctuationHandleNormalizationVariables,
  );
  const updateHandleCollisionVariables = buildUpdateHandleCollisionVariables(
    updateCollisionSeedProductId,
    createdHandle,
  );
  const updateHandleCollisionResponse = await runGraphql(updateMutation, updateHandleCollisionVariables);
  const updateTitleOnlyVariables = buildUpdateTitleOnlyVariables(updateTitleOnlySeedProductId, runId);
  const updateTitleOnlyResponse = await runGraphql(updateMutation, updateTitleOnlyVariables);
  updateResponse = await runGraphql(updateMutation, updateVariables);
  const postUpdateDetail = await runGraphql(productDetailQuery, { id: createdProductId });
  const deleteValidationVariables = buildDeleteValidationVariables();
  const deleteValidationResponse = await runGraphql(deleteMutation, deleteValidationVariables);
  const deleteMissingIdValidationVariables = buildDeleteMissingIdValidationVariables();
  const deleteMissingIdValidationResponse = await runGraphqlAllowGraphqlErrors(
    deleteMutation,
    deleteMissingIdValidationVariables,
  );
  const deleteInlineMissingIdValidationResponse = await runGraphqlAllowGraphqlErrors(deleteInlineMissingIdMutation);
  const deleteInlineNullIdValidationResponse = await runGraphqlAllowGraphqlErrors(deleteInlineNullIdMutation);
  deleteResponse = await runGraphql(deleteMutation, { input: { id: createdProductId } });
  const postDeleteLookup = await runGraphql(deletedProductLookupQuery, {
    id: createdProductId,
    query: `title:${JSON.stringify(createVariables.product.title).slice(1, -1)}`,
  });
  createdProductId = null;

  const captures = {
    'product-create-parity.json': {
      mutation: {
        variables: createVariables,
        response: createResponse,
      },
      validation: {
        variables: createValidationVariables,
        response: createValidationResponse,
      },
      handleValidation: {
        createNormalization: {
          variables: createHandleNormalizationVariables,
          response: createHandleNormalizationResponse,
        },
        createWhitespaceNormalization: {
          variables: createWhitespaceHandleVariables,
          response: createWhitespaceHandleResponse,
        },
        createPunctuationNormalization: {
          variables: createPunctuationHandleNormalizationVariables,
          response: createPunctuationHandleNormalizationResponse,
        },
        createCollision: {
          variables: createHandleCollisionVariables,
          response: createHandleCollisionResponse,
        },
      },
      downstreamRead: postCreateDetail,
    },
    'product-update-parity.json': {
      mutation: {
        variables: updateVariables,
        response: updateResponse,
      },
      validation: {
        unknownId: {
          variables: updateValidationVariables,
          response: updateValidationResponse,
        },
        missingId: {
          variables: updateMissingIdValidationVariables,
          response: updateMissingIdValidationResponse,
        },
        blankTitle: {
          variables: updateBlankTitleValidationVariables,
          response: updateBlankTitleValidationResponse,
        },
      },
      handleValidation: {
        updateNormalization: {
          variables: updateHandleNormalizationVariables,
          response: updateHandleNormalizationResponse,
        },
        updateWhitespacePreservesHandle: {
          variables: updateWhitespaceHandleVariables,
          response: updateWhitespaceHandleResponse,
        },
        updatePunctuationNormalization: {
          variables: updatePunctuationHandleNormalizationVariables,
          response: updatePunctuationHandleNormalizationResponse,
        },
        updateCollision: {
          variables: updateHandleCollisionVariables,
          response: updateHandleCollisionResponse,
        },
        updateTitleOnlyPreservesHandle: {
          variables: updateTitleOnlyVariables,
          response: updateTitleOnlyResponse,
        },
      },
      downstreamRead: postUpdateDetail,
    },
    'product-delete-parity.json': {
      mutation: {
        variables: { input: { id: deleteResponse.data?.productDelete?.deletedProductId ?? null } },
        response: deleteResponse,
      },
      validation: {
        variables: deleteValidationVariables,
        response: deleteValidationResponse,
      },
      invalidInput: {
        variableMissingId: {
          variables: deleteMissingIdValidationVariables,
          response: deleteMissingIdValidationResponse,
        },
        inlineMissingId: {
          response: deleteInlineMissingIdValidationResponse,
        },
        inlineNullId: {
          response: deleteInlineNullIdValidationResponse,
        },
      },
      downstreamRead: postDeleteLookup,
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
        productId: deleteResponse.data?.productDelete?.deletedProductId ?? null,
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
  if (updateCollisionSeedProductId) {
    try {
      await runGraphql(deleteMutation, { input: { id: updateCollisionSeedProductId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }

  if (updateTitleOnlySeedProductId) {
    try {
      await runGraphql(deleteMutation, { input: { id: updateTitleOnlySeedProductId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }

  if (createNormalizationProductId) {
    try {
      await runGraphql(deleteMutation, { input: { id: createNormalizationProductId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }

  if (whitespaceHandleSeedProductId) {
    try {
      await runGraphql(deleteMutation, { input: { id: whitespaceHandleSeedProductId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }

  if (createPunctuationNormalizationProductId) {
    try {
      await runGraphql(deleteMutation, { input: { id: createPunctuationNormalizationProductId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }

  if (createdProductId) {
    try {
      await runGraphql(deleteMutation, { input: { id: createdProductId } });
    } catch {
      // Best-effort cleanup only. The conformance script should still surface the original failure.
    }
  }
}
