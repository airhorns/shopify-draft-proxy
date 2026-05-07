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
type FileUpdateInputProbeData = {
  __type?: { inputFields?: Array<{ name?: string | null } | null> | null } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const outputFile = path.join(outputDir, 'media-file-update-validation-ordering.json');
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
`;

const fileCreateMutation = `#graphql
  mutation MediaFileUpdateValidationOrderingSeed($files: [FileCreateInput!]!) {
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
  mutation MediaFileUpdateValidationOrdering($files: [FileUpdateInput!]!) {
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
  query MediaFileUpdateValidationOrderingReadyPoll($id: ID!) {
    node(id: $id) {
      ${nodeFileSelection}
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation MediaFileUpdateValidationOrderingCleanup($fileIds: [ID!]!) {
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

const fileUpdateInputProbeQuery = `#graphql
  query MediaFileUpdateValidationOrderingInputProbe {
    __type(name: "FileUpdateInput") {
      inputFields {
        name
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

function expectUserErrors(label: string, payload: GraphqlPayload<FileUpdateData>, codes: string[]): void {
  const actualCodes = payload.data?.fileUpdate?.userErrors?.map((error) => error.code) ?? [];
  if (JSON.stringify(actualCodes) === JSON.stringify(codes)) {
    return;
  }

  throw new Error(`${label} returned ${JSON.stringify(actualCodes)} instead of ${JSON.stringify(codes)}`);
}

async function waitForReadyFile(fileId: string): Promise<GraphqlPayload<FileReadData>> {
  let lastPayload: GraphqlPayload<FileReadData> | null = null;

  for (let attempt = 0; attempt < 30; attempt += 1) {
    lastPayload = await runGraphql<FileReadData>(fileReadQuery, { id: fileId });
    if (lastPayload.data?.node?.fileStatus === 'READY') {
      return lastPayload;
    }

    await delay(2000);
  }

  throw new Error(`Timed out waiting for ${fileId} to reach READY: ${JSON.stringify(lastPayload, null, 2)}`);
}

const timestamp = Date.now();
const missingFileId = 'gid://shopify/MediaImage/900000000000123';
const longAlt = 'x'.repeat(513);
const imageCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `file-update-validation-ordering-${timestamp}.jpg`,
      originalSource: 'https://placehold.co/600x400.jpg',
      alt: 'fileUpdate validation ordering seed',
    },
  ],
};
const missingLongAltVariables = {
  files: [{ id: missingFileId, alt: longAlt }],
};

const createdFileIds: string[] = [];
let capture: Record<string, unknown> | null = null;

try {
  const fileUpdateInputProbe = await runGraphql<FileUpdateInputProbeData>(fileUpdateInputProbeQuery);
  const imageCreate = await runGraphql<FileCreateData>(fileCreateMutation, imageCreateVariables);
  expectNoUserErrors('image fileCreate', imageCreate.data?.fileCreate?.userErrors);
  const imageId = requireId('image fileCreate', imageCreate.data?.fileCreate?.files?.[0]);
  createdFileIds.push(imageId);

  const readyImageRead = await waitForReadyFile(imageId);
  const conflictingSourcesVariables = {
    files: [
      {
        id: imageId,
        originalSource: 'https://cdn.example.com/source-replacement.jpg',
        previewImageSource: 'https://cdn.example.com/preview-replacement.jpg',
      },
    ],
  };

  const missingLongAlt = await runGraphql<FileUpdateData>(fileUpdateMutation, missingLongAltVariables);
  expectUserErrors('missing id plus long alt', missingLongAlt, ['FILE_DOES_NOT_EXIST']);

  const conflictingSources = await runGraphql<FileUpdateData>(fileUpdateMutation, conflictingSourcesVariables);
  expectUserErrors('conflicting source updates', conflictingSources, ['INVALID', 'INVALID']);

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-update-validation-ordering',
    schema: {
      fileUpdateInputProbe,
      notes:
        'The public Admin GraphQL schema does not expose FileUpdateInput.revertToVersionId, so the source-plus-version ordering branch cannot be recorded through this public capture path.',
    },
    setup: {
      imageCreate: { variables: imageCreateVariables, response: imageCreate },
      readyImageRead,
    },
    branches: {
      missingLongAlt: { variables: missingLongAltVariables, response: missingLongAlt },
      conflictingSources: { variables: conflictingSourcesVariables, response: conflictingSources },
    },
    upstreamCalls: [
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [missingFileId] },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: { data: { nodes: [null] } },
        },
      },
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
    await writeFile(outputFile, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
    console.log(`wrote ${outputFile}`);
  }
}
