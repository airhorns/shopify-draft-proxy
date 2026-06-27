import 'dotenv/config';

/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type ProductCreateData = {
  productCreate?: {
    product?: {
      id?: string | null;
    } | null;
    userErrors?: unknown;
  } | null;
};

type ProductCreateMediaData = {
  productCreateMedia?: {
    media?: Array<{
      id?: string | null;
    } | null> | null;
    mediaUserErrors?: unknown;
  } | null;
};

type ProductMediaHydrateData = {
  nodes?: Array<Record<string, unknown> | null> | null;
};

const requiredApiVersion = '2026-04';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
if (apiVersion !== requiredApiVersion) {
  throw new Error(
    `product-media-missing-media-aggregation capture requires SHOPIFY_CONFORMANCE_API_VERSION=${requiredApiVersion}, got ${apiVersion}`,
  );
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'product-media-missing-media-aggregation.json');
const productsHydrateNodesObservationQuery = await readFile(
  'config/parity-requests/products/products-hydrate-nodes-observation.graphql',
  'utf8',
);
const updateMediaAggregationMutation = await readFile(
  'config/parity-requests/products/productUpdateMedia-missing-media-aggregation.graphql',
  'utf8',
);
const deleteMediaAggregationMutation = await readFile(
  'config/parity-requests/products/productDeleteMedia-missing-media-aggregation.graphql',
  'utf8',
);

const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createProductMutation = `#graphql
  mutation ProductMediaMissingAggregationCreateProduct($product: ProductCreateInput!) {
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

const createMediaMutation = `#graphql
  mutation ProductMediaMissingAggregationCreateMedia($productId: ID!, $media: [CreateMediaInput!]!) {
    productCreateMedia(productId: $productId, media: $media) {
      media {
        id
        alt
        mediaContentType
        status
      }
      mediaUserErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteProductMutation = `#graphql
  mutation ProductMediaMissingAggregationDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

function expectNoUserErrors(label: string, errors: unknown): void {
  if (Array.isArray(errors) && errors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(errors ?? null, null, 2)}`);
}

async function runCapturedMutation(
  query: string,
  variables: Record<string, unknown>,
): Promise<{
  variables: Record<string, unknown>;
  response: {
    status: number;
    payload: ConformanceGraphqlPayload;
  };
}> {
  return {
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

const runId = `${Date.now()}`;
const createProductVariables = {
  product: {
    title: `Hermes Product Media Missing Aggregation ${runId}`,
    status: 'DRAFT',
  },
};
let productId: string | null = null;

try {
  const createProductResponse = await runGraphql<ProductCreateData>(createProductMutation, createProductVariables);
  expectNoUserErrors('productCreate', createProductResponse.data?.productCreate?.userErrors);
  productId = createProductResponse.data?.productCreate?.product?.id ?? null;
  if (!productId) {
    throw new Error('Product media missing aggregation capture did not return a product id.');
  }

  const createMediaVariables = {
    productId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://placehold.co/600x400/png',
        alt: 'Ready media',
      },
    ],
  };
  const createMediaResponse = await runGraphql<ProductCreateMediaData>(createMediaMutation, createMediaVariables);
  expectNoUserErrors('productCreateMedia', createMediaResponse.data?.productCreateMedia?.mediaUserErrors);
  const mediaId = createMediaResponse.data?.productCreateMedia?.media?.[0]?.id ?? null;
  if (!mediaId) {
    throw new Error('Product media missing aggregation capture did not return a media id.');
  }

  const hydrateResponse = await runGraphqlRequest<ProductMediaHydrateData>(productsHydrateNodesObservationQuery, {
    ids: [productId],
  });
  const hydratedProduct = hydrateResponse.payload.data?.nodes?.[0] ?? null;
  if (!hydratedProduct) {
    throw new Error('Product media missing aggregation hydrate did not return the product node.');
  }

  const missingMediaIds = ['gid://shopify/MediaImage/999999999998', 'gid://shopify/MediaImage/999999999999'];
  const updateVariables = {
    productId,
    media: [
      { id: missingMediaIds[0], alt: 'Missing one' },
      { id: missingMediaIds[1], alt: 'Missing two' },
    ],
  };
  const deleteVariables = {
    productId,
    mediaIds: missingMediaIds,
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        operations: ['productUpdateMedia', 'productDeleteMedia'],
        setup: {
          productCreate: {
            variables: createProductVariables,
            response: createProductResponse,
          },
          productCreateMedia: {
            variables: createMediaVariables,
            response: createMediaResponse,
          },
        },
        cases: {
          updateMultiMissing: await runCapturedMutation(updateMediaAggregationMutation, updateVariables),
          deleteMultiMissing: await runCapturedMutation(deleteMediaAggregationMutation, deleteVariables),
        },
        upstreamCalls: [
          {
            operationName: 'ProductsHydrateNodes',
            variables: {
              ids: [productId],
            },
            query: productsHydrateNodesObservationQuery,
            response: {
              status: hydrateResponse.status,
              body: {
                data: {
                  nodes: [hydratedProduct],
                },
              },
            },
          },
        ],
        notes:
          'Creates a disposable product plus one product media node, then records productUpdateMedia/productDeleteMedia requests with two nonexistent MediaImage GIDs to capture Shopify aggregated MEDIA_DOES_NOT_EXIST messages.',
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        productId,
        mediaId,
      },
      null,
      2,
    ),
  );
} finally {
  if (productId) {
    try {
      await runGraphql(deleteProductMutation, { input: { id: productId } });
    } catch {
      // Best-effort cleanup only.
    }
  }
}
