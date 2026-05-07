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
  fileStatus?: string | null;
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
const outputFile = path.join(outputDir, 'media-file-interface-fields.json');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload<TData>>;
};

const fileNodeSelection = `#graphql
  __typename
  id
  alt
  createdAt
  updatedAt
  fileStatus
  fileErrors {
    code
    message
    details
  }
  ... on MediaImage {
    mimeType
    image {
      url
    }
    mediaErrors {
      code
      message
      details
    }
    mediaWarnings {
      code
      message
    }
  }
  ... on GenericFile {
    mimeType
    url
  }
`;

const fileCreateMutation = `#graphql
  mutation MediaFileInterfaceFieldsCreate($files: [FileCreateInput!]!) {
    fileCreate(files: $files) {
      files {
        ${fileNodeSelection}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const filesReadQuery = `#graphql
  query MediaFileInterfaceFieldsRead {
    files(first: 2, reverse: true, sortKey: ID) {
      nodes {
        ${fileNodeSelection}
      }
    }
  }
`;

const fileReadyReadQuery = `#graphql
  query MediaFileInterfaceFieldsReadyPoll($id: ID!) {
    node(id: $id) {
      ... on File {
        id
        __typename
        fileStatus
      }
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation MediaFileInterfaceFieldsCleanup($fileIds: [ID!]!) {
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

function requireFileNode(label: string, node: FileNode | null | undefined): FileNode {
  if (typeof node?.id === 'string' && node.id.length > 0) {
    return node;
  }

  throw new Error(`${label} did not return a file node: ${JSON.stringify(node ?? null, null, 2)}`);
}

async function waitForTerminalFile(fileId: string): Promise<GraphqlPayload<FileReadData>> {
  let lastPayload: GraphqlPayload<FileReadData> | null = null;

  for (let attempt = 0; attempt < 30; attempt += 1) {
    lastPayload = await runGraphql<FileReadData>(fileReadyReadQuery, { id: fileId });
    const status = lastPayload.data?.node?.fileStatus;
    if (status === 'READY' || status === 'FAILED') {
      return lastPayload;
    }

    await delay(2000);
  }

  throw new Error(`Timed out waiting for file ${fileId}: ${JSON.stringify(lastPayload, null, 2)}`);
}

function assertReadOrder(readResponse: GraphqlPayload<FilesReadData>, imageId: string, genericId: string): void {
  const nodes = readResponse.data?.files?.nodes ?? [];
  const ids = nodes.map((node) => node?.id ?? null);
  if (ids.length !== 2 || ids[0] !== genericId || ids[1] !== imageId) {
    throw new Error(
      `Expected files query to return only the created generic and image files in newest-first order: ${JSON.stringify(
        ids,
        null,
        2,
      )}`,
    );
  }
}

await mkdir(outputDir, { recursive: true });

const runId = `media-file-interface-fields-${Date.now()}`;
const imageFilename = `${runId}-image.jpg`;
const genericFilename = `${runId}-generic.pdf`;
const imageSource = 'https://placehold.co/600x400.jpg';
const genericSource = 'https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf';
const createVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: imageFilename,
      originalSource: imageSource,
      alt: `${runId} image`,
    },
    {
      contentType: 'FILE',
      filename: genericFilename,
      originalSource: genericSource,
      alt: `${runId} generic`,
    },
  ],
};
const createdFileIds: string[] = [];
let capture: Record<string, unknown> | null = null;

try {
  const createResponse = await runGraphql<FileCreateData>(fileCreateMutation, createVariables);
  expectNoUserErrors('fileCreate', createResponse.data?.fileCreate?.userErrors);

  const image = requireFileNode('fileCreate.files[0]', createResponse.data?.fileCreate?.files?.[0]);
  const generic = requireFileNode('fileCreate.files[1]', createResponse.data?.fileCreate?.files?.[1]);
  createdFileIds.push(image.id as string, generic.id as string);

  const readyImageRead = await waitForTerminalFile(image.id as string);
  const readyGenericRead = await waitForTerminalFile(generic.id as string);
  const readAfterCreateResponse = await runGraphql<FilesReadData>(filesReadQuery);
  assertReadOrder(readAfterCreateResponse, image.id as string, generic.id as string);

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-interface-fields',
    create: {
      variables: createVariables,
      response: createResponse,
    },
    readiness: {
      image: readyImageRead,
      generic: readyGenericRead,
    },
    readAfterCreate: {
      response: readAfterCreateResponse,
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
    await writeFile(outputFile, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
    console.log(`wrote ${outputFile}`);
  }
}
