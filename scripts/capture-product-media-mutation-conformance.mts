// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

import { parseWriteScopeBlocker, renderWriteScopeBlockerNote } from './product-mutation-conformance-lib.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const pendingDir = 'pending';
const blockerPath = path.join(pendingDir, 'product-media-mutation-conformance-scope-blocker.md');
const { runGraphql, runGraphqlRequest, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});
const productsHydrateNodesObservationQuery = await readFile(
  'config/parity-requests/products/products-hydrate-nodes-observation.graphql',
  'utf8',
);
const productMediaValidationDownstreamReadQuery = await readFile(
  'config/parity-requests/products/product-media-validation-downstream-read.graphql',
  'utf8',
);

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function expectNoUserErrors(pathLabel, userErrors) {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${pathLabel} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

const createProductMutation = `#graphql
  mutation ProductMediaConformanceCreateProduct($product: ProductCreateInput!) {
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
  mutation ProductMediaConformanceDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const createMediaMutation = `#graphql
  mutation ProductCreateMediaParityPlan($productId: ID!, $media: [CreateMediaInput!]!) {
    productCreateMedia(productId: $productId, media: $media) {
      media {
        id
        alt
        mediaContentType
        status
        preview {
          image {
            url
          }
        }
        ... on MediaImage {
          image {
            url
          }
        }
      }
      mediaUserErrors {
        field
        message
        code
      }
      product {
        id
        media(first: 10) {
          nodes {
            id
            alt
            mediaContentType
            status
            preview {
              image {
                url
              }
            }
            ... on MediaImage {
              image {
                url
              }
            }
          }
        }
      }
    }
  }
`;

const createMediaDualUserErrorsMutation = `#graphql
  mutation ProductCreateMediaDualUserErrors($productId: ID!, $media: [CreateMediaInput!]!) {
    productCreateMedia(productId: $productId, media: $media) {
      media {
        id
        alt
        mediaContentType
        status
      }
      userErrors {
        field
        message
      }
      mediaUserErrors {
        field
        message
        code
      }
    }
  }
`;

const updateMediaMutation = `#graphql
  mutation ProductUpdateMediaParityPlan($productId: ID!, $media: [UpdateMediaInput!]!) {
    productUpdateMedia(productId: $productId, media: $media) {
      media {
        id
        alt
        mediaContentType
        status
        preview {
          image {
            url
          }
        }
        ... on MediaImage {
          image {
            url
          }
        }
      }
      mediaUserErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteMediaMutation = `#graphql
  mutation ProductDeleteMediaParityPlan($productId: ID!, $mediaIds: [ID!]!) {
    productDeleteMedia(productId: $productId, mediaIds: $mediaIds) {
      deletedMediaIds
      deletedProductImageIds
      mediaUserErrors {
        field
        message
        code
      }
      product {
        id
        media(first: 10) {
          nodes {
            id
            alt
            mediaContentType
            status
            preview {
              image {
                url
              }
            }
            ... on MediaImage {
              image {
                url
              }
            }
          }
        }
      }
    }
  }
`;

const createMediaValidationMutation = `#graphql
  mutation ProductCreateMediaValidationBranches($productId: ID!, $media: [CreateMediaInput!]!) {
    productCreateMedia(productId: $productId, media: $media) {
      media {
        id
        alt
        mediaContentType
        status
      }
      userErrors {
        field
        message
      }
      mediaUserErrors {
        field
        message
        code
      }
      product {
        id
        media(first: 10) {
          nodes {
            id
            alt
            mediaContentType
            status
          }
        }
      }
    }
  }
`;

const updateMediaValidationMutation = `#graphql
  mutation ProductUpdateMediaValidationBranches($productId: ID!, $media: [UpdateMediaInput!]!) {
    productUpdateMedia(productId: $productId, media: $media) {
      media {
        id
        alt
        mediaContentType
        status
      }
      userErrors {
        field
        message
      }
      mediaUserErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteMediaValidationMutation = `#graphql
  mutation ProductDeleteMediaValidationBranches($productId: ID!, $mediaIds: [ID!]!) {
    productDeleteMedia(productId: $productId, mediaIds: $mediaIds) {
      deletedMediaIds
      deletedProductImageIds
      userErrors {
        field
        message
      }
      mediaUserErrors {
        field
        message
        code
      }
    }
  }
`;

const reorderMediaValidationMutation = `#graphql
  mutation ProductReorderMediaValidationBranches($id: ID!, $moves: [MoveInput!]!) {
    productReorderMedia(id: $id, moves: $moves) {
      job {
        id
        done
      }
      userErrors {
        field
        message
      }
      mediaUserErrors {
        field
        message
        code
      }
    }
  }
`;

const mediaReadQuery = `#graphql
  query ProductMediaDownstream($id: ID!) {
    product(id: $id) {
      id
      media(first: 10) {
        nodes {
          id
          alt
          mediaContentType
          status
          preview {
            image {
              url
            }
          }
          ... on MediaImage {
            image {
              url
            }
          }
        }
      }
      images(first: 10) {
        nodes {
          id
          url
          altText
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      featuredImage {
        id
        url
        altText
      }
      featuredMedia {
        __typename
        ... on MediaImage {
          id
          alt
          mediaContentType
          status
          image {
            id
            url
            altText
            width
            height
          }
          preview {
            image {
              url
              width
              height
            }
          }
        }
      }
    }
  }
`;

const productHydrateNodesQuery = `#graphql
  query ProductsHydrateNodes($ids: [ID!]!) {
    nodes(ids: $ids) {
      __typename
      id
      ... on Product {
        title
        handle
        status
        media(first: 250) {
          nodes {
            __typename
            id
            alt
            mediaContentType
            status
            preview {
              image {
                url
                width
                height
              }
            }
            ... on MediaImage {
              image {
                id
                url
                altText
                width
                height
              }
            }
          }
        }
      }
    }
  }
`;

function buildCreateProductVariables(runId) {
  return {
    product: {
      title: `Hermes Product Media Conformance ${runId}`,
      status: 'DRAFT',
    },
  };
}

function buildInvalidCreateMediaVariables(productId) {
  return {
    productId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'not-a-url',
        alt: 'Invalid source',
      },
    ],
  };
}

function buildCreateMediaVariables(productId) {
  return {
    productId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://placehold.co/600x400/png',
        alt: 'Front view',
      },
    ],
  };
}

async function recordValidationScenario(name, query, variables, downstreamProductId = null) {
  const response = await runGraphqlRaw(query, variables);
  const scenario = {
    name,
    variables,
    response,
  };
  if (downstreamProductId) {
    scenario.downstreamReadAfterScenario = await runGraphql(productMediaValidationDownstreamReadQuery, {
      productId: downstreamProductId,
    });
  }
  return scenario;
}

async function waitForReadyMedia(productId, mediaId) {
  for (let attempt = 0; attempt < 12; attempt += 1) {
    await sleep(5000);
    const payload = await runGraphql(mediaReadQuery, { id: productId });
    const node = payload.data?.product?.media?.nodes?.find((candidate) => candidate?.id === mediaId) ?? null;
    if (node?.status === 'READY') {
      return payload;
    }
  }

  throw new Error(`Timed out waiting for media ${mediaId} to become READY.`);
}

function expectSameUserErrors(pathLabel, userErrors, mediaUserErrors) {
  const stripCode = (errors) =>
    Array.isArray(errors)
      ? errors.map((error) => ({
          field: error?.field ?? null,
          message: error?.message ?? null,
        }))
      : errors;
  if (
    Array.isArray(userErrors) &&
    Array.isArray(mediaUserErrors) &&
    JSON.stringify(stripCode(userErrors)) === JSON.stringify(stripCode(mediaUserErrors))
  ) {
    return;
  }

  throw new Error(
    `${pathLabel} returned divergent userErrors/mediaUserErrors: ${JSON.stringify(
      { userErrors: userErrors ?? null, mediaUserErrors: mediaUserErrors ?? null },
      null,
      2,
    )}`,
  );
}

async function writeScopeBlocker(blocker) {
  await mkdir(pendingDir, { recursive: true });
  const note = renderWriteScopeBlockerNote({
    title: 'Product media mutation conformance blocker',
    whatFailed:
      'Attempted to capture live conformance for the staged product media mutation family (`productCreateMedia`, `productUpdateMedia`, `productDeleteMedia`).',
    operations: ['productCreateMedia', 'productUpdateMedia', 'productDeleteMedia'],
    blocker,
    whyBlocked:
      'Without a write-capable token, the repo cannot capture successful live media mutation payload shape, mediaUserErrors behavior, or immediate downstream `product.media` parity for this family.',
    completedSteps: [
      'added a reusable live-write capture harness for staged product media mutations',
      'aligned the proxy request scaffolds with the live media mutation slice used by the current runtime and parity specs',
    ],
    recommendedNextStep:
      'Switch the repo conformance credential to a safe dev-store token with product media write permissions, then rerun `corepack pnpm conformance:capture-product-media-mutations`.',
  });

  await writeFile(blockerPath, `${note}\n`, 'utf8');
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const createProductVariables = buildCreateProductVariables(runId);
let productId = null;
let mediaId = null;

try {
  const createProductResponse = await runGraphql(createProductMutation, createProductVariables);
  expectNoUserErrors('productCreate (media seed)', createProductResponse.data?.productCreate?.userErrors);
  productId = createProductResponse.data?.productCreate?.product?.id ?? null;
  if (!productId) {
    throw new Error('Product media capture did not return a product id.');
  }
  const initialProductHydrateResponse = await runGraphqlRequest(productsHydrateNodesObservationQuery, {
    ids: [productId],
  });
  if (initialProductHydrateResponse.status < 200 || initialProductHydrateResponse.status >= 300) {
    throw new Error(`Product media initial hydrate failed: ${JSON.stringify(initialProductHydrateResponse, null, 2)}`);
  }
  const initialHydratedProduct = initialProductHydrateResponse.payload?.data?.nodes?.[0] ?? null;

  const createMediaVariables = buildCreateMediaVariables(productId);
  const dualUserErrorsVariables = buildInvalidCreateMediaVariables(productId);
  const dualUserErrorsResponse = await runGraphql(createMediaDualUserErrorsMutation, dualUserErrorsVariables);
  expectSameUserErrors(
    'productCreateMedia dual userErrors',
    dualUserErrorsResponse.data?.productCreateMedia?.userErrors,
    dualUserErrorsResponse.data?.productCreateMedia?.mediaUserErrors,
  );
  const productHydrateResponse = await runGraphqlRequest(productHydrateNodesQuery, {
    ids: [productId],
  });
  if (productHydrateResponse.status < 200 || productHydrateResponse.status >= 300) {
    throw new Error(`Product media dual userErrors hydrate failed: ${JSON.stringify(productHydrateResponse)}`);
  }

  const createMediaResponse = await runGraphql(createMediaMutation, createMediaVariables);
  expectNoUserErrors('productCreateMedia', createMediaResponse.data?.productCreateMedia?.mediaUserErrors);
  mediaId = createMediaResponse.data?.productCreateMedia?.media?.[0]?.id ?? null;
  if (!mediaId) {
    throw new Error('Product media create capture did not return a media id.');
  }

  const postCreateRead = await runGraphql(mediaReadQuery, { id: productId });
  const readyRead = await waitForReadyMedia(productId, mediaId);
  const readyMediaNode =
    readyRead.data?.product?.media?.nodes?.find((candidate) => candidate?.id === mediaId) ??
    postCreateRead.data?.product?.media?.nodes?.find((candidate) => candidate?.id === mediaId) ??
    null;

  const missingProductId = 'gid://shopify/Product/999999999999';
  const missingMediaId = 'gid://shopify/MediaImage/999999999999';
  const scenarios = [];
  scenarios.push(
    await recordValidationScenario('create-missing-product-id-empty-string', createMediaValidationMutation, {
      productId: '',
      media: [
        {
          mediaContentType: 'IMAGE',
          originalSource: 'https://placehold.co/600x400/png',
          alt: 'Valid',
        },
      ],
    }),
  );
  scenarios.push(
    await recordValidationScenario('create-unknown-product-id', createMediaValidationMutation, {
      productId: missingProductId,
      media: [
        {
          mediaContentType: 'IMAGE',
          originalSource: 'https://placehold.co/600x400/png',
          alt: 'Valid',
        },
      ],
    }),
  );
  scenarios.push(
    await recordValidationScenario('create-empty-media', createMediaValidationMutation, {
      productId,
      media: [],
    }),
  );
  scenarios.push(
    await recordValidationScenario('create-invalid-original-source', createMediaValidationMutation, {
      productId,
      media: [
        {
          mediaContentType: 'IMAGE',
          originalSource: 'not-a-url',
          alt: 'Invalid source',
        },
      ],
    }),
  );
  scenarios.push(
    await recordValidationScenario('create-invalid-media-content-type', createMediaValidationMutation, {
      productId,
      media: [
        {
          mediaContentType: 'FILE',
          originalSource: 'https://placehold.co/600x400/png',
          alt: 'Invalid type',
        },
      ],
    }),
  );
  const createMixedScenario = await recordValidationScenario(
    'create-mixed-valid-invalid',
    createMediaValidationMutation,
    {
      productId,
      media: [
        {
          mediaContentType: 'IMAGE',
          originalSource: 'https://placehold.co/600x400/png',
          alt: 'Valid in mixed create',
        },
        {
          mediaContentType: 'IMAGE',
          originalSource: 'not-a-url',
          alt: 'Invalid in mixed create',
        },
      ],
    },
    productId,
  );
  scenarios.push(createMixedScenario);
  const mixedMediaId = createMixedScenario.response.payload?.data?.productCreateMedia?.media?.[0]?.id ?? null;
  if (mixedMediaId) {
    await waitForReadyMedia(productId, mixedMediaId);
  }
  scenarios.push(
    await recordValidationScenario('update-unknown-product-id', updateMediaValidationMutation, {
      productId: missingProductId,
      media: [{ id: mediaId, alt: 'Should not update' }],
    }),
  );
  scenarios.push(
    await recordValidationScenario('update-empty-media', updateMediaValidationMutation, {
      productId,
      media: [],
    }),
  );
  scenarios.push(
    await recordValidationScenario('update-unknown-media-id', updateMediaValidationMutation, {
      productId,
      media: [{ id: missingMediaId, alt: 'Unknown media' }],
    }),
  );
  scenarios.push(
    await recordValidationScenario(
      'update-mixed-valid-invalid',
      updateMediaValidationMutation,
      {
        productId,
        media: [
          { id: mediaId, alt: 'Rejected update' },
          { id: missingMediaId, alt: 'Unknown media' },
        ],
      },
      productId,
    ),
  );
  scenarios.push(
    await recordValidationScenario('delete-unknown-product-id', deleteMediaValidationMutation, {
      productId: missingProductId,
      mediaIds: [mediaId],
    }),
  );
  scenarios.push(
    await recordValidationScenario('delete-empty-media-ids', deleteMediaValidationMutation, {
      productId,
      mediaIds: [],
    }),
  );
  scenarios.push(
    await recordValidationScenario('delete-unknown-media-id', deleteMediaValidationMutation, {
      productId,
      mediaIds: [missingMediaId],
    }),
  );
  scenarios.push(
    await recordValidationScenario(
      'delete-mixed-valid-invalid',
      deleteMediaValidationMutation,
      {
        productId,
        mediaIds: [mediaId, missingMediaId],
      },
      productId,
    ),
  );
  scenarios.push(
    await recordValidationScenario('reorder-unknown-product-id', reorderMediaValidationMutation, {
      id: missingProductId,
      moves: [{ id: mediaId, newPosition: '0' }],
    }),
  );
  scenarios.push(
    await recordValidationScenario('reorder-unknown-media-id', reorderMediaValidationMutation, {
      id: productId,
      moves: [{ id: missingMediaId, newPosition: '0' }],
    }),
  );
  const validationHydrateResponse = await runGraphqlRequest(productHydrateNodesQuery, {
    ids: [productId],
  });
  const hydratedProduct = validationHydrateResponse.payload?.data?.nodes?.[0] ?? null;

  const updateMediaVariables = {
    productId,
    media: [{ id: mediaId, alt: 'Updated front view' }],
  };
  const updateMediaResponse = await runGraphql(updateMediaMutation, updateMediaVariables);
  expectNoUserErrors('productUpdateMedia', updateMediaResponse.data?.productUpdateMedia?.mediaUserErrors);
  const postUpdateRead = await runGraphql(mediaReadQuery, { id: productId });

  const deleteMediaVariables = {
    productId,
    mediaIds: [mediaId],
  };
  const deleteMediaResponse = await runGraphql(deleteMediaMutation, deleteMediaVariables);
  expectNoUserErrors('productDeleteMedia', deleteMediaResponse.data?.productDeleteMedia?.mediaUserErrors);
  const postDeleteRead = await runGraphql(mediaReadQuery, { id: productId });

  const captures = {
    'product-media-validation-branches.json': {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      operations: ['productCreateMedia', 'productUpdateMedia', 'productDeleteMedia', 'productReorderMedia'],
      seedProductMedia: [
        {
          productId,
          id: mediaId,
          position: 1,
          alt: readyMediaNode?.alt ?? 'Front view',
          mediaContentType: readyMediaNode?.mediaContentType ?? 'IMAGE',
          status: readyMediaNode?.status ?? 'READY',
          productImageId: readyMediaNode?.image?.id ?? mediaId,
        },
      ],
      scenarios,
      upstreamCalls: [
        {
          operationName: 'ProductsHydrateNodes',
          variables: {
            ids: [productId],
          },
          query: productsHydrateNodesObservationQuery,
          response: {
            status: validationHydrateResponse.status,
            body: {
              data: {
                nodes: [hydratedProduct],
              },
            },
          },
        },
      ],
      notes:
        'Creates a disposable product and seed media, records product media mutation validation branches including typed MediaUserError codes, then deletes the product during cleanup.',
    },
    'productCreateMedia-dual-userErrors.json': {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      setup: {
        productCreate: createProductResponse,
      },
      mutation: {
        variables: dualUserErrorsVariables,
        response: dualUserErrorsResponse,
      },
      upstreamCalls: [
        {
          operationName: 'ProductsHydrateNodes',
          variables: {
            ids: [productId],
          },
          query: productHydrateNodesQuery,
          response: {
            status: productHydrateResponse.status,
            body: productHydrateResponse.payload,
          },
        },
      ],
    },
    'product-create-media-parity.json': {
      mutation: {
        variables: createMediaVariables,
        response: createMediaResponse,
      },
      downstreamRead: postCreateRead,
      readyRead,
      upstreamCalls: [
        {
          operationName: 'ProductsHydrateNodes',
          variables: {
            ids: [productId],
          },
          query: productsHydrateNodesObservationQuery,
          response: {
            status: initialProductHydrateResponse.status,
            body: {
              data: {
                nodes: [initialHydratedProduct],
              },
            },
          },
        },
      ],
    },
    'product-update-media-parity.json': {
      setup: {
        createMedia: {
          variables: createMediaVariables,
          response: createMediaResponse,
        },
        readyRead,
      },
      mutation: {
        variables: updateMediaVariables,
        response: updateMediaResponse,
      },
      downstreamRead: postUpdateRead,
      upstreamCalls: [
        {
          operationName: 'ProductsHydrateNodes',
          variables: {
            ids: [productId],
          },
          query: productsHydrateNodesObservationQuery,
          response: {
            status: initialProductHydrateResponse.status,
            body: {
              data: {
                nodes: [initialHydratedProduct],
              },
            },
          },
        },
      ],
    },
    'product-delete-media-parity.json': {
      mutation: {
        variables: deleteMediaVariables,
        response: deleteMediaResponse,
      },
      downstreamRead: postDeleteRead,
    },
  };

  for (const [filename, payload] of Object.entries(captures)) {
    await writeFile(path.join(outputDir, filename), `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  }

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: Object.keys(captures),
        productId,
        mediaId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const blocker = parseWriteScopeBlocker(error?.result ?? null);
  if (blocker) {
    await writeScopeBlocker(blocker);
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
  if (productId && mediaId) {
    try {
      await runGraphql(deleteMediaMutation, { productId, mediaIds: [mediaId] });
    } catch {
      // Best-effort cleanup only.
    }
  }

  if (productId) {
    try {
      await runGraphql(deleteProductMutation, { input: { id: productId } });
    } catch {
      // Best-effort cleanup only.
    }
  }
}
