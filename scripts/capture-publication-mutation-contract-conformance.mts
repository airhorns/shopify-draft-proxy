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

type PublicationSeed = {
  id: string;
  name?: string | null;
  catalog?: { __typename?: string; id?: string | null; title?: string | null } | null;
  channels?: {
    nodes?: Array<{ id: string; name?: string | null; handle?: string | null }>;
  } | null;
};

type PublicationsData = {
  publications?: { nodes?: PublicationSeed[] } | null;
};

type PublicationProductsConnectionData = {
  publication?: {
    products?: {
      pageInfo?: {
        startCursor?: string | null;
        endCursor?: string | null;
      } | null;
    } | null;
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
const publicationReadDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'publication-created-read.graphql',
);
const publicationProductsConnectionDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'publication-products-connection-read.graphql',
);
const publicationDeleteHydrateDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'publication-delete-hydrate.graphql',
);
const publicationDeleteDownstreamReadDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'publication-delete-downstream-read.graphql',
);
const publicationDeleteMembershipReadDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'publication-delete-membership-read.graphql',
);

const publicationCreateMutation = await readFile(publicationCreateDocumentPath, 'utf8');
const publicationUpdateMutation = await readFile(publicationUpdateDocumentPath, 'utf8');
const publicationDeleteMutation = await readFile(publicationDeleteDocumentPath, 'utf8');
const publicationReadQuery = await readFile(publicationReadDocumentPath, 'utf8');
const publicationProductsConnectionQuery = await readFile(publicationProductsConnectionDocumentPath, 'utf8');
const publicationDeleteHydrateQuery = await readFile(publicationDeleteHydrateDocumentPath, 'utf8');
const publicationDeleteDownstreamReadQuery = await readFile(publicationDeleteDownstreamReadDocumentPath, 'utf8');
const publicationDeleteMembershipReadQuery = await readFile(publicationDeleteMembershipReadDocumentPath, 'utf8');

// The node-hydrate query the proxy forwards in live-hybrid to prove a publishable
// product/variant exists before staging a publicationUpdate. Shared verbatim with
// PRODUCTS_HYDRATE_NODES_OBSERVATION_QUERY (src/proxy/product_helpers.rs) so the
// recorded cassettes match the proxy's emitted forward byte-for-byte.
const observationHydrateDocumentPath = path.join(
  'config',
  'parity-requests',
  'products',
  'products-hydrate-nodes-observation.graphql',
);
const observationHydrateQuery = await readFile(observationHydrateDocumentPath, 'utf8');

type UpstreamCall = {
  operationName: string;
  variables: JsonObject;
  query: string;
  response: { status: number; body: unknown };
};

async function recordObservationHydrate(ids: string[]): Promise<UpstreamCall> {
  const variables = { ids } satisfies JsonObject;
  const result = await runGraphqlRaw(observationHydrateQuery, variables);
  return {
    operationName: 'ProductsHydrateNodes',
    variables,
    query: observationHydrateQuery,
    response: { status: result.status, body: result.payload },
  };
}

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

const variantNodeQuery = `#graphql
  query PublicationContractVariantNode($id: ID!) {
    node(id: $id) {
      __typename
      id
      ... on ProductVariant {
        title
        product {
          id
          title
        }
      }
    }
  }
`;

const protectedPublicationQuery = `#graphql
  query PublicationDeleteProtectedCandidate {
    publications(first: 100) {
      nodes {
        id
        name
        catalog {
          __typename
          id
          title
        }
        channels(first: 5) {
          nodes {
            id
            name
            handle
          }
        }
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
let secondProduct: ProductSeed | null = null;
let publicationId: string | null = null;
let persistedPublicationId: string | null = null;
let variantId: string | null = null;
let productCleanup: ConformanceGraphqlResult<ProductDeleteData> | null = null;
let secondProductCleanup: ConformanceGraphqlResult<ProductDeleteData> | null = null;
let publicationCleanup: ConformanceGraphqlResult<PublicationDeleteData> | null = null;
let deleteCreated: CaptureCase | null = null;
let deletePersisted: CaptureCase | null = null;
const cases: Record<string, CaptureCase> = {};
const upstreamCalls: UpstreamCall[] = [];

try {
  const productCreate = await runGraphqlRaw<ProductCreateData>(createProductMutation, {
    product: {
      title: `Publication mutation contract ${runId}`,
      status: 'ACTIVE',
    },
  });
  product = getCreatedProduct(productCreate);
  if (!product?.id) {
    throw new Error(
      `publication mutation contract capture could not create a seed product: ${JSON.stringify(productCreate)}`,
    );
  }
  variantId = product.variants?.nodes?.[0]?.id ?? null;
  if (!variantId) {
    throw new Error(
      `publication mutation contract capture could not resolve a seed variant: ${JSON.stringify(productCreate)}`,
    );
  }
  cases['productCreate'] = {
    query: createProductMutation,
    variables: {
      product: {
        title: `Publication mutation contract ${runId}`,
        status: 'ACTIVE',
      },
    },
    response: productCreate,
  };
  const secondProductCreate = await runGraphqlRaw<ProductCreateData>(createProductMutation, {
    product: {
      title: `Publication products connection ${runId}`,
      status: 'ACTIVE',
    },
  });
  secondProduct = getCreatedProduct(secondProductCreate);
  if (!secondProduct?.id) {
    throw new Error(
      `publication products connection capture could not create a second seed product: ${JSON.stringify(
        secondProductCreate,
      )}`,
    );
  }
  cases['secondProductCreate'] = {
    query: createProductMutation,
    variables: {
      product: {
        title: `Publication products connection ${runId}`,
        status: 'ACTIVE',
      },
    },
    response: secondProductCreate,
  };

  // Record the proxy's live read-through forwards: the real created product
  // resolves to a hydrated node (so publicationUpdate stages it), while the
  // sentinel id resolves to a null node (so it is reported "not found").
  upstreamCalls.push(await recordObservationHydrate([product.id]));
  upstreamCalls.push(await recordObservationHydrate([secondProduct.id]));
  upstreamCalls.push(await recordObservationHydrate([variantId]));
  upstreamCalls.push(await recordObservationHydrate(['gid://shopify/Product/999999999999']));
  cases['variantNode'] = await captureCase(variantNodeQuery, { id: variantId });

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
  cases['createdPublicationRead'] = await captureCase(publicationReadQuery, { id: publicationId });

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
  cases['updateAddSecondProduct'] = await captureCase(publicationUpdateMutation, {
    id: publicationId,
    input: {
      publishablesToAdd: [secondProduct.id],
    },
  });
  const publicationProductsFirstPage = await captureCase(publicationProductsConnectionQuery, {
    publicationId,
    first: 1,
  });
  cases['publicationProductsFirstPage'] = publicationProductsFirstPage;
  const firstPageEndCursor = (
    publicationProductsFirstPage.response as ConformanceGraphqlResult<PublicationProductsConnectionData>
  ).payload.data?.publication?.products?.pageInfo?.endCursor;
  if (typeof firstPageEndCursor !== 'string' || firstPageEndCursor.length === 0) {
    throw new Error(
      `Publication.products first page did not return an endCursor: ${JSON.stringify(publicationProductsFirstPage)}`,
    );
  }
  const publicationProductsAfterPage = await captureCase(publicationProductsConnectionQuery, {
    publicationId,
    first: 1,
    after: firstPageEndCursor,
  });
  cases['publicationProductsAfterPage'] = publicationProductsAfterPage;
  const afterPageStartCursor = (
    publicationProductsAfterPage.response as ConformanceGraphqlResult<PublicationProductsConnectionData>
  ).payload.data?.publication?.products?.pageInfo?.startCursor;
  if (typeof afterPageStartCursor !== 'string' || afterPageStartCursor.length === 0) {
    throw new Error(
      `Publication.products after page did not return a startCursor: ${JSON.stringify(publicationProductsAfterPage)}`,
    );
  }
  cases['publicationProductsBeforePage'] = await captureCase(publicationProductsConnectionQuery, {
    publicationId,
    last: 1,
    before: afterPageStartCursor,
  });
  cases['updateRemoveProduct'] = await captureCase(publicationUpdateMutation, {
    id: publicationId,
    input: {
      publishablesToRemove: [product.id],
    },
  });
  cases['updateAddVariant'] = await captureCase(publicationUpdateMutation, {
    id: publicationId,
    input: {
      publishablesToAdd: [variantId],
    },
  });
  cases['updateAddProductAndVariant'] = await captureCase(publicationUpdateMutation, {
    id: publicationId,
    input: {
      publishablesToAdd: [product.id, variantId],
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
  cases['afterDeletePublicationRead'] = await captureCase(publicationReadQuery, { id: publicationId });
  publicationId = null;

  const persistedPublicationCreate = await captureCase(publicationCreateMutation, { input: {} });
  cases['persistedPublicationCreate'] = persistedPublicationCreate;
  persistedPublicationId = getCreatedPublicationId(
    persistedPublicationCreate.response as ConformanceGraphqlResult<PublicationCreateData>,
  );
  if (!persistedPublicationId) {
    throw new Error(
      `publicationCreate(input: {}) did not return the persisted-delete publication id: ${JSON.stringify(
        persistedPublicationCreate,
      )}`,
    );
  }
  cases['persistedPublicationAddProduct'] = await captureCase(publicationUpdateMutation, {
    id: persistedPublicationId,
    input: { publishablesToAdd: [product.id] },
  });

  const persistedHydrateVariables = { id: persistedPublicationId };
  const persistedHydrate = await captureCase(publicationDeleteHydrateQuery, persistedHydrateVariables);
  cases['persistedPublicationHydrateBeforeDelete'] = persistedHydrate;
  upstreamCalls.push({
    operationName: 'PublicationDeleteHydrate',
    variables: persistedHydrateVariables,
    query: publicationDeleteHydrateQuery,
    response: { status: persistedHydrate.response.status, body: persistedHydrate.response.payload },
  });

  const persistedDownstreamVariables = {
    publicationId: persistedPublicationId,
    productId: product.id,
  };
  const persistedDownstreamBefore = await captureCase(
    publicationDeleteDownstreamReadQuery,
    persistedDownstreamVariables,
  );
  cases['persistedPublicationDownstreamBeforeDelete'] = persistedDownstreamBefore;
  upstreamCalls.push({
    operationName: 'PublicationDeleteDownstreamRead',
    variables: persistedDownstreamVariables,
    query: publicationDeleteDownstreamReadQuery,
    response: {
      status: persistedDownstreamBefore.response.status,
      body: persistedDownstreamBefore.response.payload,
    },
  });

  deletePersisted = await captureCase(publicationDeleteMutation, { id: persistedPublicationId });
  cases['deletePersistedPublication'] = deletePersisted;
  cases['persistedPublicationDownstreamAfterDelete'] = await captureCase(
    publicationDeleteDownstreamReadQuery,
    persistedDownstreamVariables,
  );
  cases['persistedPublicationMembershipAfterDelete'] = await captureCase(
    publicationDeleteMembershipReadQuery,
    persistedDownstreamVariables,
  );
  persistedPublicationId = null;

  const protectedCandidates = await captureCase(protectedPublicationQuery, {});
  cases['protectedPublicationCandidates'] = protectedCandidates;
  const protectedNodes = (protectedCandidates.response as ConformanceGraphqlResult<PublicationsData>).payload.data
    ?.publications?.nodes;
  const protectedPublication =
    protectedNodes?.find((publication) =>
      publication.channels?.nodes?.some((channel) => channel.handle === 'online_store'),
    ) ?? protectedNodes?.find((publication) => publication.catalog?.__typename === 'AppCatalog');
  if (!protectedPublication?.id) {
    throw new Error(
      `publication deletion capture could not resolve a protected app publication: ${JSON.stringify(
        protectedCandidates,
      )}`,
    );
  }
  const protectedHydrateVariables = { id: protectedPublication.id };
  const protectedHydrate = await captureCase(publicationDeleteHydrateQuery, protectedHydrateVariables);
  cases['protectedPublicationHydrateBeforeDelete'] = protectedHydrate;
  upstreamCalls.push({
    operationName: 'PublicationDeleteHydrate',
    variables: protectedHydrateVariables,
    query: publicationDeleteHydrateQuery,
    response: { status: protectedHydrate.response.status, body: protectedHydrate.response.payload },
  });
  const protectedDelete = await captureCase(publicationDeleteMutation, { id: protectedPublication.id });
  cases['deleteProtectedAppPublication'] = protectedDelete;
  const protectedDeletePayload = (protectedDelete.response as ConformanceGraphqlResult<PublicationDeleteData>).payload
    .data?.publicationDelete;
  if (
    protectedDeletePayload?.deletedId != null ||
    !protectedDeletePayload.userErrors?.some((error) => error.code === 'CANNOT_MODIFY_APP_CATALOG_PUBLICATION')
  ) {
    throw new Error(`expected the protected app publication delete to be rejected: ${JSON.stringify(protectedDelete)}`);
  }
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
  if (persistedPublicationId) {
    try {
      publicationCleanup = await runGraphqlRaw<PublicationDeleteData>(publicationDeleteMutation, {
        id: persistedPublicationId,
      });
    } catch (error) {
      console.warn(
        JSON.stringify(
          {
            ok: false,
            cleanup: 'publicationDelete',
            publicationId: persistedPublicationId,
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
  if (secondProduct?.id) {
    try {
      secondProductCleanup = await runGraphqlRaw<ProductDeleteData>(deleteProductMutation, {
        input: { id: secondProduct.id },
      });
    } catch (error) {
      console.warn(
        JSON.stringify(
          {
            ok: false,
            cleanup: 'productDelete',
            productId: secondProduct.id,
            error: error instanceof Error ? error.message : String(error),
          },
          null,
          2,
        ),
      );
    }
  }
}

if (!product?.id || !deleteCreated || !deletePersisted || Object.keys(cases).length === 0) {
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
        secondProduct,
      },
      cases,
      cleanup: {
        publicationDelete: publicationCleanup,
        productDelete: productCleanup,
        secondProductDelete: secondProductCleanup,
      },
      notes: [
        'Live Admin GraphQL 2026-04 publicationCreate accepts omitted catalogId and creates a publication.',
        'Live unknown catalogId returns CATALOG_NOT_FOUND with an id-specific message.',
        'Live publicationUpdate accepts Product publishables and returns payload userErrors for missing Product IDs and update batches over 50.',
        'Live Publication.products returns a ProductConnection with edges, pageInfo, cursor windowing, includedProductsCount, and full selected Product node projection after publicationUpdate adds products.',
        'Live ProductVariant IDs resolved through node(id:) but publicationUpdate returned top-level RESOURCE_NOT_FOUND for ProductVariant-only and Product+ProductVariant publishablesToAdd inputs.',
        'publicationDelete payload exposes deletedId and userErrors only.',
        'publication(id:) returns the created publication before delete and null immediately after deleting that publication.',
        'A publication created upstream before proxy replay is classified from query-only catalog/channel hydration and can be deleted locally from a fresh proxy session.',
        'The persisted deletion removes the publication from detail/list/count and Product publication-membership reads.',
        'The Online Store/AppCatalog publication rejects deletion with CANNOT_MODIFY_APP_CATALOG_PUBLICATION.',
      ],
      upstreamCalls,
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
      secondProductId: secondProduct?.id ?? null,
      variantId,
      caseCount: Object.keys(cases).length,
      deletedPublicationId:
        (deleteCreated.response as ConformanceGraphqlResult<PublicationDeleteData>).payload.data?.publicationDelete
          ?.deletedId ?? null,
      deletedPersistedPublicationId:
        (deletePersisted.response as ConformanceGraphqlResult<PublicationDeleteData>).payload.data?.publicationDelete
          ?.deletedId ?? null,
      cleanupDeletedProductId: productCleanup?.payload.data?.productDelete?.deletedProductId ?? null,
      cleanupDeletedSecondProductId: secondProductCleanup?.payload.data?.productDelete?.deletedProductId ?? null,
    },
    null,
    2,
  ),
);
