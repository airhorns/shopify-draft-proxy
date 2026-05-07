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
    files?: Array<{ id?: string | null; __typename?: string | null; fileStatus?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};

type FileDeleteData = {
  fileDelete?: {
    deletedFileIds?: string[] | null;
    userErrors?: UserError[] | null;
  } | null;
};

type FileUpdateData = {
  fileUpdate?: {
    files?: Array<{ id?: string | null; __typename?: string | null; fileStatus?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};

type FileAcknowledgeData = {
  fileAcknowledgeUpdateFailed?: {
    files?: Array<{ id?: string | null; __typename?: string | null; fileStatus?: string | null } | null> | null;
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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const outputFile = path.join(outputDir, 'media-file-user-error-aggregation.json');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<GraphqlPayload<TData>>;
};

const fileCreateMutation = `#graphql
  mutation MediaFileUserErrorAggregationCreate($files: [FileCreateInput!]!) {
    fileCreate(files: $files) {
      files {
        id
        __typename
        fileStatus
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation MediaFileUserErrorAggregationDelete($fileIds: [ID!]!) {
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

const fileUpdateMutation = `#graphql
  mutation MediaFileUserErrorAggregationUpdate($files: [FileUpdateInput!]!) {
    fileUpdate(files: $files) {
      files {
        id
        __typename
        fileStatus
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const fileAcknowledgeMutation = `#graphql
  mutation MediaFileUserErrorAggregationAcknowledge($fileIds: [ID!]!) {
    fileAcknowledgeUpdateFailed(fileIds: $fileIds) {
      files {
        id
        __typename
        fileStatus
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
  query MediaFileUserErrorAggregationPoll($id: ID!) {
    node(id: $id) {
      ... on File {
        id
        fileStatus
      }
    }
  }
`;

function expectNoTopLevelErrors(label: string, payload: GraphqlPayload<unknown>): void {
  if (payload.errors === undefined) {
    return;
  }

  throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload.errors, null, 2)}`);
}

function expectNoUserErrors(label: string, userErrors: UserError[] | null | undefined): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function requireCreatedFileIds(payload: GraphqlPayload<FileCreateData>, count: number): string[] {
  const files = payload.data?.fileCreate?.files ?? [];
  const ids = files.flatMap((file) => (typeof file?.id === 'string' && file.id.length > 0 ? [file.id] : []));
  if (ids.length === count) {
    return ids;
  }

  throw new Error(`fileCreate did not return ${count} file ids: ${JSON.stringify(payload, null, 2)}`);
}

async function waitForFileStatus(fileId: string, status: string): Promise<GraphqlPayload<FileReadData>> {
  let lastPayload: GraphqlPayload<FileReadData> | null = null;

  for (let attempt = 0; attempt < 20; attempt += 1) {
    lastPayload = await runGraphql<FileReadData>(fileReadQuery, { id: fileId });
    if (lastPayload.data?.node?.fileStatus === status) {
      return lastPayload;
    }

    await delay(2000);
  }

  throw new Error(`Timed out waiting for file ${fileId} to reach ${status}: ${JSON.stringify(lastPayload, null, 2)}`);
}

function expectSingleUserError(label: string, errors: UserError[] | null | undefined, code: string): void {
  if (Array.isArray(errors) && errors.length === 1 && errors[0]?.code === code) {
    return;
  }

  throw new Error(`${label} did not return one ${code} userError: ${JSON.stringify(errors ?? null, null, 2)}`);
}

const timestamp = Date.now();
const nonReadyCreateVariables = {
  files: [
    {
      contentType: 'IMAGE',
      filename: `media-user-error-aggregation-a-${timestamp}.jpg`,
      originalSource: 'https://example.com/not-an-image.jpg',
      alt: 'Files user error aggregation seed A',
    },
    {
      contentType: 'IMAGE',
      filename: `media-user-error-aggregation-b-${timestamp}.jpg`,
      originalSource: 'https://example.com/not-an-image.jpg',
      alt: 'Files user error aggregation seed B',
    },
  ],
};
const missingDeleteFileIds = ['gid://shopify/MediaImage/900000000000001', 'gid://shopify/MediaImage/900000000000002'];
const missingUpdateFileIds = ['gid://shopify/MediaImage/900000000000003', 'gid://shopify/MediaImage/900000000000004'];
const fileDeleteMissingVariables = { fileIds: missingDeleteFileIds };
const fileUpdateMissingVariables = {
  files: [
    { id: missingUpdateFileIds[0], alt: 'Missing first' },
    { id: missingUpdateFileIds[1], alt: 'Missing second' },
  ],
};

let createdFileIds: string[] = [];

try {
  const nonReadyCreate = await runGraphql<FileCreateData>(fileCreateMutation, nonReadyCreateVariables);
  expectNoTopLevelErrors('fileCreate non-ready seed', nonReadyCreate);
  expectNoUserErrors('fileCreate non-ready seed', nonReadyCreate.data?.fileCreate?.userErrors);
  createdFileIds = requireCreatedFileIds(nonReadyCreate, 2);
  const nonReadyReads = [];
  for (const fileId of createdFileIds) {
    nonReadyReads.push(await waitForFileStatus(fileId, 'FAILED'));
  }

  const fileDeleteMissing = await runGraphql<FileDeleteData>(fileDeleteMutation, fileDeleteMissingVariables);
  expectNoTopLevelErrors('fileDelete missing ids', fileDeleteMissing);
  expectSingleUserError(
    'fileDelete missing ids',
    fileDeleteMissing.data?.fileDelete?.userErrors,
    'FILE_DOES_NOT_EXIST',
  );

  const fileUpdateMissing = await runGraphql<FileUpdateData>(fileUpdateMutation, fileUpdateMissingVariables);
  expectNoTopLevelErrors('fileUpdate missing ids', fileUpdateMissing);
  expectSingleUserError(
    'fileUpdate missing ids',
    fileUpdateMissing.data?.fileUpdate?.userErrors,
    'FILE_DOES_NOT_EXIST',
  );

  const fileAcknowledgeMixedVariables = {
    fileIds: ['gid://shopify/MediaImage/900000000000005', createdFileIds[0]],
  };
  const fileAcknowledgeMixed = await runGraphql<FileAcknowledgeData>(
    fileAcknowledgeMutation,
    fileAcknowledgeMixedVariables,
  );
  expectNoTopLevelErrors('fileAcknowledgeUpdateFailed mixed ids', fileAcknowledgeMixed);
  expectSingleUserError(
    'fileAcknowledgeUpdateFailed mixed ids',
    fileAcknowledgeMixed.data?.fileAcknowledgeUpdateFailed?.userErrors,
    'FILE_DOES_NOT_EXIST',
  );

  const fileAcknowledgeNonReadyVariables = { fileIds: createdFileIds };
  const fileAcknowledgeNonReady = await runGraphql<FileAcknowledgeData>(
    fileAcknowledgeMutation,
    fileAcknowledgeNonReadyVariables,
  );
  expectNoTopLevelErrors('fileAcknowledgeUpdateFailed non-ready ids', fileAcknowledgeNonReady);
  expectSingleUserError(
    'fileAcknowledgeUpdateFailed non-ready ids',
    fileAcknowledgeNonReady.data?.fileAcknowledgeUpdateFailed?.userErrors,
    'NON_READY_STATE',
  );

  const capture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    scenarioId: 'media-file-user-error-aggregation',
    setup: {
      nonReadyCreate: {
        variables: nonReadyCreateVariables,
        response: nonReadyCreate,
      },
      nonReadyReads,
    },
    branches: {
      fileDeleteMissing: {
        variables: fileDeleteMissingVariables,
        response: fileDeleteMissing,
      },
      fileUpdateMissing: {
        variables: fileUpdateMissingVariables,
        response: fileUpdateMissing,
      },
      fileAcknowledgeMixed: {
        variables: fileAcknowledgeMixedVariables,
        response: fileAcknowledgeMixed,
      },
      fileAcknowledgeNonReady: {
        variables: fileAcknowledgeNonReadyVariables,
        response: fileAcknowledgeNonReady,
      },
    },
    upstreamCalls: [
      {
        operationName: 'MediaFileReferencesHydrate',
        variables: { fileIds: missingDeleteFileIds },
        query: 'sha:media-file-references-hydrate',
        response: {
          status: 200,
          body: { data: { nodes: [null, null] } },
        },
      },
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: missingUpdateFileIds },
        query: 'sha:media-file-update-hydrate',
        response: {
          status: 200,
          body: { data: { nodes: [null, null] } },
        },
      },
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputFile, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputFile, createdFileIds }, null, 2));
} finally {
  if (createdFileIds.length > 0) {
    try {
      await runGraphql<FileDeleteData>(fileDeleteMutation, { fileIds: createdFileIds });
    } catch {
      // Best-effort cleanup only.
    }
  }
}
