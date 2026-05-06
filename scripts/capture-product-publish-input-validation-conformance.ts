/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'productPublish-input-validation.json');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type ProductCreateData = {
  productCreate: {
    product: { id: string; title: string; status: string } | null;
    userErrors: Array<{ field: string[] | null; message: string }>;
  };
};

type ProductDeleteData = {
  productDelete: {
    deletedProductId: string | null;
    userErrors: Array<{ field: string[] | null; message: string }>;
  };
};

type ProductPublishValidationVariables = {
  input: {
    id: string;
    productPublications?: Array<{
      channelId?: string;
      publicationId?: string;
    }>;
  };
};

type ValidationCase = {
  variables: ProductPublishValidationVariables;
  response: ConformanceGraphqlResult;
};

const createProductMutation = `#graphql
  mutation ProductPublishInputValidationCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteProductMutation = `#graphql
  mutation ProductPublishInputValidationDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const productPublishInputValidationMutation = `#graphql
  mutation ProductPublishInputValidation($input: ProductPublishInput!) {
    productPublish(input: $input) {
      product {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productsHydrateNodesQuery = `#graphql
  query ProductsHydrateNodes($ids: [ID!]!) {
    nodes(ids: $ids) {
      __typename
      id
      ... on Product {
        legacyResourceId
        title
        handle
        status
        vendor
        productType
        tags
        totalInventory
        tracksInventory
        createdAt
        updatedAt
        publishedAt
        descriptionHtml
        onlineStorePreviewUrl
        templateSuffix
        seo {
          title
          description
        }
      }
    }
  }
`;

async function captureCase(variables: ProductPublishValidationVariables): Promise<ValidationCase> {
  return {
    variables,
    response: await runGraphqlRaw(productPublishInputValidationMutation, variables),
  };
}

await mkdir(outputDir, { recursive: true });

const runId = Date.now().toString(36);
let productId: string | null = null;
let setup: ConformanceGraphqlPayload<ProductCreateData> | null = null;
let cleanup: ConformanceGraphqlResult<ProductDeleteData> | null = null;
let hydrateResponse: ConformanceGraphqlResult | null = null;
const cases: Record<string, ValidationCase> = {};

try {
  setup = await runGraphql<ProductCreateData>(createProductMutation, {
    product: {
      title: `Product publish input validation ${runId}`,
      status: 'DRAFT',
    },
  });
  productId = setup.data?.productCreate.product?.id ?? null;
  if (!productId) {
    throw new Error(`Product setup failed: ${JSON.stringify(setup)}`);
  }

  cases['missingProductPublications'] = await captureCase({
    input: { id: productId },
  });
  cases['emptyProductPublications'] = await captureCase({
    input: { id: productId, productPublications: [] },
  });
  cases['unknownPublication'] = await captureCase({
    input: {
      id: productId,
      productPublications: [{ publicationId: 'gid://shopify/Publication/999999999999' }],
    },
  });
  cases['unknownChannel'] = await captureCase({
    input: {
      id: productId,
      productPublications: [{ channelId: 'gid://shopify/Channel/999999999999' }],
    },
  });

  hydrateResponse = await runGraphqlRaw(productsHydrateNodesQuery, {
    ids: [productId],
  });
} finally {
  if (productId) {
    try {
      cleanup = await runGraphqlRaw<ProductDeleteData>(deleteProductMutation, {
        input: { id: productId },
      });
    } catch (error) {
      console.warn(
        JSON.stringify(
          {
            ok: false,
            cleanup: 'productDelete',
            productId,
            error: error instanceof Error ? error.message : String(error),
          },
          null,
          2,
        ),
      );
    }
  }
}

if (!productId || !setup || !hydrateResponse || Object.keys(cases).length === 0) {
  throw new Error('productPublish input validation capture did not produce required setup/cases.');
}

await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'productPublish-input-validation',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup: {
        product: setup.data?.productCreate.product ?? null,
      },
      cases,
      cleanup,
      notes: [
        'Live Admin API accepted empty productPublications arrays with userErrors: [] for productPublish on this store/API version.',
        'Live Admin API returned top-level INVALID_VARIABLE for omitted variable productPublications.',
        'Live Admin API exposes UserError field/message only on ProductPublishPayload.userErrors for this store/API version.',
      ],
      upstreamCalls: [
        {
          operationName: 'ProductsHydrateNodes',
          variables: { ids: [productId] },
          query: productsHydrateNodesQuery,
          response: {
            status: hydrateResponse.status,
            body: hydrateResponse.payload,
          },
        },
      ],
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
      caseCount: Object.keys(cases).length,
      cleanupDeletedProductId: cleanup?.payload.data?.productDelete.deletedProductId ?? null,
    },
    null,
    2,
  ),
);
