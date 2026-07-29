/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;
type UserError = { field?: string[] | null; message?: string | null; code?: string | null };
type GraphqlPayload<TData> = {
  data?: TData;
  errors?: unknown;
  extensions?: unknown;
};
type ProductNode = {
  id?: string | null;
  title?: string | null;
  variants?: { nodes?: Array<{ id?: string | null; title?: string | null } | null> | null } | null;
};
type ProductCreateData = {
  productCreate?: {
    product?: ProductNode | null;
    userErrors?: UserError[] | null;
  } | null;
};
type ProductDeleteData = {
  productDelete?: {
    deletedProductId?: string | null;
    userErrors?: UserError[] | null;
  } | null;
};
type ProductCreateMediaData = {
  productCreateMedia?: {
    media?: Array<{ id?: string | null; status?: string | null } | null> | null;
    mediaUserErrors?: UserError[] | null;
  } | null;
};
type ProductMediaReadData = {
  product?: {
    id?: string | null;
    media?: { nodes?: Array<{ id?: string | null; status?: string | null } | null> | null } | null;
  } | null;
};
type FileReadData = {
  node?: {
    id?: string | null;
    __typename?: string | null;
    alt?: string | null;
    fileStatus?: string | null;
    mediaContentType?: string | null;
    status?: string | null;
    preview?: { image?: { url?: string | null; width?: number | null; height?: number | null } | null } | null;
    image?: { url?: string | null; width?: number | null; height?: number | null } | null;
  } | null;
};
type ProductVariantAppendMediaData = {
  productVariantAppendMedia?: {
    product?: { id?: string | null } | null;
    productVariants?: Array<{
      id?: string | null;
      media?: { nodes?: Array<{ id?: string | null } | null> | null } | null;
    } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileDeleteData = {
  fileDelete?: {
    deletedFileIds?: string[] | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileUpdateData = {
  fileUpdate?: {
    files?: Array<{ id?: string | null; alt?: string | null; fileStatus?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};
type ProductVariantMediaReadData = {
  productVariant?: {
    id?: string | null;
    media?: {
      nodes?: Array<{ id?: string | null; alt?: string | null; mediaContentType?: string | null } | null> | null;
    };
  } | null;
};
type RecordedUpstreamCall = {
  operationName: string;
  variables: GraphqlVariables;
  query: string;
  response: {
    status: number;
    body: GraphqlPayload<unknown>;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload<TData>>;
  runGraphqlRequest: <TData>(
    query: string,
    variables?: GraphqlVariables,
  ) => Promise<{ status: number; payload: GraphqlPayload<TData> }>;
};

const mediaFileTargetHydrateQuery = `query MediaFileTargetHydrate($fileIds: [ID!]!) {
  nodes(ids: $fileIds) {
    id
    __typename
    ... on File {
      alt
      createdAt
      fileStatus
    }
    ... on MediaImage {
      image { url width height }
      preview { image { url width height } }
    }
    ... on GenericFile {
      url
    }
  }
}`;

const mediaFileUpdateHydrateQuery = `query MediaFileUpdateHydrate($fileIds: [ID!]!) {
  nodes(ids: $fileIds) {
    id
    __typename
    ... on File {
      alt
      createdAt
      fileStatus
    }
    ... on MediaImage {
      image { url width height }
      preview { image { url width height } }
    }
    ... on GenericFile {
      url
    }
  }
}`;

const mediaProductHydrateQuery = `query MediaProductHydrate($id: ID!) {
  product(id: $id) {
    id
    title
    handle
    status
    media(first: 50) {
      nodes {
        id
        alt
        mediaContentType
        status
        preview { image { url width height } }
        ... on MediaImage { image { url width height } }
      }
    }
    variants(first: 50) {
      nodes {
        id
        title
        media(first: 10) { nodes { id } }
      }
    }
  }
}`;

const mediaVariantOwnerHydrateQuery = `query MediaVariantOwnerHydrate($id: ID!) {
  node(id: $id) {
    ... on ProductVariant {
      id
      title
      product { id }
      media(first: 10) {
        nodes {
          id
          __typename
          alt
          mediaContentType
          status
          preview { image { url width height } }
          ... on MediaImage { image { url width height } }
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
    }
  }
}`;

const productCreateMutation = `#graphql
  mutation MediaFileCascadeProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
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

const productDeleteMutation = `#graphql
  mutation MediaFileCascadeCleanupProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const productCreateMediaMutation = `#graphql
  mutation MediaFileCascadeProductCreateMedia($productId: ID!, $media: [CreateMediaInput!]!) {
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
          }
        }
      }
    }
  }
`;

const productMediaReadQuery = `#graphql
  query MediaFileCascadeProductMediaReady($productId: ID!) {
    product(id: $productId) {
      id
      media(first: 10) {
        nodes {
          id
          status
        }
      }
    }
  }
`;

const fileReadQuery = `#graphql
  query MediaFileCascadeFileHydrateRead($id: ID!) {
    node(id: $id) {
      id
      __typename
      ... on MediaImage {
        alt
        fileStatus
        mediaContentType
        status
        preview {
          image {
            url
            width
            height
          }
        }
        image {
          url
          width
          height
        }
      }
    }
  }
`;

const productVariantAppendMediaMutation = `#graphql
  mutation MediaFileCascadeVariantAppendMedia($productId: ID!, $variantMedia: [ProductVariantAppendMediaInput!]!) {
    productVariantAppendMedia(productId: $productId, variantMedia: $variantMedia) {
      product {
        id
      }
      productVariants {
        id
        media(first: 5) {
          nodes {
            id
            alt
            mediaContentType
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

const fileDeleteMutation = `#graphql
  mutation MediaFileCascadeFileDelete($fileIds: [ID!]!) {
    fileDelete(fileIds: $fileIds) {
      deletedFileIds
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const fileUpdateMutation = `#graphql
  mutation MediaFileCascadeFileUpdateRemoveReference($files: [FileUpdateInput!]!) {
    fileUpdate(files: $files) {
      files {
        id
        alt
        fileStatus
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const variantMediaReadQuery = `#graphql
  query MediaFileCascadeVariantMediaRead($variantId: ID!) {
    productVariant(id: $variantId) {
      id
      media(first: 5) {
        nodes {
          id
          alt
          mediaContentType
        }
      }
    }
  }
`;

function expectNoUserErrors(pathLabel: string, userErrors: UserError[] | null | undefined): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${pathLabel} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function requireId(pathLabel: string, id: string | null | undefined): string {
  if (typeof id === 'string' && id.length > 0) {
    return id;
  }

  throw new Error(`${pathLabel} did not return an id.`);
}

async function recordUpstreamCall(
  operationName: string,
  query: string,
  variables: GraphqlVariables,
): Promise<RecordedUpstreamCall> {
  const response = await runGraphqlRequest<unknown>(query, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors !== undefined) {
    throw new Error(
      `${operationName} failed during capture: HTTP ${response.status} ${JSON.stringify(response.payload, null, 2)}`,
    );
  }
  return {
    operationName,
    variables,
    query,
    response: { status: response.status, body: response.payload },
  };
}

function productCreateVariables(label: string, runId: string): GraphqlVariables {
  return {
    product: {
      title: `Hermes media cascade ${label} ${runId}`,
      status: 'DRAFT',
    },
  };
}

function productMediaVariables(productId: string, label: string): GraphqlVariables {
  return {
    productId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: `https://placehold.co/640x480/png?text=${label}`,
        alt: `Media cascade ${label}`,
      },
    ],
  };
}

async function waitForReadyMedia(productId: string, mediaId: string): Promise<GraphqlPayload<ProductMediaReadData>> {
  let lastPayload: GraphqlPayload<ProductMediaReadData> | null = null;
  for (let attempt = 0; attempt < 15; attempt += 1) {
    lastPayload = await runGraphql<ProductMediaReadData>(productMediaReadQuery, { productId });
    const media = lastPayload.data?.product?.media?.nodes?.find((node) => node?.id === mediaId);
    if (media?.status === 'READY') {
      return lastPayload;
    }

    await delay(2000);
  }

  throw new Error(`Timed out waiting for media ${mediaId} to reach READY: ${JSON.stringify(lastPayload, null, 2)}`);
}

async function createProductWithVariantMedia(
  label: string,
  runId: string,
): Promise<{
  productId: string;
  variantId: string;
  mediaId: string;
  productCreate: { variables: GraphqlVariables; response: GraphqlPayload<ProductCreateData> };
  productCreateMedia: { variables: GraphqlVariables; response: GraphqlPayload<ProductCreateMediaData> };
  mediaReadyRead: { variables: GraphqlVariables; response: GraphqlPayload<ProductMediaReadData> };
  fileReadBeforeCascade: { variables: GraphqlVariables; response: GraphqlPayload<FileReadData> };
  productVariantAppendMedia: { variables: GraphqlVariables; response: GraphqlPayload<ProductVariantAppendMediaData> };
  variantReadBeforeCascade: { variables: GraphqlVariables; response: GraphqlPayload<ProductVariantMediaReadData> };
}> {
  const createVariables = productCreateVariables(label, runId);
  const productCreateResponse = await runGraphql<ProductCreateData>(productCreateMutation, createVariables);
  expectNoUserErrors(`${label} productCreate`, productCreateResponse.data?.productCreate?.userErrors);
  const product = productCreateResponse.data?.productCreate?.product;
  const productId = requireId(`${label} productCreate.product`, product?.id);
  const variantId = requireId(`${label} productCreate.product.variants.nodes[0]`, product?.variants?.nodes?.[0]?.id);

  const mediaVariables = productMediaVariables(productId, label);
  const productCreateMediaResponse = await runGraphql<ProductCreateMediaData>(
    productCreateMediaMutation,
    mediaVariables,
  );
  expectNoUserErrors(
    `${label} productCreateMedia`,
    productCreateMediaResponse.data?.productCreateMedia?.mediaUserErrors,
  );
  const mediaId = requireId(
    `${label} productCreateMedia.media[0]`,
    productCreateMediaResponse.data?.productCreateMedia?.media?.[0]?.id,
  );

  const mediaReadyVariables = { productId };
  const mediaReadyRead = await waitForReadyMedia(productId, mediaId);
  const fileReadVariables = { id: mediaId };
  const fileReadBeforeCascade = await runGraphql<FileReadData>(fileReadQuery, fileReadVariables);
  const appendVariables = {
    productId,
    variantMedia: [{ variantId, mediaIds: [mediaId] }],
  };
  const appendResponse = await runGraphql<ProductVariantAppendMediaData>(
    productVariantAppendMediaMutation,
    appendVariables,
  );
  expectNoUserErrors(`${label} productVariantAppendMedia`, appendResponse.data?.productVariantAppendMedia?.userErrors);

  const variantReadVariables = { variantId };
  const variantReadBeforeCascade = await runGraphql<ProductVariantMediaReadData>(
    variantMediaReadQuery,
    variantReadVariables,
  );

  return {
    productId,
    variantId,
    mediaId,
    productCreate: { variables: createVariables, response: productCreateResponse },
    productCreateMedia: { variables: mediaVariables, response: productCreateMediaResponse },
    mediaReadyRead: { variables: mediaReadyVariables, response: mediaReadyRead },
    fileReadBeforeCascade: { variables: fileReadVariables, response: fileReadBeforeCascade },
    productVariantAppendMedia: { variables: appendVariables, response: appendResponse },
    variantReadBeforeCascade: { variables: variantReadVariables, response: variantReadBeforeCascade },
  };
}

async function cleanupProduct(productId: string): Promise<GraphqlPayload<ProductDeleteData>> {
  return runGraphql<ProductDeleteData>(productDeleteMutation, { input: { id: productId } });
}

async function cleanupFile(fileId: string): Promise<GraphqlPayload<FileDeleteData>> {
  return runGraphql<FileDeleteData>(fileDeleteMutation, { fileIds: [fileId] });
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const productIds: string[] = [];
const cleanupFileIds: string[] = [];

try {
  const deleteSetup = await createProductWithVariantMedia('delete', runId);
  productIds.push(deleteSetup.productId);
  const deleteTargetHydrate = await recordUpstreamCall('MediaFileTargetHydrate', mediaFileTargetHydrateQuery, {
    fileIds: [deleteSetup.mediaId],
  });
  const deleteVariantHydrate = await recordUpstreamCall('MediaVariantOwnerHydrate', mediaVariantOwnerHydrateQuery, {
    id: deleteSetup.variantId,
  });
  const deleteVariables = { fileIds: [deleteSetup.mediaId] };
  const deleteResponse = await runGraphql<FileDeleteData>(fileDeleteMutation, deleteVariables);
  expectNoUserErrors('fileDelete cascade', deleteResponse.data?.fileDelete?.userErrors);
  const deleteDownstreamVariables = { variantId: deleteSetup.variantId };
  const deleteDownstreamRead = await runGraphql<ProductVariantMediaReadData>(
    variantMediaReadQuery,
    deleteDownstreamVariables,
  );

  const updateSetup = await createProductWithVariantMedia('update-detach', runId);
  productIds.push(updateSetup.productId);
  cleanupFileIds.push(updateSetup.mediaId);
  const updateProductHydrate = await recordUpstreamCall('MediaProductHydrate', mediaProductHydrateQuery, {
    id: updateSetup.productId,
  });
  const updateTargetHydrate = await recordUpstreamCall('MediaFileUpdateHydrate', mediaFileUpdateHydrateQuery, {
    fileIds: [updateSetup.mediaId],
  });
  const updateVariables = {
    files: [{ id: updateSetup.mediaId, referencesToRemove: [updateSetup.productId] }],
  };
  const updateResponse = await runGraphql<FileUpdateData>(fileUpdateMutation, updateVariables);
  expectNoUserErrors('fileUpdate referencesToRemove cascade', updateResponse.data?.fileUpdate?.userErrors);
  const updateDownstreamVariables = { variantId: updateSetup.variantId };
  const updateDownstreamRead = await runGraphql<ProductVariantMediaReadData>(
    variantMediaReadQuery,
    updateDownstreamVariables,
  );

  const capture = {
    storeDomain,
    apiVersion,
    deleteScenario: {
      setup: deleteSetup,
      mutation: {
        variables: deleteVariables,
        response: deleteResponse,
      },
      downstreamRead: {
        variables: deleteDownstreamVariables,
        response: deleteDownstreamRead,
      },
    },
    updateDetachScenario: {
      setup: updateSetup,
      mutation: {
        variables: updateVariables,
        response: updateResponse,
      },
      downstreamRead: {
        variables: updateDownstreamVariables,
        response: updateDownstreamRead,
      },
    },
    upstreamCalls: [deleteTargetHydrate, deleteVariantHydrate, updateProductHydrate, updateTargetHydrate],
  };

  await writeFile(
    path.join(outputDir, 'media-file-cascade-variant-media-clear.json'),
    `${JSON.stringify(capture, null, 2)}\n`,
    'utf8',
  );
  console.log(`Wrote ${path.join(outputDir, 'media-file-cascade-variant-media-clear.json')}`);
} finally {
  const cleanup: unknown[] = [];
  for (const fileId of cleanupFileIds) {
    cleanup.push(await cleanupFile(fileId).catch((error: unknown) => ({ error: String(error) })));
  }
  for (const productId of productIds) {
    cleanup.push(await cleanupProduct(productId).catch((error: unknown) => ({ error: String(error) })));
  }
  if (cleanup.length > 0) {
    console.log(`Cleanup results: ${JSON.stringify(cleanup)}`);
  }
}
