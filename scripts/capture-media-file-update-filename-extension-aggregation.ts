/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

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
const outputFile = path.join(outputDir, 'media-file-update-filename-extension-aggregation.json');
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
  ... on File {
    alt
    createdAt
    fileStatus
  }
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
  ... on GenericFile {
    url
  }
`;

const fileCreateMutation = `#graphql
  mutation MediaFileUpdateFilenameAggregationSeed($files: [FileCreateInput!]!) {
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
  mutation MediaFileUpdateFilenameExtensionAggregation($files: [FileUpdateInput!]!) {
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
  query MediaFileUpdateFilenameAggregationReadyPoll($id: ID!) {
    node(id: $id) {
      ${nodeFileSelection}
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation MediaFileUpdateFilenameAggregationCleanup($fileIds: [ID!]!) {
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

function expectSingleUserErrorCode(label: string, payload: GraphqlPayload<FileUpdateData>, code: string): void {
  const errors = payload.data?.fileUpdate?.userErrors ?? [];
  if (errors.length === 1 && errors[0]?.code === code) {
    return;
  }

  throw new Error(`${label} did not return exactly one ${code}: ${JSON.stringify(payload, null, 2)}`);
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

const createdFileIds: string[] = [];
const timestamp = Date.now();

const imageCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `filename-aggregation-one-${timestamp}.jpg`,
      originalSource: 'https://placehold.co/600x400.jpg',
      alt: 'Filename aggregation image one',
    },
    {
      contentType: 'IMAGE',
      filename: `filename-aggregation-two-${timestamp}.jpg`,
      originalSource: 'https://placehold.co/600x400.jpg',
      alt: 'Filename aggregation image two',
    },
  ],
};
const externalVideoCreateVariables = {
  files: [
    {
      contentType: 'EXTERNAL_VIDEO',
      originalSource: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
      alt: 'Filename aggregation external video one',
    },
    {
      contentType: 'EXTERNAL_VIDEO',
      originalSource: 'https://www.youtube.com/watch?v=oHg5SJYRHA0',
      alt: 'Filename aggregation external video two',
    },
  ],
};

let capture: Record<string, unknown> | null = null;

try {
  const imageCreate = await runGraphql<FileCreateData>(fileCreateMutation, imageCreateVariables);
  expectNoUserErrors('image fileCreate', imageCreate.data?.fileCreate?.userErrors);
  const firstImageId = requireId('first image fileCreate', imageCreate.data?.fileCreate?.files?.[0]);
  const secondImageId = requireId('second image fileCreate', imageCreate.data?.fileCreate?.files?.[1]);
  createdFileIds.push(firstImageId, secondImageId);

  const externalVideoCreate = await runGraphql<FileCreateData>(fileCreateMutation, externalVideoCreateVariables);
  expectNoUserErrors('external video fileCreate', externalVideoCreate.data?.fileCreate?.userErrors);
  const firstExternalVideoId = requireId(
    'first external video fileCreate',
    externalVideoCreate.data?.fileCreate?.files?.[0],
  );
  const secondExternalVideoId = requireId(
    'second external video fileCreate',
    externalVideoCreate.data?.fileCreate?.files?.[1],
  );
  createdFileIds.push(firstExternalVideoId, secondExternalVideoId);

  const firstImageRead = await waitForReadyFile(firstImageId, 'first image');
  const secondImageRead = await waitForReadyFile(secondImageId, 'second image');
  const firstExternalVideoRead = await waitForReadyFile(firstExternalVideoId, 'first external video');
  const secondExternalVideoRead = await waitForReadyFile(secondExternalVideoId, 'second external video');

  const singleImageMismatchVariables = {
    files: [{ id: firstImageId, filename: `filename-aggregation-single-${timestamp}.png` }],
  };
  const singleImageMismatch = await runGraphql<FileUpdateData>(fileUpdateMutation, singleImageMismatchVariables);
  expectSingleUserErrorCode(
    'single image filename extension mismatch',
    singleImageMismatch,
    'INVALID_FILENAME_EXTENSION',
  );

  const multiImageMismatchVariables = {
    files: [
      { id: firstImageId, filename: `filename-aggregation-multi-one-${timestamp}.png` },
      { id: secondImageId, filename: `filename-aggregation-multi-two-${timestamp}.gif` },
    ],
  };
  const multiImageMismatch = await runGraphql<FileUpdateData>(fileUpdateMutation, multiImageMismatchVariables);
  expectSingleUserErrorCode(
    'multi image filename extension mismatch',
    multiImageMismatch,
    'INVALID_FILENAME_EXTENSION',
  );

  const multiExternalVideoFilenameVariables = {
    files: [
      { id: firstExternalVideoId, filename: `filename-aggregation-video-one-${timestamp}.mp4` },
      { id: secondExternalVideoId, filename: `filename-aggregation-video-two-${timestamp}.mp4` },
    ],
  };
  const multiExternalVideoFilename = await runGraphql<FileUpdateData>(
    fileUpdateMutation,
    multiExternalVideoFilenameVariables,
  );
  expectSingleUserErrorCode(
    'multi external video filename update',
    multiExternalVideoFilename,
    'UNSUPPORTED_MEDIA_TYPE_FOR_FILENAME_UPDATE',
  );

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-update-filename-extension-aggregation',
    setup: {
      imageCreate: { variables: imageCreateVariables, response: imageCreate },
      externalVideoCreate: {
        variables: externalVideoCreateVariables,
        response: externalVideoCreate,
      },
      reads: {
        firstImage: firstImageRead,
        secondImage: secondImageRead,
        firstExternalVideo: firstExternalVideoRead,
        secondExternalVideo: secondExternalVideoRead,
      },
    },
    cases: {
      singleImageMismatch: {
        variables: singleImageMismatchVariables,
        response: singleImageMismatch,
      },
      multiImageMismatch: {
        variables: multiImageMismatchVariables,
        response: multiImageMismatch,
      },
      multiExternalVideoFilename: {
        variables: multiExternalVideoFilenameVariables,
        response: multiExternalVideoFilename,
      },
    },
    upstreamCalls: [
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [firstImageId] },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: { data: { nodes: [firstImageRead.data?.node ?? null] } },
        },
      },
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [secondImageId] },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: { data: { nodes: [secondImageRead.data?.node ?? null] } },
        },
      },
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [firstExternalVideoId, secondExternalVideoId] },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: {
            data: {
              nodes: [firstExternalVideoRead.data?.node ?? null, secondExternalVideoRead.data?.node ?? null],
            },
          },
        },
      },
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
