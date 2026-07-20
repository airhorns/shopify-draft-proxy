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
type FileNode = {
  id?: string | null;
  __typename?: string | null;
  alt?: string | null;
  createdAt?: string | null;
  fileStatus?: string | null;
  image?: { url?: string | null; width?: number | null; height?: number | null } | null;
  preview?: { image?: { url?: string | null; width?: number | null; height?: number | null } | null } | null;
};
type GraphqlPayload<TData> = {
  data?: TData;
  errors?: unknown;
  extensions?: unknown;
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
type FileReadData = { node?: FileNode | null };
type MixedFileReadData = {
  files?: { nodes?: Array<FileNode | null> | null } | null;
  nodes?: Array<FileNode | null> | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const outputFile = path.join(outputDir, 'media-file-update-mixed-batch-partial-success.json');
const paritySpecFile = path.join(
  'config',
  'parity-specs',
  'media',
  'media-file-update-mixed-batch-partial-success.json',
);
const createRequestFile = path.join(
  'config',
  'parity-requests',
  'media',
  'media-file-update-mixed-batch-create.graphql',
);
const updateRequestFile = path.join(
  'config',
  'parity-requests',
  'media',
  'media-file-update-mixed-batch-update.graphql',
);
const readRequestFile = path.join('config', 'parity-requests', 'media', 'media-file-update-mixed-batch-read.graphql');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload<TData>>;
};

const fileSelection = `#graphql
  id
  __typename
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
        width
        height
      }
    }
  }
`;

const nodeFileSelection = `#graphql
  id
  __typename
  ... on MediaImage {
    alt
    createdAt
    fileStatus
    image {
      url
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
  ... on GenericFile {
    alt
    createdAt
    fileStatus
    url
  }
  ... on Video {
    alt
    createdAt
    fileStatus
  }
  ... on ExternalVideo {
    alt
    createdAt
    fileStatus
  }
`;

const fileCreateMutation = `mutation MediaFileUpdateMixedBatchSeed($files: [FileCreateInput!]!) {
  fileCreate(files: $files) {
    files {
      id
      __typename
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
            width
            height
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
}`;

const fileUpdateMutation = `#graphql
  mutation MediaFileUpdateValidationBranches($files: [FileUpdateInput!]!) {
    fileUpdate(files: $files) {
      files {
        ${fileSelection}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const mixedFileUpdateMutation = `mutation MediaFileUpdateMixedBatch($files: [FileUpdateInput!]!) {
  fileUpdate(files: $files) {
    files {
      id
      __typename
      alt
      fileStatus
    }
    userErrors {
      field
      message
      code
    }
  }
}`;

const fileReadQuery = `#graphql
  query MediaFileUpdateValidationReadyPoll($id: ID!) {
    node(id: $id) {
      ${nodeFileSelection}
    }
  }
`;

const mixedFileReadQuery = `query MediaFileUpdateMixedBatchRead($ids: [ID!]!, $query: String!) {
  files(first: 2, query: $query, sortKey: FILENAME) {
    nodes {
      id
      __typename
      alt
      fileStatus
    }
  }
  nodes(ids: $ids) {
    id
    __typename
    ... on MediaImage {
      alt
      fileStatus
    }
  }
}`;

const fileDeleteMutation = `#graphql
  mutation MediaFileUpdateValidationCleanup($fileIds: [ID!]!) {
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

function expectNoUserErrors(label: string, errors: UserError[] | null | undefined): void {
  if (Array.isArray(errors) && errors.length === 0) {
    return;
  }

  throw new Error(`${label} returned userErrors: ${JSON.stringify(errors ?? null, null, 2)}`);
}

function requireId(label: string, node: FileNode | null | undefined): string {
  if (typeof node?.id === 'string' && node.id.length > 0) {
    return node.id;
  }

  throw new Error(`${label} did not return a file id: ${JSON.stringify(node ?? null, null, 2)}`);
}

function expectUserErrorCode(label: string, payload: GraphqlPayload<FileUpdateData>, code: string): void {
  const errors = payload.data?.fileUpdate?.userErrors ?? [];
  if (errors.some((error) => error?.code === code)) {
    return;
  }

  throw new Error(`${label} did not include ${code}: ${JSON.stringify(payload, null, 2)}`);
}

async function waitForReadyFile(fileId: string, label: string): Promise<GraphqlPayload<FileReadData>> {
  let lastPayload: GraphqlPayload<FileReadData> | null = null;

  for (let attempt = 0; attempt < 30; attempt += 1) {
    lastPayload = await runGraphql<FileReadData>(fileReadQuery, { id: fileId });
    if (lastPayload.data?.node?.fileStatus === 'READY') {
      return lastPayload;
    }

    await delay(2000);
  }

  throw new Error(`Timed out waiting for ${label} ${fileId} to reach READY: ${JSON.stringify(lastPayload, null, 2)}`);
}

async function waitForMixedFilesRead(
  variables: GraphqlVariables,
  expectedIds: string[],
): Promise<GraphqlPayload<MixedFileReadData>> {
  let lastPayload: GraphqlPayload<MixedFileReadData> | null = null;

  for (let attempt = 0; attempt < 30; attempt += 1) {
    lastPayload = await runGraphql<MixedFileReadData>(mixedFileReadQuery, variables);
    const fileIds = (lastPayload.data?.files?.nodes ?? [])
      .map((file) => file?.id)
      .filter((id): id is string => typeof id === 'string');
    const nodes = lastPayload.data?.nodes ?? [];
    if (
      fileIds.length === expectedIds.length &&
      expectedIds.every((id) => fileIds.includes(id)) &&
      nodes.length === expectedIds.length &&
      nodes.every((node, index) => node?.id === expectedIds[index] && node?.fileStatus === 'READY')
    ) {
      return lastPayload;
    }

    await delay(2000);
  }

  throw new Error(`Timed out waiting for mixed files read: ${JSON.stringify(lastPayload, null, 2)}`);
}

function tailGid(id: string, replacementType: string): string {
  const tail = id.split('/').pop();
  if (!tail) {
    throw new Error(`Cannot derive tail from ${id}`);
  }
  return `gid://shopify/${replacementType}/${tail}`;
}

function mediaFileIdDifference(jsonPath: string): { path: string; matcher: string; reason: string } {
  return {
    path: jsonPath,
    matcher: 'shopify-gid:MediaImage',
    reason: 'Shopify and the local proxy allocate different MediaImage ids for disposable setup files.',
  };
}

function mediaFileIdDifferences(): Array<{ path: string; matcher: string; reason: string }> {
  return [
    mediaFileIdDifference('$.files.nodes[0].id'),
    mediaFileIdDifference('$.files.nodes[1].id'),
    mediaFileIdDifference('$.nodes[0].id'),
    mediaFileIdDifference('$.nodes[1].id'),
  ];
}

const createdFileIds: string[] = [];
const timestamp = Date.now();

const imageCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `mixed-batch-${timestamp}-valid.jpg`,
      originalSource: 'https://placehold.co/600x400.jpg',
      alt: 'Mixed batch valid row before update',
    },
    {
      contentType: 'IMAGE',
      filename: `mixed-batch-${timestamp}-invalid.jpg`,
      originalSource: 'https://placehold.co/601x401.jpg',
      alt: 'Mixed batch invalid row before update',
    },
  ],
};
const videoCreateVariables = {
  files: [
    {
      contentType: 'EXTERNAL_VIDEO',
      originalSource: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
      alt: 'Validation seed video',
    },
  ],
};
const nonReadyCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `validation-non-ready-${timestamp}.jpg`,
      originalSource: 'https://placehold.co/600x400.jpg',
      alt: 'Validation non-ready image',
    },
  ],
};

let capture: Record<string, unknown> | null = null;

try {
  const imageCreate = await runGraphql<FileCreateData>(fileCreateMutation, imageCreateVariables);
  expectNoUserErrors('image fileCreate', imageCreate.data?.fileCreate?.userErrors);
  const imageId = requireId('image fileCreate', imageCreate.data?.fileCreate?.files?.[0]);
  const mixedInvalidImageId = requireId('second image fileCreate', imageCreate.data?.fileCreate?.files?.[1]);
  createdFileIds.push(imageId);
  createdFileIds.push(mixedInvalidImageId);

  const videoCreate = await runGraphql<FileCreateData>(fileCreateMutation, videoCreateVariables);
  expectNoUserErrors('video fileCreate', videoCreate.data?.fileCreate?.userErrors);
  const videoId = requireId('video fileCreate', videoCreate.data?.fileCreate?.files?.[0]);
  createdFileIds.push(videoId);

  const nonReadyCreate = await runGraphql<FileCreateData>(fileCreateMutation, nonReadyCreateVariables);
  expectNoUserErrors('non-ready fileCreate', nonReadyCreate.data?.fileCreate?.userErrors);
  const nonReadyImageId = requireId('non-ready fileCreate', nonReadyCreate.data?.fileCreate?.files?.[0]);
  createdFileIds.push(nonReadyImageId);

  const nonReadyAltVariables = {
    files: [{ id: nonReadyImageId, alt: 'Validation non-ready update attempt' }],
  };
  const nonReadyAlt = await runGraphql<FileUpdateData>(fileUpdateMutation, nonReadyAltVariables);
  expectUserErrorCode('non-ready alt update', nonReadyAlt, 'NON_READY_STATE');

  const readyImageRead = await waitForReadyFile(imageId, 'image');
  const readyMixedInvalidImageRead = await waitForReadyFile(mixedInvalidImageId, 'second image');
  const readyVideoRead = await waitForReadyFile(videoId, 'video');

  const mixedReadyReadVariables = {
    ids: [imageId, mixedInvalidImageId],
    query: `filename:mixed-batch-${timestamp}*`,
  };
  const mixedReadyRead = await waitForMixedFilesRead(mixedReadyReadVariables, [imageId, mixedInvalidImageId]);
  const mixedReadyNodes = mixedReadyRead.data?.nodes ?? [];
  if (
    mixedReadyNodes.length !== 2 ||
    mixedReadyNodes[0]?.id !== imageId ||
    mixedReadyNodes[0]?.fileStatus !== 'READY' ||
    mixedReadyNodes[1]?.id !== mixedInvalidImageId ||
    mixedReadyNodes[1]?.fileStatus !== 'READY'
  ) {
    throw new Error(`mixed fileUpdate setup files were not both READY: ${JSON.stringify(mixedReadyRead, null, 2)}`);
  }

  const mixedBatchVariables = {
    files: [
      { id: imageId, alt: 'Mixed batch valid row after update' },
      { id: mixedInvalidImageId, alt: 'x'.repeat(513) },
    ],
  };
  const mixedBatch = await runGraphql<FileUpdateData>(mixedFileUpdateMutation, mixedBatchVariables);
  const mixedBatchFiles = mixedBatch.data?.fileUpdate?.files ?? [];
  const mixedBatchErrors = mixedBatch.data?.fileUpdate?.userErrors ?? [];
  if (
    mixedBatchFiles.length !== 1 ||
    mixedBatchFiles[0]?.id !== imageId ||
    mixedBatchFiles[0]?.alt !== 'Mixed batch valid row after update' ||
    mixedBatchErrors.length !== 1 ||
    JSON.stringify(mixedBatchErrors[0]?.field) !== JSON.stringify(['files', '1', 'alt']) ||
    mixedBatchErrors[0]?.message !== 'The alt value exceeds the maximum limit of 512 characters.' ||
    mixedBatchErrors[0]?.code !== 'ALT_VALUE_LIMIT_EXCEEDED'
  ) {
    throw new Error(`mixed fileUpdate did not preserve per-item success: ${JSON.stringify(mixedBatch, null, 2)}`);
  }

  const mixedDownstreamRead = await runGraphql<MixedFileReadData>(mixedFileReadQuery, mixedReadyReadVariables);
  const mixedNodes = mixedDownstreamRead.data?.nodes ?? [];
  const mixedFilesById = new Map(
    (mixedDownstreamRead.data?.files?.nodes ?? [])
      .filter((file): file is FileNode => typeof file?.id === 'string')
      .map((file) => [file.id as string, file]),
  );
  if (
    mixedNodes[0]?.id !== imageId ||
    mixedNodes[0]?.alt !== 'Mixed batch valid row after update' ||
    mixedNodes[1]?.id !== mixedInvalidImageId ||
    mixedNodes[1]?.alt !== 'Mixed batch invalid row before update' ||
    mixedFilesById.get(imageId)?.alt !== 'Mixed batch valid row after update' ||
    mixedFilesById.get(mixedInvalidImageId)?.alt !== 'Mixed batch invalid row before update'
  ) {
    throw new Error(
      `mixed fileUpdate downstream read did not preserve the successful subset: ${JSON.stringify(mixedDownstreamRead, null, 2)}`,
    );
  }

  const videoOriginalSourceVariables = {
    files: [{ id: videoId, originalSource: 'https://cdn.example.com/validation-new-video.mp4' }],
  };
  const videoOriginalSource = await runGraphql<FileUpdateData>(fileUpdateMutation, videoOriginalSourceVariables);
  expectUserErrorCode('video originalSource update', videoOriginalSource, 'INVALID');

  const videoFilenameVariables = {
    files: [{ id: videoId, filename: `validation-renamed-${timestamp}.youtube` }],
  };
  const videoFilename = await runGraphql<FileUpdateData>(fileUpdateMutation, videoFilenameVariables);
  expectUserErrorCode('video filename update', videoFilename, 'UNSUPPORTED_MEDIA_TYPE_FOR_FILENAME_UPDATE');

  const imageFilenameMismatchVariables = {
    files: [{ id: imageId, filename: `mixed-batch-${timestamp}-valid.png` }],
  };
  const imageFilenameMismatch = await runGraphql<FileUpdateData>(fileUpdateMutation, imageFilenameMismatchVariables);
  expectUserErrorCode('image filename extension mismatch', imageFilenameMismatch, 'INVALID_FILENAME_EXTENSION');

  const missingReferenceIds = ['gid://shopify/Product/999999999991', 'gid://shopify/Product/999999999992'];
  const missingReferencesVariables = {
    files: [
      {
        id: imageId,
        referencesToAdd: [missingReferenceIds[0]],
        referencesToRemove: [missingReferenceIds[1]],
      },
    ],
  };
  const missingReferences = await runGraphql<FileUpdateData>(fileUpdateMutation, missingReferencesVariables);
  expectUserErrorCode('missing reference targets', missingReferences, 'REFERENCE_TARGET_DOES_NOT_EXIST');

  const wrongTypeVariables = {
    files: [{ id: tailGid(imageId, 'Video'), alt: 'Validation wrong typed id' }],
  };
  const wrongType = await runGraphql<FileUpdateData>(fileUpdateMutation, wrongTypeVariables);
  expectUserErrorCode('wrong typed gid update', wrongType, 'FILE_DOES_NOT_EXIST');

  const successAltVariables = {
    files: [{ id: imageId, alt: 'Validation successful alt update preserves READY' }],
  };
  const successAlt = await runGraphql<FileUpdateData>(fileUpdateMutation, successAltVariables);
  expectNoUserErrors('success alt update', successAlt.data?.fileUpdate?.userErrors);

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-update-mixed-batch-partial-success',
    setup: {
      imageCreate: { variables: imageCreateVariables, response: imageCreate },
      videoCreate: { variables: videoCreateVariables, response: videoCreate },
      nonReadyCreate: { variables: nonReadyCreateVariables, response: nonReadyCreate },
      readyImageRead,
      readyMixedInvalidImageRead,
      readyVideoRead,
      mixedReadyRead: { variables: mixedReadyReadVariables, response: mixedReadyRead },
    },
    mixedBatch: {
      mutation: { variables: mixedBatchVariables, response: mixedBatch },
      downstreamRead: { variables: mixedReadyReadVariables, response: mixedDownstreamRead },
    },
    branches: {
      nonReadyAlt: { variables: nonReadyAltVariables, response: nonReadyAlt },
      videoOriginalSource: { variables: videoOriginalSourceVariables, response: videoOriginalSource },
      videoFilename: { variables: videoFilenameVariables, response: videoFilename },
      imageFilenameMismatch: { variables: imageFilenameMismatchVariables, response: imageFilenameMismatch },
      missingReferences: { variables: missingReferencesVariables, response: missingReferences },
      wrongType: { variables: wrongTypeVariables, response: wrongType },
      successAlt: { variables: successAltVariables, response: successAlt },
    },
    upstreamCalls: [],
  };
} finally {
  let cleanup: GraphqlPayload<FileDeleteData> | null = null;
  if (createdFileIds.length > 0) {
    cleanup = await runGraphql<FileDeleteData>(fileDeleteMutation, { fileIds: createdFileIds });
  }
  if (capture) {
    capture['cleanup'] = {
      variables: { fileIds: createdFileIds },
      response: cleanup,
    };
    await mkdir(outputDir, { recursive: true });
    await mkdir(path.dirname(paritySpecFile), { recursive: true });
    await mkdir(path.dirname(createRequestFile), { recursive: true });
    await writeFile(outputFile, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
    await writeFile(createRequestFile, `${fileCreateMutation}\n`, 'utf8');
    await writeFile(updateRequestFile, `${mixedFileUpdateMutation}\n`, 'utf8');
    await writeFile(readRequestFile, `${mixedFileReadQuery}\n`, 'utf8');
    await writeFile(
      paritySpecFile,
      `${JSON.stringify(
        {
          scenarioId: 'media-file-update-mixed-batch-partial-success',
          operationNames: ['fileCreate', 'fileUpdate', 'files', 'nodes'],
          scenarioStatus: 'captured',
          assertionKinds: ['payload-shape', 'user-errors-parity', 'downstream-read-parity', 'side-effect-boundary'],
          liveCaptureFiles: [outputFile],
          proxyConfig: { readMode: 'snapshot' },
          proxyRequest: {
            documentPath: createRequestFile,
            variablesCapturePath: '$.setup.imageCreate.variables',
            apiVersion,
          },
          comparisonMode: 'captured-vs-proxy-request',
          notes:
            "Real Admin GraphQL capture of two READY image files updated in one fileUpdate call. Shopify returns and persists the valid alt update while reporting the second row's indexed ALT_VALUE_LIMIT_EXCEEDED userError; immediate files and nodes reads keep the rejected row unchanged.",
          comparison: {
            mode: 'strict-json',
            expectedDifferences: [],
            targets: [
              {
                name: 'both-files-ready-before-mixed-update',
                capturePath: '$.setup.mixedReadyRead.response.data',
                proxyPath: '$.data',
                proxyRequest: {
                  documentPath: readRequestFile,
                  variables: {
                    ids: [
                      { fromPrimaryProxyPath: '$.data.fileCreate.files[0].id' },
                      { fromPrimaryProxyPath: '$.data.fileCreate.files[1].id' },
                    ],
                    query: { fromCapturePath: '$.setup.mixedReadyRead.variables.query' },
                  },
                  apiVersion,
                },
                expectedDifferences: mediaFileIdDifferences(),
              },
              {
                name: 'mixed-batch-returns-valid-file-and-indexed-error',
                capturePath: '$.mixedBatch.mutation.response.data.fileUpdate',
                proxyPath: '$.data.fileUpdate',
                proxyRequest: {
                  documentPath: updateRequestFile,
                  variables: {
                    files: [
                      {
                        id: { fromPrimaryProxyPath: '$.data.fileCreate.files[0].id' },
                        alt: 'Mixed batch valid row after update',
                      },
                      {
                        id: { fromPrimaryProxyPath: '$.data.fileCreate.files[1].id' },
                        alt: 'x'.repeat(513),
                      },
                    ],
                  },
                  apiVersion,
                },
                expectedDifferences: [mediaFileIdDifference('$.files[0].id')],
              },
              {
                name: 'mixed-batch-files-and-nodes-read-successful-subset',
                capturePath: '$.mixedBatch.downstreamRead.response.data',
                proxyPath: '$.data',
                preserveProxyState: true,
                proxyRequest: {
                  documentPath: readRequestFile,
                  variables: {
                    ids: [
                      { fromPrimaryProxyPath: '$.data.fileCreate.files[0].id' },
                      { fromPrimaryProxyPath: '$.data.fileCreate.files[1].id' },
                    ],
                    query: { fromCapturePath: '$.setup.mixedReadyRead.variables.query' },
                  },
                  apiVersion,
                },
                expectedDifferences: mediaFileIdDifferences(),
              },
            ],
          },
        },
        null,
        2,
      )}\n`,
      'utf8',
    );
    console.log(`wrote ${outputFile}`);
    console.log(`wrote ${paritySpecFile}`);
  }
}
