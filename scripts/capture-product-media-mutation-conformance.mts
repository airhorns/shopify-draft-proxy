// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
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
const blockerPath = path.join(pendingDir, 'product-media-mutation-conformance-scope-blocker.md');
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

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
  if (
    Array.isArray(userErrors) &&
    Array.isArray(mediaUserErrors) &&
    JSON.stringify(userErrors) === JSON.stringify(mediaUserErrors)
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
    },
    'product-update-media-parity.json': {
      mutation: {
        variables: updateMediaVariables,
        response: updateMediaResponse,
      },
      downstreamRead: postUpdateRead,
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
