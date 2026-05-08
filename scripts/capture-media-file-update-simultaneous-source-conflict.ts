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
type ImageValue = { url?: string | null; width?: number | null; height?: number | null };
type FileNode = {
  id?: string | null;
  __typename?: string | null;
  alt?: string | null;
  createdAt?: string | null;
  fileStatus?: string | null;
  image?: ImageValue | null;
  preview?: { image?: ImageValue | null } | null;
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

const expectedMessage = 'Cannot update the preview image and image at the same time because they are one and the same.';
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

if (apiVersion !== '2026-04') {
  throw new Error(
    `media-file-update-simultaneous-source-conflict requires SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`,
  );
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const outputFile = path.join(outputDir, 'media-file-update-simultaneous-source-conflict.json');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload<TData>>;
  runGraphqlRaw: <TData>(
    query: string,
    variables?: GraphqlVariables,
  ) => Promise<{ status: number; payload: GraphqlPayload<TData> }>;
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

const fileCreateMutation = `#graphql
  mutation MediaFileUpdateSimultaneousSourceSeed($files: [FileCreateInput!]!) {
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
  mutation MediaFileUpdateSimultaneousSource($files: [FileUpdateInput!]!) {
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
  query MediaFileUpdateSimultaneousSourceReadyPoll($id: ID!) {
    node(id: $id) {
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
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation MediaFileUpdateSimultaneousSourceCleanup($fileIds: [ID!]!) {
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

function assertConflictErrors(label: string, errors: UserError[] | null | undefined, indexes: number[]): void {
  const expected = indexes.flatMap((index) => [
    { field: ['files', String(index), 'previewImageSource'], message: expectedMessage, code: 'INVALID' },
    { field: ['files', String(index), 'originalSource'], message: expectedMessage, code: 'INVALID' },
  ]);

  if (JSON.stringify(errors ?? null) !== JSON.stringify(expected)) {
    throw new Error(
      `${label} userErrors did not match expected conflict shape: ${JSON.stringify(errors ?? null, null, 2)}`,
    );
  }
}

async function runFileUpdate(variables: GraphqlVariables): Promise<GraphqlPayload<FileUpdateData>> {
  const response = await runGraphqlRaw<FileUpdateData>(fileUpdateMutation, variables);
  if (response.status < 200 || response.status >= 300) {
    throw new Error(`fileUpdate returned HTTP ${response.status}: ${JSON.stringify(response.payload, null, 2)}`);
  }
  return response.payload;
}

const timestamp = Date.now();
const createdFileIds: string[] = [];
const imageCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `simultaneous-source-a-${timestamp}.jpg`,
      originalSource: 'https://placehold.co/600x400.jpg',
      alt: 'Simultaneous source conflict seed A',
    },
    {
      contentType: 'IMAGE',
      filename: `simultaneous-source-b-${timestamp}.jpg`,
      originalSource: 'https://placehold.co/640x480.jpg',
      alt: 'Simultaneous source conflict seed B',
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

  const firstReadyImageRead = await waitForReadyFile(firstImageId, 'first image');
  const secondReadyImageRead = await waitForReadyFile(secondImageId, 'second image');

  const oneInputVariables = {
    files: [
      {
        id: firstImageId,
        originalSource: 'https://placehold.co/800x600.jpg',
        previewImageSource: 'https://placehold.co/320x240.jpg',
      },
    ],
  };
  const oneInput = await runFileUpdate(oneInputVariables);
  assertConflictErrors('one-input fileUpdate', oneInput.data?.fileUpdate?.userErrors, [0]);

  const twoInputVariables = {
    files: [
      {
        id: firstImageId,
        originalSource: 'https://placehold.co/801x601.jpg',
        previewImageSource: 'https://placehold.co/321x241.jpg',
      },
      {
        id: secondImageId,
        originalSource: 'https://placehold.co/802x602.jpg',
        previewImageSource: 'https://placehold.co/322x242.jpg',
      },
    ],
  };
  const twoInput = await runFileUpdate(twoInputVariables);
  assertConflictErrors('two-input fileUpdate', twoInput.data?.fileUpdate?.userErrors, [0, 1]);

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-update-simultaneous-source-conflict',
    setup: {
      imageCreate: { variables: imageCreateVariables, response: imageCreate },
      firstReadyImageRead,
      secondReadyImageRead,
    },
    cases: {
      oneInput: { variables: oneInputVariables, response: oneInput },
      twoInput: { variables: twoInputVariables, response: twoInput },
    },
    upstreamCalls: [
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [firstImageId] },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: { data: { nodes: [firstReadyImageRead.data?.node ?? null] } },
        },
      },
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [firstImageId, secondImageId] },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: {
            data: {
              nodes: [firstReadyImageRead.data?.node ?? null, secondReadyImageRead.data?.node ?? null],
            },
          },
        },
      },
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [secondImageId] },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: { data: { nodes: [secondReadyImageRead.data?.node ?? null] } },
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
