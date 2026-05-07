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
  url?: string | null;
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
type FilesReadData = { files?: { nodes?: Array<FileNode | null> | null } | null };

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const outputFile = path.join(outputDir, 'media-file-update-source-semantics.json');
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
  ... on GenericFile {
    url
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
`;

const fileCreateMutation = `#graphql
  mutation MediaFileUpdateSourceSemanticsSeed($files: [FileCreateInput!]!) {
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
  mutation MediaFileUpdateSourceSemantics($files: [FileUpdateInput!]!) {
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

const fileReadyReadQuery = `#graphql
  query MediaFileUpdateSourceSemanticsReadyPoll($id: ID!) {
    node(id: $id) {
      ${nodeFileSelection}
    }
  }
`;

const imageFilesReadQuery = `#graphql
  query MediaFileUpdateSourceSemanticsRead {
    files(first: 1, reverse: true) {
      nodes {
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
    }
  }
`;

const genericFilesReadQuery = `#graphql
  query MediaFileUpdateSourceSemanticsGenericRead {
    files(first: 2, reverse: true) {
      nodes {
        id
        __typename
        alt
        createdAt
        fileStatus
        ... on GenericFile {
          url
        }
      }
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation MediaFileUpdateSourceSemanticsCleanup($fileIds: [ID!]!) {
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
    lastPayload = await runGraphql<FileReadData>(fileReadyReadQuery, { id: fileId });
    if (lastPayload.data?.node?.fileStatus === 'READY') {
      return lastPayload;
    }

    await delay(2000);
  }

  throw new Error(`Timed out waiting for ${label} ${fileId} to reach READY: ${JSON.stringify(lastPayload, null, 2)}`);
}

const createdFileIds: string[] = [];
const timestamp = Date.now();
const imageSource = 'https://placehold.co/600x400.jpg';
const imageReplacementSource = 'https://placehold.co/640x480.jpg';
const genericSource = 'https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf';
const genericReplacementSource = 'https://www.w3.org/WAI/WCAG21/working-examples/pdf-table/table.pdf';

const imageCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `source-semantics-image-${timestamp}.jpg`,
      originalSource: imageSource,
      alt: 'Source semantics image seed',
    },
  ],
};
const genericCreateVariables = {
  files: [
    {
      contentType: 'FILE',
      filename: `source-semantics-generic-${timestamp}.pdf`,
      originalSource: genericSource,
      alt: 'Source semantics generic seed',
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

  const imageOriginalSourceVariables = {
    files: [{ id: imageId, originalSource: imageReplacementSource }],
  };
  const imageOriginalSource = await runGraphql<FileUpdateData>(fileUpdateMutation, imageOriginalSourceVariables);
  expectNoUserErrors('image originalSource fileUpdate', imageOriginalSource.data?.fileUpdate?.userErrors);
  const imageAfterOriginalSource = await runGraphql<FilesReadData>(imageFilesReadQuery);

  const genericCreate = await runGraphql<FileCreateData>(fileCreateMutation, genericCreateVariables);
  expectNoUserErrors('generic fileCreate', genericCreate.data?.fileCreate?.userErrors);
  const genericId = requireId('generic fileCreate', genericCreate.data?.fileCreate?.files?.[0]);
  createdFileIds.push(genericId);
  const readyGenericRead = await waitForReadyFile(genericId, 'generic file');

  const genericOriginalSourceVariables = {
    files: [{ id: genericId, originalSource: genericReplacementSource }],
  };
  const genericOriginalSource = await runGraphql<FileUpdateData>(fileUpdateMutation, genericOriginalSourceVariables);
  expectNoUserErrors('generic originalSource fileUpdate', genericOriginalSource.data?.fileUpdate?.userErrors);
  const genericAfterOriginalSource = await runGraphql<FilesReadData>(genericFilesReadQuery);

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-update-source-semantics',
    setup: {
      imageCreate: { variables: imageCreateVariables, response: imageCreate },
      readyImageRead,
      genericCreate: { variables: genericCreateVariables, response: genericCreate },
      readyGenericRead,
    },
    updates: {
      imageOriginalSource: { variables: imageOriginalSourceVariables, response: imageOriginalSource },
      genericOriginalSource: { variables: genericOriginalSourceVariables, response: genericOriginalSource },
    },
    reads: {
      imageAfterOriginalSource: { response: imageAfterOriginalSource },
      genericAfterOriginalSource: { response: genericAfterOriginalSource },
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
        variables: { fileIds: [genericId] },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: { data: { nodes: [readyGenericRead.data?.node ?? null] } },
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
