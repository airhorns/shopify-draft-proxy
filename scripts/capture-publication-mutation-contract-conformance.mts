/* oxlint-disable no-console -- CLI capture scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

type CaptureCase = {
  query: string;
  variables: JsonObject;
  response: ConformanceGraphqlResult;
};

type ProductSeed = {
  id: string;
  title: string;
  handle?: string | null;
  status?: string | null;
  createdAt?: string | null;
  updatedAt?: string | null;
  variants?: {
    nodes?: Array<{
      id: string;
      title?: string | null;
    }>;
  };
};

type ProductCreateData = {
  productCreate?: {
    product?: ProductSeed | null;
    userErrors?: Array<{ field?: string[] | null; message: string }>;
  } | null;
};

type ProductDeleteData = {
  productDelete?: {
    deletedProductId?: string | null;
    userErrors?: Array<{ field?: string[] | null; message: string }>;
  } | null;
};

type PublicationCreateData = {
  publicationCreate?: {
    publication?: { id: string } | null;
    userErrors?: Array<{ field?: string[] | null; message: string; code?: string | null }>;
  } | null;
};

type PublicationDeleteData = {
  publicationDelete?: {
    deletedId?: string | null;
    userErrors?: Array<{ field?: string[] | null; message: string; code?: string | null }>;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'publication-mutation-contract.json');
const publicationCreateDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'publicationCreate-validation.graphql',
);
const publicationUpdateDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'publicationUpdate-contract.graphql',
);
const publicationDeleteDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'publicationDelete-contract.graphql',
);

const publicationCreateMutation = await readFile(publicationCreateDocumentPath, 'utf8');
const publicationUpdateMutation = await readFile(publicationUpdateDocumentPath, 'utf8');
const publicationDeleteMutation = await readFile(publicationDeleteDocumentPath, 'utf8');

const createProductMutation = `#graphql
  mutation PublicationContractProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
        createdAt
        updatedAt
        variants(first: 1) {
          nodes {
            id
            title
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

const deleteProductMutation = `#graphql
  mutation PublicationContractProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

async function captureCase(query: string, variables: JsonObject): Promise<CaptureCase> {
  return {
    query,
    variables,
    response: await runGraphqlRaw(query, variables),
  };
}

function getCreatedProduct(result: ConformanceGraphqlResult<ProductCreateData>): ProductSeed | null {
  return result.payload.data?.productCreate?.product ?? null;
}

function getCreatedPublicationId(result: ConformanceGraphqlResult<PublicationCreateData>): string | null {
  return result.payload.data?.publicationCreate?.publication?.id ?? null;
}

await mkdir(outputDir, { recursive: true });

const runId = Date.now().toString(36);
let product: ProductSeed | null = null;
let publicationId: string | null = null;
let productCleanup: ConformanceGraphqlResult<ProductDeleteData> | null = null;
let publicationCleanup: ConformanceGraphqlResult<PublicationDeleteData> | null = null;
let deleteCreated: CaptureCase | null = null;
const cases: Record<string, CaptureCase> = {};

try {
  const productCreate = await runGraphqlRaw<ProductCreateData>(createProductMutation, {
    product: {
      title: `Publication mutation contract ${runId}`,
      status: 'DRAFT',
    },
  });
  product = getCreatedProduct(productCreate);
  if (!product?.id) {
    throw new Error(
      `publication mutation contract capture could not create a seed product: ${JSON.stringify(productCreate)}`,
    );
  }
  cases['productCreate'] = {
    query: createProductMutation,
    variables: {
      product: {
        title: `Publication mutation contract ${runId}`,
        status: 'DRAFT',
      },
    },
    response: productCreate,
  };

  const createOmittedCatalog = await captureCase(publicationCreateMutation, { input: {} });
  cases['createOmittedCatalog'] = createOmittedCatalog;
  publicationId = getCreatedPublicationId(
    createOmittedCatalog.response as ConformanceGraphqlResult<PublicationCreateData>,
  );
  if (!publicationId) {
    throw new Error(
      `publicationCreate(input: {}) did not return a publication id: ${JSON.stringify(createOmittedCatalog)}`,
    );
  }

  cases['createUnknownCatalog'] = await captureCase(publicationCreateMutation, {
    input: { catalogId: 'gid://shopify/Catalog/999999999999' },
  });
  cases['createInvalidDefaultState'] = await captureCase(publicationCreateMutation, {
    input: { defaultState: 'NOT_A_PUBLICATION_STATE' },
  });
  cases['createUnknownInputFields'] = await captureCase(publicationCreateMutation, {
    input: {
      name: 'Not a PublicationCreateInput field',
      channelId: 'gid://shopify/Channel/999999999999',
    },
  });

  cases['updateAddProduct'] = await captureCase(publicationUpdateMutation, {
    id: publicationId,
    input: {
      publishablesToAdd: [product.id],
      autoPublish: true,
    },
  });
  cases['updateRemoveProduct'] = await captureCase(publicationUpdateMutation, {
    id: publicationId,
    input: {
      publishablesToRemove: [product.id],
    },
  });
  cases['updateInvalidProduct'] = await captureCase(publicationUpdateMutation, {
    id: publicationId,
    input: {
      publishablesToAdd: ['gid://shopify/Product/999999999999'],
    },
  });
  cases['updateTooManyProducts'] = await captureCase(publicationUpdateMutation, {
    id: publicationId,
    input: {
      publishablesToAdd: Array.from({ length: 51 }, (_, index) => `gid://shopify/Product/${900000000000 + index}`),
    },
  });
  cases['updateMissingPublication'] = await captureCase(publicationUpdateMutation, {
    id: 'gid://shopify/Publication/999999999999',
    input: {
      autoPublish: true,
    },
  });
  cases['deleteMissingPublication'] = await captureCase(publicationDeleteMutation, {
    id: 'gid://shopify/Publication/999999999999',
  });

  deleteCreated = await captureCase(publicationDeleteMutation, { id: publicationId });
  cases['deleteCreatedPublication'] = deleteCreated;
  publicationId = null;
} finally {
  if (publicationId) {
    try {
      publicationCleanup = await runGraphqlRaw<PublicationDeleteData>(publicationDeleteMutation, { id: publicationId });
    } catch (error) {
      console.warn(
        JSON.stringify(
          {
            ok: false,
            cleanup: 'publicationDelete',
            publicationId,
            error: error instanceof Error ? error.message : String(error),
          },
          null,
          2,
        ),
      );
    }
  }
  if (product?.id) {
    try {
      productCleanup = await runGraphqlRaw<ProductDeleteData>(deleteProductMutation, {
        input: { id: product.id },
      });
    } catch (error) {
      console.warn(
        JSON.stringify(
          {
            ok: false,
            cleanup: 'productDelete',
            productId: product.id,
            error: error instanceof Error ? error.message : String(error),
          },
          null,
          2,
        ),
      );
    }
  }
}

if (!product?.id || !deleteCreated || Object.keys(cases).length === 0) {
  throw new Error('publication mutation contract capture did not produce required setup/cases.');
}

await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      scenarioId: 'publication-mutation-contract',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup: {
        product,
      },
      seedProducts: [product],
      cases,
      cleanup: {
        publicationDelete: publicationCleanup,
        productDelete: productCleanup,
      },
      notes: [
        'Live Admin GraphQL 2026-04 publicationCreate accepts omitted catalogId and creates a publication.',
        'Live unknown catalogId returns CATALOG_NOT_FOUND with an id-specific message.',
        'Live publicationUpdate accepts Product publishables and returns payload userErrors for missing Product IDs and update batches over 50.',
        'Live ProductVariant IDs resolved through node(id:) but publicationUpdate returned top-level RESOURCE_NOT_FOUND for variants on this store, so this fixture does not assert the local ProductVariant guardrail.',
        'publicationDelete payload exposes deletedId and userErrors only.',
      ],
      upstreamCalls: [],
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
      productId: product.id,
      caseCount: Object.keys(cases).length,
      deletedPublicationId: deleteCreated.response.payload.data?.publicationDelete?.deletedId ?? null,
      cleanupDeletedProductId: productCleanup?.payload.data?.productDelete?.deletedProductId ?? null,
    },
    null,
    2,
  ),
);
