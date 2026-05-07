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
type ExpectedErrorCode = 'ALT_VALUE_LIMIT_EXCEEDED' | 'INVALID' | 'INVALID_IMAGE_SOURCE_URL';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const outputFile = path.join(outputDir, 'media-file-update-fabricated-validation.json');
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
`;

const fileCreateMutation = `#graphql
  mutation MediaFileUpdateFabricatedValidationSeed($files: [FileCreateInput!]!) {
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
  query MediaFileUpdateFabricatedValidationReadyPoll($id: ID!) {
    node(id: $id) {
      ${nodeFileSelection}
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation MediaFileUpdateFabricatedValidationCleanup($fileIds: [ID!]!) {
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

function hasUserErrorCode(payload: GraphqlPayload<FileUpdateData>, code: ExpectedErrorCode): boolean {
  return (payload.data?.fileUpdate?.userErrors ?? []).some((error) => error?.code === code);
}

async function runFileUpdate(variables: GraphqlVariables): Promise<GraphqlPayload<FileUpdateData>> {
  const response = await runGraphqlRaw<FileUpdateData>(fileUpdateMutation, variables);
  return response.payload;
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

function longUrl(length: number): string {
  const prefix = 'https://cdn.example.com/';
  const suffix = '.jpg';
  return `${prefix}${'a'.repeat(length - prefix.length - suffix.length)}${suffix}`;
}

const createdFileIds: string[] = [];
const timestamp = Date.now();
const longAlt = `Media update validation long alt ${'a'.repeat(513)}`;
const overLengthSource = longUrl(3000);

const imageCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `media-file-update-validation-seed-${timestamp}.jpg`,
      originalSource: 'https://placehold.co/600x400.jpg',
      alt: 'Media file update validation seed image',
    },
  ],
};

let capture: Record<string, unknown> | null = null;

try {
  const imageCreate = await runGraphql<FileCreateData>(fileCreateMutation, imageCreateVariables);
  expectNoUserErrors('image fileCreate', imageCreate.data?.fileCreate?.userErrors);
  const imageId = requireId('image fileCreate', imageCreate.data?.fileCreate?.files?.[0]);
  createdFileIds.push(imageId);

  const readyImageRead = await waitForReadyFile(imageId, 'image');

  const longAltVariables = {
    files: [{ id: imageId, alt: longAlt }],
  };
  const longAltUpdate = await runFileUpdate(longAltVariables);

  const invalidOriginalSourceVariables = {
    files: [{ id: imageId, originalSource: 'not-a-url' }],
  };
  const invalidOriginalSource = await runFileUpdate(invalidOriginalSourceVariables);

  const overLengthOriginalSourceVariables = {
    files: [{ id: imageId, originalSource: overLengthSource }],
  };
  const overLengthOriginalSource = await runFileUpdate(overLengthOriginalSourceVariables);

  const overLengthPreviewImageSourceVariables = {
    files: [{ id: imageId, previewImageSource: overLengthSource }],
  };
  const overLengthPreviewImageSource = await runFileUpdate(overLengthPreviewImageSourceVariables);

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-update-fabricated-validation',
    setup: {
      imageCreate: { variables: imageCreateVariables, response: imageCreate },
      readyImageRead,
    },
    branches: {
      longAlt: {
        variables: longAltVariables,
        response: longAltUpdate,
        observedCode: hasUserErrorCode(longAltUpdate, 'ALT_VALUE_LIMIT_EXCEEDED') ? 'ALT_VALUE_LIMIT_EXCEEDED' : null,
      },
      invalidOriginalSource: { variables: invalidOriginalSourceVariables, response: invalidOriginalSource },
      invalidOriginalSourceObservation: {
        observedCode: hasUserErrorCode(invalidOriginalSource, 'INVALID_IMAGE_SOURCE_URL')
          ? 'INVALID_IMAGE_SOURCE_URL'
          : hasUserErrorCode(invalidOriginalSource, 'INVALID')
            ? 'INVALID'
            : null,
      },
      overLengthOriginalSource: { variables: overLengthOriginalSourceVariables, response: overLengthOriginalSource },
      overLengthPreviewImageSource: {
        variables: overLengthPreviewImageSourceVariables,
        response: overLengthPreviewImageSource,
        observedCode: hasUserErrorCode(overLengthPreviewImageSource, 'INVALID_IMAGE_SOURCE_URL')
          ? 'INVALID_IMAGE_SOURCE_URL'
          : hasUserErrorCode(overLengthPreviewImageSource, 'INVALID')
            ? 'INVALID'
            : null,
      },
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
