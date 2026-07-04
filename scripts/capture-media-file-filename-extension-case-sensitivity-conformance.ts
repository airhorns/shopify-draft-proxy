/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
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
  url?: string | null;
};
type FileCreateData = {
  fileCreate?: {
    files?: Array<{ id?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileUpdateData = {
  fileUpdate?: {
    files?: FileNode[] | null;
    userErrors?: UserError[] | null;
  } | null;
};
type HydrateData = { nodes?: Array<FileNode | null> | null };
type FileDeleteData = {
  fileDelete?: {
    deletedFileIds?: string[] | null;
    userErrors?: UserError[] | null;
  } | null;
};

const scenarioId = 'media-file-filename-extension-case-sensitivity';
const apiVersion = '2026-04';
const fixtureFilename = `${scenarioId}.json`;
const fixturePath = path.join(
  'fixtures',
  'conformance',
  'harry-test-heelo.myshopify.com',
  apiVersion,
  'media',
  fixtureFilename,
);
const specPath = path.join('config', 'parity-specs', 'media', `${scenarioId}.json`);
const fileCreateRequestPath = 'config/parity-requests/media/media-file-create-validation-branches.graphql';
const fileUpdateRequestPath = 'config/parity-requests/media/file_update_filename_extension/file-update.graphql';

const mediaFileUpdateHydrateQuery =
  'query MediaFileUpdateHydrate($fileIds: [ID!]!) {\n  nodes(ids: $fileIds) {\n    id\n    __typename\n    ... on File {\n      alt\n      createdAt\n      fileStatus\n    }\n    ... on MediaImage {\n      image { url width height }\n      preview { image { url width height } }\n    }\n    ... on GenericFile {\n      url\n    }\n  }\n}';

const fileDeleteMutation = `#graphql
  mutation MediaFileExtensionCaseSensitivityCleanup($fileIds: [ID!]!) {
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

function fileExtension(value: string | null | undefined): string {
  const pathOnly = (value ?? '').split(/[?#]/u)[0] ?? '';
  const filename = pathOnly
    .split('/')
    .reverse()
    .find((segment) => segment.length > 0);
  const dot = filename?.lastIndexOf('.') ?? -1;
  return dot >= 0 && filename ? filename.slice(dot + 1) : '';
}

function hydratedSourceUrl(node: FileNode | null | undefined): string | null {
  return node?.url ?? node?.image?.url ?? node?.preview?.image?.url ?? null;
}

function expectNoTopLevelErrors<TData>(label: string, response: ConformanceGraphqlResult<TData>): void {
  if (response.status >= 200 && response.status < 300 && response.payload.errors === undefined) {
    return;
  }
  throw new Error(`${label} returned transport/top-level errors: ${JSON.stringify(response, null, 2)}`);
}

function expectPayload(label: string, actual: unknown, expected: { files: unknown[]; userErrors: UserError[] }): void {
  if (JSON.stringify(actual) === JSON.stringify(expected)) {
    return;
  }
  throw new Error(`${label} returned unexpected payload: ${JSON.stringify(actual, null, 2)}`);
}

function requireFileId(label: string, response: ConformanceGraphqlResult<FileCreateData>): string {
  expectNoTopLevelErrors(label, response);
  const errors = response.payload.data?.fileCreate?.userErrors ?? [];
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
  const id = response.payload.data?.fileCreate?.files?.[0]?.id;
  if (typeof id === 'string' && id.length > 0) {
    return id;
  }
  throw new Error(`${label} did not return a file id: ${JSON.stringify(response.payload, null, 2)}`);
}

async function waitForReadyUppercaseFile(
  runGraphqlRaw: <TData>(query: string, variables?: GraphqlVariables) => Promise<ConformanceGraphqlResult<TData>>,
  fileId: string,
): Promise<ConformanceGraphqlResult<HydrateData>> {
  let lastResponse: ConformanceGraphqlResult<HydrateData> | null = null;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    lastResponse = await runGraphqlRaw<HydrateData>(mediaFileUpdateHydrateQuery, { fileIds: [fileId] });
    expectNoTopLevelErrors('MediaFileUpdateHydrate', lastResponse);
    const node = lastResponse.payload.data?.nodes?.[0] ?? null;
    if (node?.fileStatus === 'READY') {
      const sourceUrl = hydratedSourceUrl(node);
      const extension = fileExtension(sourceUrl);
      if (extension !== 'JPG') {
        throw new Error(
          `Hydrated READY file did not preserve uppercase .JPG source extension: ${JSON.stringify(
            { sourceUrl, extension, node },
            null,
            2,
          )}`,
        );
      }
      return lastResponse;
    }
    await delay(2000);
  }
  throw new Error(`Timed out waiting for ${fileId} to reach READY: ${JSON.stringify(lastResponse, null, 2)}`);
}

function paritySpec(): Record<string, unknown> {
  return {
    scenarioId,
    operationNames: ['fileCreate', 'fileUpdate'],
    scenarioStatus: 'captured',
    assertionKinds: ['payload-shape', 'user-errors-parity', 'side-effect-boundary'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/marketing_inventory_online_store.rs'],
    proxyRequest: {
      documentPath: fileCreateRequestPath,
      variablesCapturePath: '$.cases.fileCreateCaseMismatch.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Captured Shopify Admin GraphQL 2026-04 case-sensitive filename extension rejection for fileCreate originalSource-vs-filename comparison and fileUpdate existing GenericFile filepath-vs-filename comparison. The fileUpdate target replays the proxy handler normal MediaFileUpdateHydrate upstream read from an exact GraphQL cassette entry.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'file-create-extension-case-mismatch',
          capturePath: '$.cases.fileCreateCaseMismatch.response.payload.data.fileCreate',
          proxyPath: '$.data.fileCreate',
        },
        {
          name: 'file-update-extension-case-mismatch',
          capturePath: '$.cases.fileUpdateCaseMismatch.response.payload.data.fileUpdate',
          proxyPath: '$.data.fileUpdate',
          proxyRequest: {
            documentPath: fileUpdateRequestPath,
            variablesCapturePath: '$.cases.fileUpdateCaseMismatch.variables',
            apiVersion,
          },
        },
      ],
    },
  };
}

const config = readConformanceScriptConfig({ exitOnMissing: true });
if (config.apiVersion !== apiVersion) {
  throw new Error(`This capture must run against Admin GraphQL ${apiVersion}; got ${config.apiVersion}`);
}

const adminAccessToken = await getValidConformanceAccessToken({
  adminOrigin: config.adminOrigin,
  apiVersion: config.apiVersion,
});
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin: config.adminOrigin,
  apiVersion: config.apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fileCreateDocument = await readFile(fileCreateRequestPath, 'utf8');
const fileUpdateDocument = await readFile(fileUpdateRequestPath, 'utf8');
const runId = Date.now();
const createdFileIds: string[] = [];
let capture: Record<string, unknown> | null = null;

try {
  const fileCreateCaseVariables = {
    files: [
      {
        originalSource: `https://placehold.co/600x400.PNG?text=media-extension-case-create-${runId}`,
        filename: `media-extension-case-create-${runId}.png`,
        contentType: 'IMAGE',
      },
    ],
  };
  const fileCreateCaseMismatch = await runGraphqlRaw<FileCreateData>(fileCreateDocument, fileCreateCaseVariables);
  expectNoTopLevelErrors('fileCreate case mismatch', fileCreateCaseMismatch);
  expectPayload('fileCreate case mismatch', fileCreateCaseMismatch.payload.data?.fileCreate, {
    files: [],
    userErrors: [
      {
        field: ['files', '0', 'filename'],
        message: 'Provided filename extension must match original source.',
        code: 'MISMATCHED_FILENAME_AND_ORIGINAL_SOURCE',
      },
    ],
  });

  const setupCreateVariables = {
    files: [
      {
        originalSource: `https://placehold.co/600x400.JPG?text=media-extension-case-update-${runId}`,
        filename: `media-extension-case-update-${runId}.JPG`,
        contentType: 'FILE',
        alt: `Media extension case update ${runId}`,
      },
    ],
  };
  const setupCreate = await runGraphqlRaw<FileCreateData>(fileCreateDocument, setupCreateVariables);
  const updateFileId = requireFileId('setup fileCreate', setupCreate);
  createdFileIds.push(updateFileId);
  const hydrate = await waitForReadyUppercaseFile(runGraphqlRaw, updateFileId);

  const fileUpdateCaseVariables = {
    files: [{ id: updateFileId, filename: `media-extension-case-update-${runId}.jpg` }],
  };
  const fileUpdateCaseMismatch = await runGraphqlRaw<FileUpdateData>(fileUpdateDocument, fileUpdateCaseVariables);
  expectNoTopLevelErrors('fileUpdate case mismatch', fileUpdateCaseMismatch);
  expectPayload('fileUpdate case mismatch', fileUpdateCaseMismatch.payload.data?.fileUpdate, {
    files: [],
    userErrors: [
      {
        field: ['files'],
        message: 'The filename extension provided must match the original filename.',
        code: 'INVALID_FILENAME_EXTENSION',
      },
    ],
  });

  capture = {
    capturedAt: new Date().toISOString(),
    storeDomain: config.storeDomain,
    apiVersion: config.apiVersion,
    scenarioId,
    cases: {
      fileCreateCaseMismatch: {
        operationName: 'fileCreate',
        variables: fileCreateCaseVariables,
        response: fileCreateCaseMismatch,
      },
      fileUpdateCaseMismatch: {
        operationName: 'fileUpdate',
        variables: fileUpdateCaseVariables,
        response: fileUpdateCaseMismatch,
      },
    },
    setup: {
      fileCreate: {
        operationName: 'fileCreate',
        variables: setupCreateVariables,
        response: setupCreate,
      },
      hydrate: {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [updateFileId] },
        response: hydrate,
      },
    },
    upstreamCalls: [
      {
        operationName: 'MediaFileUpdateHydrate',
        variables: { fileIds: [updateFileId] },
        query: mediaFileUpdateHydrateQuery,
        response: {
          status: hydrate.status,
          body: hydrate.payload,
        },
      },
    ],
  };
} finally {
  let cleanup: ConformanceGraphqlResult<FileDeleteData> | null = null;
  if (createdFileIds.length > 0) {
    cleanup = await runGraphqlRaw<FileDeleteData>(fileDeleteMutation, { fileIds: createdFileIds });
  }
  if (capture) {
    capture['cleanup'] = {
      operationName: 'fileDelete',
      variables: { fileIds: createdFileIds },
      response: cleanup,
    };
    await mkdir(path.dirname(fixturePath), { recursive: true });
    await mkdir(path.dirname(specPath), { recursive: true });
    await writeFile(fixturePath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
    await writeFile(specPath, `${JSON.stringify(paritySpec(), null, 2)}\n`, 'utf8');
    console.log(
      JSON.stringify(
        {
          ok: true,
          scenarioId,
          fixturePath,
          specPath,
          createdFileIds,
        },
        null,
        2,
      ),
    );
  }
}
