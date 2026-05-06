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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const outputFile = path.join(outputDir, 'media-file-update-validation-branches.json');
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

const fileCreateMutation = `#graphql
  mutation MediaFileUpdateValidationSeed($files: [FileCreateInput!]!) {
    fileCreate(files: $files) {
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

const fileReadQuery = `#graphql
  query MediaFileUpdateValidationReadyPoll($id: ID!) {
    node(id: $id) {
      ${nodeFileSelection}
    }
  }
`;

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

function tailGid(id: string, replacementType: string): string {
  const tail = id.split('/').pop();
  if (!tail) {
    throw new Error(`Cannot derive tail from ${id}`);
  }
  return `gid://shopify/${replacementType}/${tail}`;
}

const createdFileIds: string[] = [];
const timestamp = Date.now();

const imageCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `har-702-seed-${timestamp}.jpg`,
      originalSource: 'https://placehold.co/600x400.jpg',
      alt: 'HAR-702 seed image',
    },
  ],
};
const videoCreateVariables = {
  files: [
    {
      contentType: 'EXTERNAL_VIDEO',
      originalSource: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
      alt: 'HAR-702 seed video',
    },
  ],
};
const nonReadyCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `har-702-non-ready-${timestamp}.jpg`,
      originalSource: 'https://placehold.co/600x400.jpg',
      alt: 'HAR-702 non-ready image',
    },
  ],
};

let capture: Record<string, unknown> | null = null;

try {
  const imageCreate = await runGraphql<FileCreateData>(fileCreateMutation, imageCreateVariables);
  expectNoUserErrors('image fileCreate', imageCreate.data?.fileCreate?.userErrors);
  const imageId = requireId('image fileCreate', imageCreate.data?.fileCreate?.files?.[0]);
  createdFileIds.push(imageId);

  const videoCreate = await runGraphql<FileCreateData>(fileCreateMutation, videoCreateVariables);
  expectNoUserErrors('video fileCreate', videoCreate.data?.fileCreate?.userErrors);
  const videoId = requireId('video fileCreate', videoCreate.data?.fileCreate?.files?.[0]);
  createdFileIds.push(videoId);

  const nonReadyCreate = await runGraphql<FileCreateData>(fileCreateMutation, nonReadyCreateVariables);
  expectNoUserErrors('non-ready fileCreate', nonReadyCreate.data?.fileCreate?.userErrors);
  const nonReadyImageId = requireId('non-ready fileCreate', nonReadyCreate.data?.fileCreate?.files?.[0]);
  createdFileIds.push(nonReadyImageId);

  const nonReadyAltVariables = {
    files: [{ id: nonReadyImageId, alt: 'HAR-702 non-ready update attempt' }],
  };
  const nonReadyAlt = await runGraphql<FileUpdateData>(fileUpdateMutation, nonReadyAltVariables);
  expectUserErrorCode('non-ready alt update', nonReadyAlt, 'NON_READY_STATE');

  const readyImageRead = await waitForReadyFile(imageId, 'image');
  const readyVideoRead = await waitForReadyFile(videoId, 'video');

  const videoOriginalSourceVariables = {
    files: [{ id: videoId, originalSource: 'https://cdn.example.com/har-702-new-video.mp4' }],
  };
  const videoOriginalSource = await runGraphql<FileUpdateData>(fileUpdateMutation, videoOriginalSourceVariables);
  expectUserErrorCode('video originalSource update', videoOriginalSource, 'INVALID');

  const videoFilenameVariables = {
    files: [{ id: videoId, filename: `har-702-renamed-${timestamp}.youtube` }],
  };
  const videoFilename = await runGraphql<FileUpdateData>(fileUpdateMutation, videoFilenameVariables);
  expectUserErrorCode('video filename update', videoFilename, 'UNSUPPORTED_MEDIA_TYPE_FOR_FILENAME_UPDATE');

  const imageFilenameMismatchVariables = {
    files: [{ id: imageId, filename: `har-702-seed-${timestamp}.png` }],
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

  const sourceAndVersionVariables = {
    files: [
      {
        id: imageId,
        originalSource: 'https://placehold.co/600x400.png',
        revertToVersionId: 'gid://shopify/FileVersion/9',
      },
    ],
  };
  const sourceAndVersion = {
    data: {
      fileUpdate: {
        files: [],
        userErrors: [
          {
            field: ['files', '0'],
            message: 'Specify either a source or revertToVersionId, not both.',
            code: 'CANNOT_SPECIFY_SOURCE_AND_VERSION_ID',
          },
        ],
      },
    },
    notes:
      'Current public Admin GraphQL 2026-04 schema does not expose FileUpdateInput.revertToVersionId, so this branch is encoded from the internal HAR-702 evidence and executable local proxy behavior rather than a live public-schema mutation response.',
  };

  const wrongTypeVariables = {
    files: [{ id: tailGid(imageId, 'Video'), alt: 'HAR-702 wrong typed id' }],
  };
  const wrongType = await runGraphql<FileUpdateData>(fileUpdateMutation, wrongTypeVariables);
  expectUserErrorCode('wrong typed gid update', wrongType, 'FILE_DOES_NOT_EXIST');

  const successAltVariables = {
    files: [{ id: imageId, alt: 'HAR-702 successful alt update preserves READY' }],
  };
  const successAlt = await runGraphql<FileUpdateData>(fileUpdateMutation, successAltVariables);
  expectNoUserErrors('success alt update', successAlt.data?.fileUpdate?.userErrors);

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-update-validation-branches',
    setup: {
      imageCreate: { variables: imageCreateVariables, response: imageCreate },
      videoCreate: { variables: videoCreateVariables, response: videoCreate },
      nonReadyCreate: { variables: nonReadyCreateVariables, response: nonReadyCreate },
      readyImageRead,
      readyVideoRead,
    },
    branches: {
      nonReadyAlt: { variables: nonReadyAltVariables, response: nonReadyAlt },
      videoOriginalSource: { variables: videoOriginalSourceVariables, response: videoOriginalSource },
      videoFilename: { variables: videoFilenameVariables, response: videoFilename },
      imageFilenameMismatch: { variables: imageFilenameMismatchVariables, response: imageFilenameMismatch },
      missingReferences: { variables: missingReferencesVariables, response: missingReferences },
      sourceAndVersion: { variables: sourceAndVersionVariables, response: sourceAndVersion },
      wrongType: { variables: wrongTypeVariables, response: wrongType },
      successAlt: { variables: successAltVariables, response: successAlt },
    },
    upstreamCalls: [
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [imageId] },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: { data: { nodes: [readyImageRead.data?.node ?? null] } },
        },
      },
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [videoId] },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: { data: { nodes: [readyVideoRead.data?.node ?? null] } },
        },
      },
      ...missingReferenceIds.map((productId) => ({
        operationName: 'MediaProductHydrate',
        variables: { id: productId },
        query: 'sha:media-product-hydrate',
        response: {
          status: 200,
          body: { data: { product: null } },
        },
      })),
    ],
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
    await writeFile(`${outputFile}.tmp`, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
    await writeFile(outputFile, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
    console.log(`wrote ${outputFile}`);
  }
}
