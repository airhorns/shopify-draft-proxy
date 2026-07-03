/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type UserError = { field?: string[] | null; message?: string | null; code?: string | null };
type GraphqlPayload<TData = unknown> = {
  data?: TData;
  errors?: unknown;
  extensions?: unknown;
};
type FileNode = {
  id?: string | null;
  alt?: string | null;
  createdAt?: string | null;
  fileStatus?: string | null;
};
type FileCreateData = {
  fileCreate?: {
    files?: Array<FileNode | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileUpdateData = {
  fileUpdate?: {
    files?: Array<FileNode | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileDeleteData = {
  fileDelete?: {
    deletedFileIds?: string[] | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileAcknowledgeData = {
  fileAcknowledgeUpdateFailed?: {
    files?: Array<FileNode | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};
type ProductCreateData = {
  productCreate?: {
    product?: { id?: string | null; title?: string | null } | null;
    userErrors?: UserError[] | null;
  } | null;
};
type ProductDeleteData = {
  productDelete?: {
    deletedProductId?: string | null;
    userErrors?: UserError[] | null;
  } | null;
};
type RecordedUpstreamCall = {
  operationName: string;
  variables: JsonRecord;
  query: string;
  response: {
    status: number;
    body: GraphqlPayload;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: JsonRecord) => Promise<GraphqlPayload<TData>>;
  runGraphqlRequest: <TData>(query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlResult<TData>>;
};

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const requestDir = path.join('config', 'parity-requests', 'media');
const specDir = path.join('config', 'parity-specs', 'media');
const sourceImageUrl = 'https://placehold.co/600x400.jpg';
const replacementImageUrl = 'https://placehold.co/800x600.jpg';
const runId = `${Date.now()}`;

const filesUploadCreateDocument = `mutation FilesUploadLocalRuntimeCreate($files: [FileCreateInput!]!) {
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

const filesUploadReadDocument = `query FilesUploadLocalRuntimeRead {
  files(first: 2, reverse: true, sortKey: ID) {
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
    edges {
      cursor
      node {
        id
        alt
      }
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
  fileSavedSearches(first: 5) {
    nodes {
      id
      name
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}
`;

const filesUploadReadAfterDocument = `query FilesUploadLocalRuntimeReadPageTwo($after: String!) {
  files(first: 1, after: $after, reverse: true, sortKey: ID) {
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
    edges {
      cursor
      node {
        id
        alt
      }
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}
`;

const stagedUploadsCreateDocument = `mutation FilesUploadLocalRuntimeStagedUpload($input: [StagedUploadInput!]!) {
  stagedUploadsCreate(input: $input) {
    stagedTargets {
      url
      resourceUrl
      parameters {
        name
        value
      }
    }
    userErrors {
      field
      message
    }
  }
}
`;

const fileUpdateDocument = `mutation FileUpdateParity($files: [FileUpdateInput!]!) {
  fileUpdate(files: $files) {
    files {
      id
      alt
      fileStatus
      ... on MediaImage {
        image {
          url
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

const acknowledgeCreateDocument = `mutation MediaFileAcknowledgeUpdateFailedSemanticsCreate($files: [FileCreateInput!]!) {
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

const acknowledgeDocument = `mutation MediaFileAcknowledgeUpdateFailedSemanticsAck($fileIds: [ID!]!) {
  fileAcknowledgeUpdateFailed(fileIds: $fileIds) {
    files {
      id
      alt
      fileStatus
      __typename
      ... on MediaImage {
        image {
          url
        }
        mediaErrors {
          code
          message
        }
        mediaWarnings {
          code
          message
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

const acknowledgeReadDocument = `query MediaFileAcknowledgeUpdateFailedSemanticsRead {
  files(first: 1, reverse: true) {
    nodes {
      id
      alt
      createdAt
      fileStatus
      __typename
      ... on MediaImage {
        mediaErrors {
          code
          message
        }
        mediaWarnings {
          code
          message
        }
        image {
          url
          width
          height
        }
      }
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}
`;

const productReferenceCreateDocument = `mutation FileReferenceCreate($files: [FileCreateInput!]!) {
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

const productReferenceAttachDocument = `mutation FileReferenceAttach($files: [FileUpdateInput!]!) {
  fileUpdate(files: $files) {
    files {
      id
      alt
      fileStatus
      ... on MediaImage {
        image {
          url
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

const productReferenceFilesReadDocument = `query FileReferenceFilesRead {
  files(first: 1, reverse: true) {
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
      }
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}
`;

const productReferenceProductReadDocument = `query FileReferenceProductRead($productId: ID!) {
  product(id: $productId) {
    id
    title
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
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
}
`;

const fileAcknowledgeReadDocument = `query FileAcknowledgeUpdateFailedDownstreamRead {
  files(first: 1, reverse: true) {
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
      }
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}
`;

const productCreateDocument = `mutation MediaRetiredReplacementProductCreate($product: ProductCreateInput!) {
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

const productDeleteDocument = `mutation MediaRetiredReplacementProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

const fileDeleteDocument = `mutation MediaRetiredReplacementFileDelete($fileIds: [ID!]!) {
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

function expectNoTopLevelErrors(label: string, payload: GraphqlPayload): void {
  if (payload.errors === undefined) return;
  throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload.errors, null, 2)}`);
}

function expectNoUserErrors(label: string, errors: UserError[] | null | undefined): void {
  if (Array.isArray(errors) && errors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(errors ?? null, null, 2)}`);
}

function requireUserErrorCode(label: string, errors: UserError[] | null | undefined, code: string): void {
  if (Array.isArray(errors) && errors.some((error) => error?.code === code)) return;
  throw new Error(`${label} did not include ${code}: ${JSON.stringify(errors ?? null, null, 2)}`);
}

function requireString(value: unknown, label: string): string {
  if (typeof value === 'string' && value.length > 0) return value;
  throw new Error(`Missing ${label}: ${JSON.stringify(value ?? null, null, 2)}`);
}

function createdFileId(label: string, payload: GraphqlPayload<FileCreateData>): string {
  return requireString(payload.data?.fileCreate?.files?.[0]?.id, `${label} file id`);
}

function createdProductId(label: string, payload: GraphqlPayload<ProductCreateData>): string {
  return requireString(payload.data?.productCreate?.product?.id, `${label} product id`);
}

async function writeText(relativePath: string, body: string): Promise<void> {
  await mkdir(path.dirname(relativePath), { recursive: true });
  await writeFile(relativePath, body, 'utf8');
}

async function writeJson(relativePath: string, body: unknown): Promise<void> {
  await writeText(relativePath, `${JSON.stringify(body, null, 2)}\n`);
}

async function recordUpstreamCall(
  operationName: string,
  query: string,
  variables: JsonRecord,
): Promise<RecordedUpstreamCall> {
  const response = await runGraphqlRequest(query, variables);
  if (response.status < 200 || response.status >= 300) {
    throw new Error(`${operationName} returned HTTP ${response.status}: ${JSON.stringify(response.payload, null, 2)}`);
  }

  return {
    operationName,
    variables,
    query,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

async function run<TData>(label: string, query: string, variables: JsonRecord = {}): Promise<GraphqlPayload<TData>> {
  const response = await runGraphql<TData>(query, variables);
  expectNoTopLevelErrors(label, response);
  return response;
}

function createSpecExpectedDifferences() {
  return [
    {
      path: '$.files[*].id',
      matcher: 'shopify-gid:MediaImage',
      reason: 'The proxy generates a local MediaImage GID; Shopify returns the dev-store MediaImage GID.',
    },
    {
      path: '$.files[*].createdAt',
      matcher: 'iso-timestamp',
      reason: 'The proxy uses deterministic staged timestamps; Shopify returns the live creation timestamp.',
    },
    {
      path: '$.files[*].image',
      ignore: true,
      regrettable: true,
      reason: 'Shopify image materialization is asynchronous; the proxy keeps the accepted source URL immediately.',
    },
    {
      path: '$.files[*].preview',
      ignore: true,
      regrettable: true,
      reason: 'Shopify preview materialization is asynchronous; the proxy keeps the accepted source URL immediately.',
    },
  ];
}

function readNodeExpectedDifferences(root = '$') {
  return [
    {
      path: `${root}.id`,
      matcher: 'shopify-gid:MediaImage',
      reason: 'The proxy generates a local MediaImage GID; Shopify returns the dev-store MediaImage GID.',
    },
    {
      path: `${root}.createdAt`,
      matcher: 'iso-timestamp',
      reason: 'The proxy uses deterministic staged timestamps; Shopify returns the live creation timestamp.',
    },
    {
      path: `${root}.fileStatus`,
      ignore: true,
      regrettable: true,
      reason:
        'Shopify media processing status can advance asynchronously; the proxy keeps deterministic staged status.',
    },
    {
      path: `${root}.image`,
      ignore: true,
      regrettable: true,
      reason: 'Shopify image materialization is asynchronous; the proxy keeps the accepted source URL immediately.',
    },
    {
      path: `${root}.preview`,
      ignore: true,
      regrettable: true,
      reason: 'Shopify preview materialization is asynchronous; the proxy keeps the accepted source URL immediately.',
    },
  ];
}

function readConnectionExpectedDifferences() {
  return [
    ...readNodeExpectedDifferences('$.nodes[*]'),
    {
      path: '$.edges[*].cursor',
      matcher: 'non-empty-string',
      reason: 'Shopify and the proxy use different opaque cursor encodings.',
    },
    {
      path: '$.edges[*].node.id',
      matcher: 'shopify-gid:MediaImage',
      reason: 'The proxy generates local MediaImage GIDs; Shopify returns dev-store MediaImage GIDs.',
    },
    {
      path: '$.pageInfo.startCursor',
      matcher: 'non-empty-string',
      reason: 'Shopify and the proxy use different opaque cursor encodings.',
    },
    {
      path: '$.pageInfo.endCursor',
      matcher: 'non-empty-string',
      reason: 'Shopify and the proxy use different opaque cursor encodings.',
    },
  ];
}

function acknowledgeMessageDifference() {
  return {
    path: '$.userErrors[0].message',
    matcher: 'regex:^File with id gid://shopify/MediaImage/.+ is not in the READY state\\.$',
    reason: 'The message embeds the file id; the proxy emits its synthetic GID and Shopify emits the live GID.',
  };
}

function stagedUploadExpectedDifferences() {
  return [
    {
      path: '$.stagedTargets[0].url',
      matcher: 'non-empty-string',
      reason: 'Shopify returns a signed storage URL; the proxy returns an inert local placeholder URL.',
    },
    {
      path: '$.stagedTargets[0].resourceUrl',
      matcher: 'non-empty-string',
      reason:
        'Shopify returns a signed storage resource URL; the proxy returns an inert local placeholder resource URL.',
    },
    {
      path: '$.stagedTargets[0].parameters[3].value',
      matcher: 'non-empty-string',
      reason: 'The storage key embeds live storage path details; the proxy returns an inert local upload key.',
    },
    {
      path: '$.stagedTargets[0].parameters[4].value',
      matcher: 'non-empty-string',
      reason: 'The date value is generated by Shopify storage signing; the proxy returns a deterministic placeholder.',
    },
    {
      path: '$.stagedTargets[0].parameters[5].value',
      matcher: 'non-empty-string',
      reason:
        'The credential value is generated by Shopify storage signing; the proxy returns a deterministic placeholder.',
    },
    {
      path: '$.stagedTargets[0].parameters[7].value',
      matcher: 'non-empty-string',
      reason:
        'The signature value is generated by Shopify storage signing; the proxy returns a deterministic placeholder.',
    },
    {
      path: '$.stagedTargets[0].parameters[8].value',
      matcher: 'non-empty-string',
      reason:
        'The policy value is generated by Shopify storage signing; the proxy returns a deterministic placeholder.',
    },
  ];
}

function runtimeTestFiles(): string[] {
  return ['tests/graphql_routes/marketing_inventory_online_store.rs'];
}

const createdFileIds: string[] = [];
const createdProductIds: string[] = [];

try {
  const filesUploadFilename = `media-replacement-files-upload-${runId}.jpg`;
  const filesUploadCreateVariables = {
    files: [
      {
        alt: 'Media parity replacement file page one',
        contentType: 'IMAGE',
        filename: filesUploadFilename,
        originalSource: sourceImageUrl,
      },
      {
        alt: 'Media parity replacement file page two',
        contentType: 'IMAGE',
        filename: `media-replacement-files-upload-page-two-${runId}.jpg`,
        originalSource: sourceImageUrl,
      },
      {
        alt: 'Media parity replacement file page three',
        contentType: 'IMAGE',
        filename: `media-replacement-files-upload-page-three-${runId}.jpg`,
        originalSource: sourceImageUrl,
      },
    ],
  };
  const filesUploadCreate = await run<FileCreateData>(
    'files upload fileCreate',
    filesUploadCreateDocument,
    filesUploadCreateVariables,
  );
  expectNoUserErrors('files upload fileCreate', filesUploadCreate.data?.fileCreate?.userErrors);
  const filesUploadFileId = createdFileId('files upload fileCreate', filesUploadCreate);
  for (const file of filesUploadCreate.data?.fileCreate?.files ?? []) {
    if (file?.id) createdFileIds.push(file.id);
  }
  const filesUploadReadVariables = {};
  const filesUploadRead = await run('files upload readAfterCreate', filesUploadReadDocument, filesUploadReadVariables);
  const filesUploadReadAfterCursor = requireString(
    (filesUploadRead.data as { files?: { pageInfo?: { endCursor?: unknown } } } | undefined)?.files?.pageInfo
      ?.endCursor,
    'files upload readAfterCreate endCursor',
  );
  const filesUploadReadPageTwoVariables = { after: filesUploadReadAfterCursor };
  const filesUploadReadPageTwo = await run(
    'files upload readAfterCreate page two',
    filesUploadReadAfterDocument,
    filesUploadReadPageTwoVariables,
  );
  const stagedUploadsVariables = {
    input: [
      {
        filename: 'safe-upload.txt',
        mimeType: 'text/plain',
        resource: 'FILE',
        httpMethod: 'POST',
      },
    ],
  };
  const stagedUploadsCreate = await run(
    'files upload stagedUploadsCreate',
    stagedUploadsCreateDocument,
    stagedUploadsVariables,
  );
  const filesUploadConflictVariables = {
    files: [
      {
        id: filesUploadFileId,
        originalSource: replacementImageUrl,
        previewImageSource: sourceImageUrl,
      },
    ],
  };
  const filesUploadConflict = await run<FileUpdateData>(
    'files upload conflicting fileUpdate',
    fileUpdateDocument,
    filesUploadConflictVariables,
  );
  requireUserErrorCode(
    'files upload conflicting fileUpdate',
    filesUploadConflict.data?.fileUpdate?.userErrors,
    'NON_READY_STATE',
  );

  const acknowledgeFilename = `media-replacement-ack-semantics-${runId}.jpg`;
  const acknowledgeCreateVariables = {
    files: [
      {
        alt: 'Media parity acknowledgement semantics',
        contentType: 'IMAGE',
        filename: acknowledgeFilename,
        originalSource: sourceImageUrl,
      },
    ],
  };
  const acknowledgeCreate = await run<FileCreateData>(
    'acknowledge semantics fileCreate',
    acknowledgeCreateDocument,
    acknowledgeCreateVariables,
  );
  expectNoUserErrors('acknowledge semantics fileCreate', acknowledgeCreate.data?.fileCreate?.userErrors);
  const acknowledgeFileId = createdFileId('acknowledge semantics fileCreate', acknowledgeCreate);
  createdFileIds.push(acknowledgeFileId);
  const acknowledgeVariables = { fileIds: [acknowledgeFileId] };
  const acknowledgeNonReady = await run<FileAcknowledgeData>(
    'acknowledge semantics non-ready acknowledge',
    acknowledgeDocument,
    acknowledgeVariables,
  );
  requireUserErrorCode(
    'acknowledge semantics non-ready acknowledge',
    acknowledgeNonReady.data?.fileAcknowledgeUpdateFailed?.userErrors,
    'NON_READY_STATE',
  );
  const acknowledgeReadVariables = {};
  const acknowledgeRead = await run('acknowledge semantics readAfterNonReady', acknowledgeReadDocument);

  const productCreateVariables = {
    product: {
      title: `Media parity file reference target ${runId}`,
      status: 'ACTIVE',
    },
  };
  const productCreate = await run<ProductCreateData>(
    'product reference productCreate',
    productCreateDocument,
    productCreateVariables,
  );
  expectNoUserErrors('product reference productCreate', productCreate.data?.productCreate?.userErrors);
  const productId = createdProductId('product reference productCreate', productCreate);
  createdProductIds.push(productId);
  const productHydrate = await recordUpstreamCall('MediaProductHydrate', mediaProductHydrateQuery, { id: productId });
  const productReferenceFilename = `media-replacement-product-reference-${runId}.jpg`;
  const productReferenceCreateVariables = {
    files: [
      {
        alt: 'Media parity product reference source',
        contentType: 'IMAGE',
        filename: productReferenceFilename,
        originalSource: sourceImageUrl,
      },
    ],
  };
  const productReferenceCreate = await run<FileCreateData>(
    'product reference fileCreate',
    productReferenceCreateDocument,
    productReferenceCreateVariables,
  );
  expectNoUserErrors('product reference fileCreate', productReferenceCreate.data?.fileCreate?.userErrors);
  const productReferenceFileId = createdFileId('product reference fileCreate', productReferenceCreate);
  createdFileIds.push(productReferenceFileId);
  const productReferenceAttachVariables = {
    files: [
      {
        id: productReferenceFileId,
        alt: 'Media parity product reference attached',
        originalSource: replacementImageUrl,
        referencesToAdd: [productId],
      },
    ],
  };
  const productReferenceAttach = await run<FileUpdateData>(
    'product reference fileUpdate',
    productReferenceAttachDocument,
    productReferenceAttachVariables,
  );
  requireUserErrorCode(
    'product reference fileUpdate',
    productReferenceAttach.data?.fileUpdate?.userErrors,
    'NON_READY_STATE',
  );
  const productReadAfterAttach = await run('product reference product read', productReferenceProductReadDocument, {
    productId,
  });
  const productReferenceFilesReadVariables = {};
  const productReferenceFilesRead = await run(
    'product reference files read',
    productReferenceFilesReadDocument,
    productReferenceFilesReadVariables,
  );

  const fileAcknowledgeFilename = `media-replacement-file-acknowledge-${runId}.jpg`;
  const fileAcknowledgeCreateVariables = {
    files: [
      {
        alt: 'Media parity file acknowledgement source',
        contentType: 'IMAGE',
        filename: fileAcknowledgeFilename,
        originalSource: sourceImageUrl,
      },
    ],
  };
  const fileAcknowledgeCreate = await run<FileCreateData>(
    'file acknowledge fileCreate',
    acknowledgeCreateDocument,
    fileAcknowledgeCreateVariables,
  );
  expectNoUserErrors('file acknowledge fileCreate', fileAcknowledgeCreate.data?.fileCreate?.userErrors);
  const fileAcknowledgeFileId = createdFileId('file acknowledge fileCreate', fileAcknowledgeCreate);
  createdFileIds.push(fileAcknowledgeFileId);
  const fileAcknowledgeUpdateVariables = {
    files: [
      {
        id: fileAcknowledgeFileId,
        alt: 'Media parity file acknowledgement ready attempt',
        originalSource: replacementImageUrl,
      },
    ],
  };
  const fileAcknowledgeUpdate = await run<FileUpdateData>(
    'file acknowledge fileUpdate',
    fileUpdateDocument,
    fileAcknowledgeUpdateVariables,
  );
  requireUserErrorCode(
    'file acknowledge fileUpdate',
    fileAcknowledgeUpdate.data?.fileUpdate?.userErrors,
    'NON_READY_STATE',
  );
  const fileAcknowledgeVariables = { fileIds: [fileAcknowledgeFileId] };
  const fileAcknowledge = await run<FileAcknowledgeData>(
    'file acknowledge update failed',
    acknowledgeDocument,
    fileAcknowledgeVariables,
  );
  requireUserErrorCode(
    'file acknowledge update failed',
    fileAcknowledge.data?.fileAcknowledgeUpdateFailed?.userErrors,
    'NON_READY_STATE',
  );
  const fileAcknowledgeReadVariables = {};
  const fileAcknowledgeRead = await run(
    'file acknowledge read after acknowledge',
    fileAcknowledgeReadDocument,
    fileAcknowledgeReadVariables,
  );

  const capturedAt = new Date().toISOString();
  const filesUploadFixturePath = path.join(fixtureDir, 'files-upload-live-capture.json');
  const acknowledgeFixturePath = path.join(fixtureDir, 'media-file-acknowledge-update-failed-semantics.json');
  const productReferenceFixturePath = path.join(fixtureDir, 'file-update-product-reference-local-staging.json');
  const fileAcknowledgeFixturePath = path.join(fixtureDir, 'file-acknowledge-update-failed-local-staging.json');

  await writeJson(filesUploadFixturePath, {
    capturedAt,
    storeDomain,
    apiVersion,
    scenarioId: 'files-upload-local-runtime',
    create: { variables: filesUploadCreateVariables, response: filesUploadCreate },
    readAfterCreate: { variables: filesUploadReadVariables, response: filesUploadRead },
    readAfterCreatePageTwo: { variables: filesUploadReadPageTwoVariables, response: filesUploadReadPageTwo },
    stagedUploadsCreate: { variables: stagedUploadsVariables, response: stagedUploadsCreate },
    fileUpdateConflictingSources: { variables: filesUploadConflictVariables, response: filesUploadConflict },
    upstreamCalls: [],
  });
  await writeJson(acknowledgeFixturePath, {
    capturedAt,
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-acknowledge-update-failed-semantics',
    create: { variables: acknowledgeCreateVariables, response: acknowledgeCreate },
    acknowledgeNonReady: { variables: acknowledgeVariables, response: acknowledgeNonReady },
    readAfterNonReady: { variables: acknowledgeReadVariables, response: acknowledgeRead },
    upstreamCalls: [],
  });
  await writeJson(productReferenceFixturePath, {
    capturedAt,
    storeDomain,
    apiVersion,
    scenarioId: 'file-update-product-reference-local-staging',
    setup: {
      productCreate: { variables: productCreateVariables, response: productCreate },
    },
    create: { variables: productReferenceCreateVariables, response: productReferenceCreate },
    attach: { variables: productReferenceAttachVariables, response: productReferenceAttach },
    productReadAfterAttach: { variables: { productId }, response: productReadAfterAttach },
    filesReadAfterAttach: { variables: productReferenceFilesReadVariables, response: productReferenceFilesRead },
    upstreamCalls: [productHydrate],
  });
  await writeJson(fileAcknowledgeFixturePath, {
    capturedAt,
    storeDomain,
    apiVersion,
    scenarioId: 'fileAcknowledgeUpdateFailed-local-staging',
    create: { variables: fileAcknowledgeCreateVariables, response: fileAcknowledgeCreate },
    updateToReady: { variables: fileAcknowledgeUpdateVariables, response: fileAcknowledgeUpdate },
    acknowledge: { variables: fileAcknowledgeVariables, response: fileAcknowledge },
    readAfterAcknowledge: { variables: fileAcknowledgeReadVariables, response: fileAcknowledgeRead },
    upstreamCalls: [],
  });

  await writeText(path.join(requestDir, 'files-upload-local-runtime-create.graphql'), filesUploadCreateDocument);
  await writeText(path.join(requestDir, 'files-upload-local-runtime-read.graphql'), filesUploadReadDocument);
  await writeText(
    path.join(requestDir, 'files-upload-local-runtime-read-page-two.graphql'),
    filesUploadReadAfterDocument,
  );
  await writeText(
    path.join(requestDir, 'files-upload-local-runtime-staged-upload.graphql'),
    stagedUploadsCreateDocument,
  );
  await writeText(
    path.join(requestDir, 'media-file-acknowledge-update-failed-semantics-create.graphql'),
    acknowledgeCreateDocument,
  );
  await writeText(
    path.join(requestDir, 'media-file-acknowledge-update-failed-semantics-ack.graphql'),
    acknowledgeDocument,
  );
  await writeText(
    path.join(requestDir, 'media-file-acknowledge-update-failed-semantics-read.graphql'),
    acknowledgeReadDocument,
  );
  await writeText(path.join(requestDir, 'fileUpdate-product-reference-create.graphql'), productReferenceCreateDocument);
  await writeText(path.join(requestDir, 'fileUpdate-product-reference-attach.graphql'), productReferenceAttachDocument);
  await writeText(
    path.join(requestDir, 'fileUpdate-product-reference-files-read.graphql'),
    productReferenceFilesReadDocument,
  );
  await writeText(
    path.join(requestDir, 'fileUpdate-product-reference-product-read.graphql'),
    productReferenceProductReadDocument,
  );
  await writeText(path.join(requestDir, 'fileAcknowledgeUpdateFailed-parity.graphql'), acknowledgeDocument);
  await writeText(
    path.join(requestDir, 'fileAcknowledgeUpdateFailed-downstream-read.graphql'),
    fileAcknowledgeReadDocument,
  );

  await writeJson(path.join(specDir, 'files-upload-local-runtime.json'), {
    scenarioId: 'files-upload-local-runtime',
    operationNames: ['files', 'fileSavedSearches', 'fileCreate', 'fileUpdate', 'stagedUploadsCreate'],
    scenarioStatus: 'captured',
    assertionKinds: [
      'payload-shape',
      'empty-state-parity',
      'downstream-read-parity',
      'side-effect-boundary',
      'user-errors-parity',
    ],
    liveCaptureFiles: [filesUploadFixturePath],
    runtimeTestFiles: runtimeTestFiles(),
    proxyRequest: {
      documentPath: 'config/parity-requests/media/files-upload-local-runtime-create.graphql',
      variablesCapturePath: '$.create.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Replaces the former local-runtime evidence with live Shopify Admin GraphQL capture for fileCreate, immediate paginated files/fileSavedSearches reads, stagedUploadsCreate target metadata, and non-ready fileUpdate validation.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'file-create-payload',
          capturePath: '$.create.response.data.fileCreate',
          proxyPath: '$.data.fileCreate',
          expectedDifferences: createSpecExpectedDifferences(),
        },
        {
          name: 'files-read-after-create-page-one',
          capturePath: '$.readAfterCreate.response.data.files',
          proxyPath: '$.data.files',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/files-upload-local-runtime-read.graphql',
            variablesCapturePath: '$.readAfterCreate.variables',
            apiVersion,
          },
          expectedDifferences: readConnectionExpectedDifferences(),
        },
        {
          name: 'files-read-after-create-page-two',
          capturePath: '$.readAfterCreatePageTwo.response.data.files',
          proxyPath: '$.data.files',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/files-upload-local-runtime-read-page-two.graphql',
            variables: {
              after: {
                fromProxyResponse: 'files-read-after-create-page-one',
                path: '$.data.files.pageInfo.endCursor',
              },
            },
            apiVersion,
          },
          expectedDifferences: [
            ...readConnectionExpectedDifferences(),
            {
              path: '$.pageInfo.hasNextPage',
              ignore: true,
              regrettable: true,
              reason:
                'The disposable live store can contain older media beyond this scenario; the proxy only knows the staged scenario files.',
            },
          ],
        },
        {
          name: 'file-saved-searches-empty',
          capturePath: '$.readAfterCreate.response.data.fileSavedSearches',
          proxyPath: '$.data.fileSavedSearches',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/files-upload-local-runtime-read.graphql',
            variablesCapturePath: '$.readAfterCreate.variables',
            apiVersion,
          },
        },
        {
          name: 'staged-upload-target-metadata',
          capturePath: '$.stagedUploadsCreate.response.data.stagedUploadsCreate',
          proxyPath: '$.data.stagedUploadsCreate',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/files-upload-local-runtime-staged-upload.graphql',
            variablesCapturePath: '$.stagedUploadsCreate.variables',
            apiVersion,
          },
          expectedDifferences: stagedUploadExpectedDifferences(),
        },
        {
          name: 'file-update-conflicting-sources-user-error',
          capturePath: '$.fileUpdateConflictingSources.response.data.fileUpdate',
          proxyPath: '$.data.fileUpdate',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/fileUpdate-parity.graphql',
            variables: {
              files: [
                {
                  id: { fromPrimaryProxyPath: '$.data.fileCreate.files[0].id' },
                  originalSource: replacementImageUrl,
                  previewImageSource: sourceImageUrl,
                },
              ],
            },
            apiVersion,
          },
        },
      ],
    },
  });

  await writeJson(path.join(specDir, 'media-file-acknowledge-update-failed-semantics.json'), {
    scenarioId: 'media-file-acknowledge-update-failed-semantics',
    operationNames: ['fileCreate', 'fileAcknowledgeUpdateFailed', 'files'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'downstream-read-parity', 'user-errors-parity'],
    liveCaptureFiles: [acknowledgeFixturePath],
    runtimeTestFiles: runtimeTestFiles(),
    proxyRequest: {
      documentPath: 'config/parity-requests/media/media-file-acknowledge-update-failed-semantics-create.graphql',
      variablesCapturePath: '$.create.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify capture for acknowledging a freshly-created non-ready file plus the immediate files read that exposes no-data mediaErrors/mediaWarnings lists.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'create-uploaded-file',
          capturePath: '$.create.response.data.fileCreate',
          proxyPath: '$.data.fileCreate',
          expectedDifferences: createSpecExpectedDifferences(),
        },
        {
          name: 'acknowledge-non-ready',
          capturePath: '$.acknowledgeNonReady.response.data.fileAcknowledgeUpdateFailed',
          proxyPath: '$.data.fileAcknowledgeUpdateFailed',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/media-file-acknowledge-update-failed-semantics-ack.graphql',
            variables: {
              fileIds: [{ fromPrimaryProxyPath: '$.data.fileCreate.files[0].id' }],
            },
            apiVersion,
          },
          expectedDifferences: [acknowledgeMessageDifference()],
        },
        {
          name: 'files-read-after-non-ready-acknowledge',
          capturePath: '$.readAfterNonReady.response.data.files.nodes[0]',
          proxyPath: '$.data.files.nodes[0]',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/media-file-acknowledge-update-failed-semantics-read.graphql',
            variablesCapturePath: '$.readAfterNonReady.variables',
            apiVersion,
          },
          expectedDifferences: readNodeExpectedDifferences(),
        },
      ],
    },
  });

  await writeJson(path.join(specDir, 'fileUpdate-product-reference-local-staging.json'), {
    scenarioId: 'file-update-product-reference-local-staging',
    operationNames: ['fileCreate', 'fileUpdate', 'files', 'product'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'downstream-read-parity', 'side-effect-boundary'],
    liveCaptureFiles: [productReferenceFixturePath],
    runtimeTestFiles: runtimeTestFiles(),
    proxyRequest: {
      documentPath: 'config/parity-requests/media/fileUpdate-product-reference-create.graphql',
      variablesCapturePath: '$.create.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify capture for fileUpdate.referencesToAdd against an existing product while the file is still non-ready; the product remains unattached and downstream files preserve the staged file.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'file-create',
          capturePath: '$.create.response.data.fileCreate',
          proxyPath: '$.data.fileCreate',
          expectedDifferences: createSpecExpectedDifferences(),
        },
        {
          name: 'file-update-attach-product-reference',
          capturePath: '$.attach.response.data.fileUpdate',
          proxyPath: '$.data.fileUpdate',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/fileUpdate-product-reference-attach.graphql',
            variables: {
              files: [
                {
                  id: { fromPrimaryProxyPath: '$.data.fileCreate.files[0].id' },
                  alt: 'Media parity product reference attached',
                  originalSource: replacementImageUrl,
                  referencesToAdd: [
                    { fromCapturePath: '$.setup.productCreate.response.data.productCreate.product.id' },
                  ],
                },
              ],
            },
            apiVersion,
          },
        },
        {
          name: 'product-media-read-after-file-reference-attach',
          capturePath: '$.productReadAfterAttach.response.data.product',
          proxyPath: '$.data.product',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/fileUpdate-product-reference-product-read.graphql',
            variables: {
              productId: { fromCapturePath: '$.setup.productCreate.response.data.productCreate.product.id' },
            },
            apiVersion,
          },
        },
        {
          name: 'files-read-after-product-reference-attach',
          capturePath: '$.filesReadAfterAttach.response.data.files.nodes[0]',
          proxyPath: '$.data.files.nodes[0]',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/fileUpdate-product-reference-files-read.graphql',
            variablesCapturePath: '$.filesReadAfterAttach.variables',
            apiVersion,
          },
          expectedDifferences: readNodeExpectedDifferences(),
        },
      ],
    },
  });

  await writeJson(path.join(specDir, 'fileAcknowledgeUpdateFailed-local-staging.json'), {
    scenarioId: 'fileAcknowledgeUpdateFailed-local-staging',
    operationNames: ['fileCreate', 'fileUpdate', 'fileAcknowledgeUpdateFailed', 'files'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'downstream-read-parity', 'side-effect-boundary', 'user-errors-parity'],
    liveCaptureFiles: [fileAcknowledgeFixturePath],
    runtimeTestFiles: runtimeTestFiles(),
    proxyRequest: {
      documentPath: 'config/parity-requests/media/media-file-acknowledge-update-failed-semantics-create.graphql',
      variablesCapturePath: '$.create.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Live Shopify capture for the non-ready fileUpdate and fileAcknowledgeUpdateFailed validation branches plus immediate downstream files read.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'update-to-ready',
          capturePath: '$.updateToReady.response.data.fileUpdate',
          proxyPath: '$.data.fileUpdate',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/fileUpdate-parity.graphql',
            variables: {
              files: [
                {
                  id: { fromPrimaryProxyPath: '$.data.fileCreate.files[0].id' },
                  alt: 'Media parity file acknowledgement ready attempt',
                  originalSource: replacementImageUrl,
                },
              ],
            },
            apiVersion,
          },
        },
        {
          name: 'acknowledge-update-failed',
          capturePath: '$.acknowledge.response.data.fileAcknowledgeUpdateFailed',
          proxyPath: '$.data.fileAcknowledgeUpdateFailed',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/fileAcknowledgeUpdateFailed-parity.graphql',
            variables: {
              fileIds: [{ fromPrimaryProxyPath: '$.data.fileCreate.files[0].id' }],
            },
            apiVersion,
          },
          expectedDifferences: [acknowledgeMessageDifference()],
        },
        {
          name: 'files-read-after-acknowledge',
          capturePath: '$.readAfterAcknowledge.response.data.files.nodes[0]',
          proxyPath: '$.data.files.nodes[0]',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/fileAcknowledgeUpdateFailed-downstream-read.graphql',
            variablesCapturePath: '$.readAfterAcknowledge.variables',
            apiVersion,
          },
          expectedDifferences: readNodeExpectedDifferences(),
        },
      ],
    },
  });

  console.log(
    JSON.stringify(
      {
        ok: true,
        storeDomain,
        apiVersion,
        fixtures: [
          filesUploadFixturePath,
          acknowledgeFixturePath,
          productReferenceFixturePath,
          fileAcknowledgeFixturePath,
        ],
        createdFileIds,
        createdProductIds,
      },
      null,
      2,
    ),
  );
} finally {
  if (createdFileIds.length > 0) {
    try {
      await runGraphql<FileDeleteData>(fileDeleteDocument, { fileIds: createdFileIds });
    } catch {
      // Best-effort cleanup only.
    }
  }
  for (const productId of createdProductIds) {
    try {
      await runGraphql<ProductDeleteData>(productDeleteDocument, { input: { id: productId } });
    } catch {
      // Best-effort cleanup only.
    }
  }
}
