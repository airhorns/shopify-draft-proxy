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

type FileUpdateData = {
  fileUpdate?: {
    files?: Array<{ id?: string | null; fileStatus?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};

type FileReadData = {
  node?: {
    id?: string | null;
    fileStatus?: string | null;
  } | null;
};

type FilesReadData = {
  files?: {
    nodes?: Array<{
      id?: string | null;
      fileStatus?: string | null;
    } | null> | null;
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload<TData>>;
};

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

const fileUpdateMutation = `#graphql
  mutation FileUpdateParity($files: [FileUpdateInput!]!) {
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

const fileReadQuery = `#graphql
  query FileReadyPoll($id: ID!) {
    node(id: $id) {
      ... on MediaImage {
        id
        fileStatus
      }
    }
  }
`;

const fileCreateThenImageReadMutation = `#graphql
  mutation MediaFileCreateThenImageRead($files: [FileCreateInput!]!) {
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
          preview {
            image {
              url
            }
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

const filesAfterCreateReadQuery = `#graphql
  query MediaFileCreateThenImageReadFiles {
    files(first: 5, reverse: true) {
      nodes {
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
          preview {
            image {
              url
            }
          }
        }
      }
    }
  }
`;

const fileDeleteTypedGidCreateMutation = `#graphql
  mutation MediaFileDeleteTypedGidRoundtripCreate($files: [FileCreateInput!]!) {
    fileCreate(files: $files) {
      files {
        id
        alt
        createdAt
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

function buildFileCreateThenImageReadVariables(runId: string): GraphqlVariables {
  return {
    files: [
      {
        contentType: 'IMAGE',
        originalSource: 'https://placehold.co/600x400/png',
        alt: `Hermes file image read ${runId}`,
      },
    ],
  };
}

function buildTypedDeleteCreateVariables(runId: string): GraphqlVariables {
  return {
    files: [
      {
        contentType: 'IMAGE',
        originalSource: 'https://placehold.co/600x400/png',
        alt: `Hermes typed delete actual ${runId}`,
      },
      {
        contentType: 'IMAGE',
        originalSource: 'https://placehold.co/600x400/png',
        alt: `Hermes typed delete wrong type ${runId}`,
      },
    ],
  };
}

function buildFileUpdateVariables(fileId: string): GraphqlVariables {
  return {
    files: [
      {
        id: fileId,
        alt: 'Hermes Files API updated alt',
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

async function waitForReadyFile(fileId: string): Promise<GraphqlPayload<FileReadData>> {
  let lastPayload: GraphqlPayload<FileReadData> | null = null;

  for (let attempt = 0; attempt < 15; attempt += 1) {
    lastPayload = await runGraphql<FileReadData>(fileReadQuery, { id: fileId });
    if (lastPayload.data?.node?.fileStatus === 'READY') {
      return lastPayload;
    }

    await delay(2000);
  }

  throw new Error(`Timed out waiting for file ${fileId} to reach READY: ${JSON.stringify(lastPayload, null, 2)}`);
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
let createdFileId: string | null = null;
let imageReadFileId: string | null = null;
let typedDeleteActualFileId: string | null = null;
let typedDeleteWrongTypeFileId: string | null = null;
let productId: string | null = null;
let productMediaId: string | null = null;

try {
  const createVariables = buildFileCreateVariables(runId);
  const createResponse = await runGraphql<FileCreateData>(fileCreateMutation, createVariables);
  expectNoUserErrors('fileCreate', createResponse.data?.fileCreate?.userErrors);
  createdFileId = requireId('fileCreate.files[0]', createResponse.data?.fileCreate?.files?.[0]?.id);

  const readyFileRead = await waitForReadyFile(createdFileId);
  const updateVariables = buildFileUpdateVariables(createdFileId);
  const updateResponse = await runGraphql<FileUpdateData>(fileUpdateMutation, updateVariables);
  expectNoUserErrors('fileUpdate', updateResponse.data?.fileUpdate?.userErrors);

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

  const imageReadVariables = buildFileCreateThenImageReadVariables(runId);
  const imageReadCreateResponse = await runGraphql<FileCreateData>(fileCreateThenImageReadMutation, imageReadVariables);
  expectNoUserErrors('fileCreate (image read scenario)', imageReadCreateResponse.data?.fileCreate?.userErrors);
  imageReadFileId = requireId(
    'fileCreate image read files[0]',
    imageReadCreateResponse.data?.fileCreate?.files?.[0]?.id,
  );
  const imageReadAfterCreateResponse = await runGraphql<FilesReadData>(filesAfterCreateReadQuery);
  const imageReadDeleteVariables = { fileIds: [imageReadFileId] };
  const imageReadDeleteResponse = await runGraphql<FileDeleteData>(fileDeleteMutation, imageReadDeleteVariables);
  expectNoUserErrors('fileDelete (image read cleanup)', imageReadDeleteResponse.data?.fileDelete?.userErrors);

  const typedDeleteCreateVariables = buildTypedDeleteCreateVariables(runId);
  const typedDeleteCreateResponse = await runGraphql<FileCreateData>(
    fileDeleteTypedGidCreateMutation,
    typedDeleteCreateVariables,
  );
  expectNoUserErrors('fileCreate (typed delete scenario)', typedDeleteCreateResponse.data?.fileCreate?.userErrors);
  typedDeleteActualFileId = requireId(
    'typed delete files[0]',
    typedDeleteCreateResponse.data?.fileCreate?.files?.[0]?.id,
  );
  typedDeleteWrongTypeFileId = requireId(
    'typed delete files[1]',
    typedDeleteCreateResponse.data?.fileCreate?.files?.[1]?.id,
  );
  const typedDeleteActualVariables = { fileIds: [typedDeleteActualFileId] };
  const typedDeleteActualResponse = await runGraphql<FileDeleteData>(fileDeleteMutation, typedDeleteActualVariables);
  expectNoUserErrors('fileDelete (typed delete actual gid)', typedDeleteActualResponse.data?.fileDelete?.userErrors);
  const typedDeleteWrongTypeVariables = {
    fileIds: [typedDeleteWrongTypeFileId.replace('/MediaImage/', '/Video/')],
  };
  const typedDeleteWrongTypeResponse = await runGraphql<FileDeleteData>(
    fileDeleteMutation,
    typedDeleteWrongTypeVariables,
  );
  expectNoUserErrors(
    'fileDelete (typed delete wrong type gid)',
    typedDeleteWrongTypeResponse.data?.fileDelete?.userErrors,
  );

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
    'file-update-parity.json': {
      setup: {
        createMutation: {
          variables: createVariables,
          response: createResponse,
        },
        readyFileRead,
      },
      mutation: {
        variables: updateVariables,
        response: updateResponse,
      },
      cleanup: {
        deleteMutation: {
          variables: deleteCreatedVariables,
          response: deleteCreatedResponse,
        },
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
    'media-file-create-then-image-read.json': {
      create: {
        variables: imageReadVariables,
        response: imageReadCreateResponse,
      },
      readAfterCreate: {
        response: imageReadAfterCreateResponse,
      },
      cleanup: {
        variables: imageReadDeleteVariables,
        response: imageReadDeleteResponse,
      },
      upstreamCalls: [],
    },
    'media-file-delete-typed-gid-roundtrip.json': {
      create: {
        variables: typedDeleteCreateVariables,
        response: typedDeleteCreateResponse,
      },
      deleteActual: {
        variables: typedDeleteActualVariables,
        response: typedDeleteActualResponse,
      },
      deleteWrongType: {
        variables: typedDeleteWrongTypeVariables,
        response: typedDeleteWrongTypeResponse,
      },
      upstreamCalls: [],
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
        imageReadFileId,
        typedDeleteActualFileId,
        typedDeleteWrongTypeFileId,
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

  if (imageReadFileId) {
    try {
      await runGraphql<ProductDeleteData>(fileDeleteMutation, { fileIds: [imageReadFileId] });
    } catch {
      // Best-effort cleanup only.
    }
  }

  if (typedDeleteActualFileId) {
    try {
      await runGraphql<ProductDeleteData>(fileDeleteMutation, { fileIds: [typedDeleteActualFileId] });
    } catch {
      // Best-effort cleanup only.
    }
  }

  if (typedDeleteWrongTypeFileId) {
    try {
      await runGraphql<ProductDeleteData>(fileDeleteMutation, { fileIds: [typedDeleteWrongTypeFileId] });
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
