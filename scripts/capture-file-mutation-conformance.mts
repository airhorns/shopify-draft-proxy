/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { runAdminGraphql } from './conformance-graphql-client.mjs';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

type GraphqlVariables = Record<string, unknown>;
type UserError = { field?: string[] | null; message?: string | null; code?: string | null };
type GraphqlPayload<TData> = {
  data?: TData;
  errors?: unknown;
  extensions?: unknown;
};

type FileCreateData = {
  fileCreate?: {
    files?: Array<{ id?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};

type FileDeleteData = {
  fileDelete?: {
    deletedFileIds?: string[] | null;
    userErrors?: UserError[] | null;
  } | null;
};

type ProductCreateData = {
  productCreate?: {
    product?: { id?: string | null } | null;
    userErrors?: UserError[] | null;
  } | null;
};

type ProductDeleteData = {
  productDelete?: {
    userErrors?: UserError[] | null;
  } | null;
};

type ProductCreateMediaData = {
  productCreateMedia?: {
    media?: Array<{ id?: string | null } | null> | null;
    mediaUserErrors?: UserError[] | null;
  } | null;
};

type ProductMediaReadData = {
  product?: {
    id?: string | null;
    media?: {
      nodes?: Array<{ id?: string | null } | null> | null;
    } | null;
  } | null;
};

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`);
  }

  return value;
}

const storeDomain = requireEnv('SHOPIFY_CONFORMANCE_STORE_DOMAIN');
const adminOrigin = requireEnv('SHOPIFY_CONFORMANCE_ADMIN_ORIGIN');
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);

async function runGraphql<TData>(query: string, variables: GraphqlVariables = {}): Promise<GraphqlPayload<TData>> {
  return (await runAdminGraphql<TData>(
    {
      adminOrigin,
      apiVersion,
      headers: buildAdminAuthHeaders(adminAccessToken),
    },
    query,
    variables,
  )) as GraphqlPayload<TData>;
}

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

const fileCreateMutation = `#graphql
  mutation FileCreateDeleteParity($files: [FileCreateInput!]!) {
    fileCreate(files: $files) {
      files {
        id
        alt
        createdAt
        fileStatus
        ... on MediaImage {
          image {
            url
            width
            height
          }
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation FileDeleteParity($fileIds: [ID!]!) {
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

const productCreateMutation = `#graphql
  mutation FileDeleteMediaReferenceSeedProduct($product: ProductCreateInput!) {
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

const productDeleteMutation = `#graphql
  mutation FileDeleteMediaReferenceCleanupProduct($input: ProductDeleteInput!) {
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
  mutation FileDeleteMediaReferenceSeedMedia($productId: ID!, $media: [CreateMediaInput!]!) {
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

const productMediaReadQuery = `#graphql
  query FileDeleteMediaReferenceDownstream($id: ID!) {
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

function buildFileCreateVariables(runId: string): GraphqlVariables {
  return {
    files: [
      {
        contentType: 'IMAGE',
        originalSource: 'https://placehold.co/600x400/png',
        alt: `Hermes Files API conformance ${runId}`,
      },
    ],
  };
}

function buildProductCreateVariables(runId: string): GraphqlVariables {
  return {
    product: {
      title: `Hermes fileDelete media reference ${runId}`,
      status: 'DRAFT',
    },
  };
}

function buildProductMediaVariables(productId: string): GraphqlVariables {
  return {
    productId,
    media: [
      {
        mediaContentType: 'IMAGE',
        originalSource: 'https://placehold.co/600x400/png',
        alt: 'File delete media reference',
      },
    ],
  };
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
let createdFileId: string | null = null;
let productId: string | null = null;
let productMediaId: string | null = null;

try {
  const createVariables = buildFileCreateVariables(runId);
  const createResponse = await runGraphql<FileCreateData>(fileCreateMutation, createVariables);
  expectNoUserErrors('fileCreate', createResponse.data?.fileCreate?.userErrors);
  createdFileId = requireId('fileCreate.files[0]', createResponse.data?.fileCreate?.files?.[0]?.id);

  const deleteCreatedVariables = { fileIds: [createdFileId] };
  const deleteCreatedResponse = await runGraphql<FileDeleteData>(fileDeleteMutation, deleteCreatedVariables);
  expectNoUserErrors('fileDelete (created file)', deleteCreatedResponse.data?.fileDelete?.userErrors);

  const productCreateVariables = buildProductCreateVariables(runId);
  const productCreateResponse = await runGraphql<ProductCreateData>(productCreateMutation, productCreateVariables);
  expectNoUserErrors(
    'productCreate (fileDelete media reference seed)',
    productCreateResponse.data?.productCreate?.userErrors,
  );
  productId = requireId('productCreate.product', productCreateResponse.data?.productCreate?.product?.id);

  const mediaCreateVariables = buildProductMediaVariables(productId);
  const mediaCreateResponse = await runGraphql<ProductCreateMediaData>(
    productCreateMediaMutation,
    mediaCreateVariables,
  );
  expectNoUserErrors(
    'productCreateMedia (fileDelete media reference seed)',
    mediaCreateResponse.data?.productCreateMedia?.mediaUserErrors,
  );
  productMediaId = requireId(
    'productCreateMedia.media[0]',
    mediaCreateResponse.data?.productCreateMedia?.media?.[0]?.id,
  );

  const productReadBeforeDelete = await runGraphql<ProductMediaReadData>(productMediaReadQuery, { id: productId });
  const deleteMediaReferenceVariables = { fileIds: [productMediaId] };
  const deleteMediaReferenceResponse = await runGraphql<FileDeleteData>(
    fileDeleteMutation,
    deleteMediaReferenceVariables,
  );
  expectNoUserErrors('fileDelete (product media reference)', deleteMediaReferenceResponse.data?.fileDelete?.userErrors);
  const productReadAfterDelete = await runGraphql<ProductMediaReadData>(productMediaReadQuery, { id: productId });

  const captures = {
    'file-create-delete-parity.json': {
      createMutation: {
        variables: createVariables,
        response: createResponse,
      },
      deleteMutation: {
        variables: deleteCreatedVariables,
        response: deleteCreatedResponse,
      },
    },
    'file-delete-product-media-parity.json': {
      setup: {
        productCreate: {
          variables: productCreateVariables,
          response: productCreateResponse,
        },
        productCreateMedia: {
          variables: mediaCreateVariables,
          response: mediaCreateResponse,
        },
        productReadBeforeDelete,
      },
      mutation: {
        variables: deleteMediaReferenceVariables,
        response: deleteMediaReferenceResponse,
      },
      downstreamRead: productReadAfterDelete,
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
        createdFileId,
        productId,
        productMediaId,
      },
      null,
      2,
    ),
  );
} finally {
  if (createdFileId) {
    try {
      await runGraphql<ProductDeleteData>(fileDeleteMutation, { fileIds: [createdFileId] });
    } catch {
      // Best-effort cleanup only.
    }
  }

  if (productMediaId) {
    try {
      await runGraphql<ProductDeleteData>(fileDeleteMutation, { fileIds: [productMediaId] });
    } catch {
      // Best-effort cleanup only.
    }
  }

  if (productId) {
    try {
      await runGraphql<ProductDeleteData>(productDeleteMutation, { input: { id: productId } });
    } catch {
      // Best-effort cleanup only.
    }
  }
}
