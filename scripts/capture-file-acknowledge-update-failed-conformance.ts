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
    files?: Array<{ id?: string | null; fileStatus?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};

type FileUpdateData = {
  fileUpdate?: {
    files?: Array<{ id?: string | null; fileStatus?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};

type FileDeleteData = {
  fileDelete?: {
    deletedFileIds?: string[] | null;
    userErrors?: UserError[] | null;
  } | null;
};

type FileReadData = {
  node?: {
    id?: string | null;
    fileStatus?: string | null;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputFile = path.join(outputDir, 'file-acknowledge-update-failed-parity.json');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload<TData>>;
};

const fileCreateMutation = `#graphql
  mutation FileAcknowledgeUpdateFailedSeed($files: [FileCreateInput!]!) {
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

const fileUpdateMutation = `#graphql
  mutation FileAcknowledgeUpdateFailedUpdate($files: [FileUpdateInput!]!) {
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

const fileAcknowledgeUpdateFailedMutation = `#graphql
  mutation FileAcknowledgeUpdateFailedParity($fileIds: [ID!]!) {
    fileAcknowledgeUpdateFailed(fileIds: $fileIds) {
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

const fileReadQuery = `#graphql
  query FileAcknowledgeUpdateFailedPoll($id: ID!) {
    node(id: $id) {
      ... on MediaImage {
        id
        fileStatus
      }
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation FileAcknowledgeUpdateFailedCleanup($fileIds: [ID!]!) {
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

async function waitForFileStatus(fileId: string, status: string): Promise<GraphqlPayload<FileReadData>> {
  let lastPayload: GraphqlPayload<FileReadData> | null = null;

  for (let attempt = 0; attempt < 15; attempt += 1) {
    lastPayload = await runGraphql<FileReadData>(fileReadQuery, { id: fileId });
    if (lastPayload.data?.node?.fileStatus === status) {
      return lastPayload;
    }

    await delay(2000);
  }

  throw new Error(`Timed out waiting for file ${fileId} to reach ${status}: ${JSON.stringify(lastPayload, null, 2)}`);
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
let readyFileId: string | null = null;
let failedCreateFileId: string | null = null;

try {
  const createVariables = {
    files: [
      {
        contentType: 'IMAGE',
        originalSource: 'https://placehold.co/600x400/png',
        alt: `HAR-375 acknowledgement ready seed ${runId}`,
      },
    ],
  };
  const createResponse = await runGraphql<FileCreateData>(fileCreateMutation, createVariables);
  expectNoUserErrors('fileCreate ready seed', createResponse.data?.fileCreate?.userErrors);
  readyFileId = requireId('fileCreate ready seed files[0]', createResponse.data?.fileCreate?.files?.[0]?.id);
  const readyFileRead = await waitForFileStatus(readyFileId, 'READY');

  const updateVariables = {
    files: [
      {
        id: readyFileId,
        alt: `HAR-375 acknowledgement updated ${runId}`,
        originalSource: 'https://example.com/not-an-image.jpg',
      },
    ],
  };
  const updateResponse = await runGraphql<FileUpdateData>(fileUpdateMutation, updateVariables);
  expectNoUserErrors('fileUpdate bad source attempt', updateResponse.data?.fileUpdate?.userErrors);
  const readAfterUpdateAttempt = await runGraphql<FileReadData>(fileReadQuery, { id: readyFileId });

  const acknowledgeVariables = { fileIds: [readyFileId] };
  const acknowledgeResponse = await runGraphql(fileAcknowledgeUpdateFailedMutation, acknowledgeVariables);
  const readAfterAcknowledge = await runGraphql<FileReadData>(fileReadQuery, { id: readyFileId });

  const unknownVariables = { fileIds: ['gid://shopify/MediaImage/999999999999999'] };
  const unknownResponse = await runGraphql(fileAcknowledgeUpdateFailedMutation, unknownVariables);

  const failedCreateVariables = {
    files: [
      {
        contentType: 'IMAGE',
        originalSource: 'https://example.com/not-an-image.jpg',
        alt: `HAR-375 acknowledgement failed create ${runId}`,
      },
    ],
  };
  const failedCreateResponse = await runGraphql<FileCreateData>(fileCreateMutation, failedCreateVariables);
  expectNoUserErrors('fileCreate failed-source seed', failedCreateResponse.data?.fileCreate?.userErrors);
  failedCreateFileId = requireId(
    'fileCreate failed-source files[0]',
    failedCreateResponse.data?.fileCreate?.files?.[0]?.id,
  );
  const failedCreateRead = await waitForFileStatus(failedCreateFileId, 'FAILED');
  const failedCreateAcknowledgeVariables = { fileIds: [failedCreateFileId] };
  const failedCreateAcknowledgeResponse = await runGraphql(
    fileAcknowledgeUpdateFailedMutation,
    failedCreateAcknowledgeVariables,
  );

  const deleteReadyVariables = { fileIds: [readyFileId] };
  const deleteReadyResponse = await runGraphql<FileDeleteData>(fileDeleteMutation, deleteReadyVariables);
  expectNoUserErrors('fileDelete ready seed', deleteReadyResponse.data?.fileDelete?.userErrors);
  const deletedAcknowledgeResponse = await runGraphql(fileAcknowledgeUpdateFailedMutation, acknowledgeVariables);

  const capture = {
    notes:
      'Shopify Admin GraphQL 2026-04 expects fileAcknowledgeUpdateFailed(fileIds:) and returns files plus userErrors. A safely staged bad-source fileUpdate stayed READY with a null image in this capture and could be acknowledged successfully; a FAILED file produced by bad-source fileCreate returned NON_READY_STATE, and deleted/unknown IDs returned FILE_DOES_NOT_EXIST.',
    success: {
      setup: {
        create: {
          variables: createVariables,
          response: createResponse,
        },
        readyFileRead,
        update: {
          variables: updateVariables,
          response: updateResponse,
        },
        readAfterUpdateAttempt,
      },
      mutation: {
        variables: acknowledgeVariables,
        response: acknowledgeResponse,
      },
      readAfterAcknowledge,
    },
    validation: {
      unknown: {
        variables: unknownVariables,
        response: unknownResponse,
      },
      failedCreateNonReady: {
        create: {
          variables: failedCreateVariables,
          response: failedCreateResponse,
        },
        failedCreateRead,
        mutation: {
          variables: failedCreateAcknowledgeVariables,
          response: failedCreateAcknowledgeResponse,
        },
      },
      deleted: {
        delete: {
          variables: deleteReadyVariables,
          response: deleteReadyResponse,
        },
        mutation: {
          variables: acknowledgeVariables,
          response: deletedAcknowledgeResponse,
        },
      },
    },
  };

  await writeFile(outputFile, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

  console.log(JSON.stringify({ ok: true, outputFile, readyFileId, failedCreateFileId }, null, 2));
} finally {
  if (readyFileId) {
    try {
      await runGraphql<FileDeleteData>(fileDeleteMutation, { fileIds: [readyFileId] });
    } catch {
      // Best-effort cleanup only.
    }
  }

  if (failedCreateFileId) {
    try {
      await runGraphql<FileDeleteData>(fileDeleteMutation, { fileIds: [failedCreateFileId] });
    } catch {
      // Best-effort cleanup only.
    }
  }
}
