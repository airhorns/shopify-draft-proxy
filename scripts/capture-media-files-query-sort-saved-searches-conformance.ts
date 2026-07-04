/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type UserError = { field?: string[] | null; message?: string | null; code?: string | null };
type GraphqlPayload<TData = unknown> = {
  data?: TData;
  errors?: unknown;
  extensions?: unknown;
};
type FileNode = {
  id?: string | null;
  __typename?: string | null;
  fileStatus?: string | null;
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
type SavedSearchCreateData = {
  savedSearchCreate?: {
    savedSearch?: {
      id?: string | null;
      name?: string | null;
      query?: string | null;
      resourceType?: string | null;
    } | null;
    userErrors?: UserError[] | null;
  } | null;
};
type SavedSearchDeleteData = {
  savedSearchDelete?: {
    deletedSavedSearchId?: string | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileSavedSearchesData = {
  fileSavedSearches?: {
    nodes?: Array<{ id?: string | null; name?: string | null } | null> | null;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: JsonRecord) => Promise<GraphqlPayload<TData>>;
};

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const requestDir = path.join('config', 'parity-requests', 'media');
const specDir = path.join('config', 'parity-specs', 'media');
const runId = `${Date.now()}`;
const filenamePrefix = `files_query_sort_${runId}`;
const savedSearchName = `Files query sort ${runId}`;

const fileCreateDocument = `mutation MediaFilesQuerySortCreate($files: [FileCreateInput!]!) {
  fileCreate(files: $files) {
    files {
      __typename
      id
      alt
      fileStatus
    }
    userErrors {
      field
      message
    }
  }
}
`;

const savedSearchCreateDocument = `mutation MediaFilesQuerySortSavedSearchCreate($input: SavedSearchCreateInput!) {
  savedSearchCreate(input: $input) {
    savedSearch {
      id
      name
      query
      resourceType
    }
    userErrors {
      field
      message
    }
  }
}
`;

const filesReadDocument = `query MediaFilesQuerySortSavedSearchesRead(
  $exactFilenameQuery: String!
  $imageQuery: String!
  $prefixQuery: String!
  $savedSearchId: ID!
) {
  filename: files(first: 10, query: $exactFilenameQuery) {
    nodes {
      __typename
      id
      alt
      fileStatus
    }
  }
  imageType: files(first: 10, query: $imageQuery, sortKey: FILENAME) {
    nodes {
      __typename
      alt
    }
  }
  byFilename: files(first: 10, query: $prefixQuery, sortKey: FILENAME) {
    nodes {
      __typename
      alt
    }
  }
  byFilenameReverse: files(first: 10, query: $prefixQuery, sortKey: FILENAME, reverse: true) {
    nodes {
      __typename
      alt
    }
  }
  savedSearchFiles: files(first: 10, savedSearchId: $savedSearchId, sortKey: FILENAME) {
    nodes {
      __typename
      alt
    }
  }
  fileSavedSearches(first: 10) {
    nodes {
      id
      name
      query
      resourceType
    }
  }
}
`;

const savedSearchDeleteDocument = `mutation MediaFilesQuerySortSavedSearchDelete($input: SavedSearchDeleteInput!) {
  savedSearchDelete(input: $input) {
    deletedSavedSearchId
    userErrors {
      field
      message
    }
  }
}
`;

const fileDeleteDocument = `mutation MediaFilesQuerySortDelete($fileIds: [ID!]!) {
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

const fileSavedSearchesCleanupDocument = `query MediaFilesQuerySortSavedSearchCleanup {
  fileSavedSearches(first: 50) {
    nodes {
      id
      name
    }
  }
}
`;

function expectNoTopLevelErrors(label: string, payload: GraphqlPayload): void {
  if (payload.errors === undefined) return;
  throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload.errors, null, 2)}`);
}

function expectNoUserErrors(label: string, errors: UserError[] | null | undefined): void {
  if (Array.isArray(errors) && errors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(errors ?? null, null, 2)}`);
}

function requireString(value: unknown, label: string): string {
  if (typeof value === 'string' && value.length > 0) return value;
  throw new Error(`Missing ${label}: ${JSON.stringify(value ?? null, null, 2)}`);
}

function createdFileIds(payload: GraphqlPayload<FileCreateData>): string[] {
  return (payload.data?.fileCreate?.files ?? []).map((file, index) =>
    requireString(file?.id, `created file ${index} id`),
  );
}

async function run<TData>(label: string, query: string, variables: JsonRecord = {}): Promise<GraphqlPayload<TData>> {
  const response = await runGraphql<TData>(query, variables);
  expectNoTopLevelErrors(label, response);
  return response;
}

async function writeText(relativePath: string, body: string): Promise<void> {
  await mkdir(path.dirname(relativePath), { recursive: true });
  await writeFile(relativePath, body, 'utf8');
}

async function writeJson(relativePath: string, body: unknown): Promise<void> {
  await writeText(relativePath, `${JSON.stringify(body, null, 2)}\n`);
}

async function cleanupExistingSavedSearches(): Promise<void> {
  const read = await run<FileSavedSearchesData>(
    'files query sort stale saved-search read',
    fileSavedSearchesCleanupDocument,
  );
  for (const node of read.data?.fileSavedSearches?.nodes ?? []) {
    const id = node?.id;
    const name = node?.name;
    if (typeof id !== 'string' || typeof name !== 'string' || !name.startsWith('Files query sort ')) {
      continue;
    }
    try {
      const cleanup = await run<SavedSearchDeleteData>(
        'files query sort stale savedSearchDelete',
        savedSearchDeleteDocument,
        {
          input: { id },
        },
      );
      expectNoUserErrors('files query sort stale savedSearchDelete', cleanup.data?.savedSearchDelete?.userErrors);
    } catch (error) {
      console.warn(
        `[capture-media-files-query-sort-saved-searches] stale saved-search cleanup failed: ${String(error)}`,
      );
    }
  }
}

function readNodeCount(payload: GraphqlPayload, key: string): number {
  const data = payload.data as Record<string, { nodes?: unknown[] } | undefined> | undefined;
  const nodes = data?.[key]?.nodes;
  return Array.isArray(nodes) ? nodes.length : 0;
}

async function waitForIndexedFiles(readVariables: JsonRecord): Promise<GraphqlPayload> {
  let lastRead: GraphqlPayload | null = null;
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    lastRead = await run('files query sort read', filesReadDocument, readVariables);
    if (
      readNodeCount(lastRead, 'filename') === 1 &&
      readNodeCount(lastRead, 'imageType') === 2 &&
      readNodeCount(lastRead, 'byFilename') === 3 &&
      readNodeCount(lastRead, 'savedSearchFiles') === 3
    ) {
      return lastRead;
    }
    await sleep(5000);
  }
  throw new Error(`files(query:) did not index created files in time: ${JSON.stringify(lastRead, null, 2)}`);
}

const createdFileIdsForCleanup: string[] = [];
let savedSearchIdForCleanup: string | null = null;

try {
  await cleanupExistingSavedSearches();

  const filenames = {
    zulu: `${filenamePrefix}_zulu.jpg`,
    alpha: `${filenamePrefix}_alpha.pdf`,
    middle: `${filenamePrefix}_middle.jpg`,
  };
  const createVariables = {
    files: [
      {
        alt: 'Files query sort zulu image',
        contentType: 'IMAGE',
        filename: filenames.zulu,
        originalSource: 'https://placehold.co/600x400.jpg',
      },
      {
        alt: 'Files query sort alpha file',
        contentType: 'FILE',
        filename: filenames.alpha,
        originalSource: 'https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf',
      },
      {
        alt: 'Files query sort middle image',
        contentType: 'IMAGE',
        filename: filenames.middle,
        originalSource: 'https://placehold.co/800x600.jpg',
      },
    ],
  };
  const create = await run<FileCreateData>('files query sort fileCreate', fileCreateDocument, createVariables);
  expectNoUserErrors('files query sort fileCreate', create.data?.fileCreate?.userErrors);
  createdFileIdsForCleanup.push(...createdFileIds(create));

  const savedSearchCreateVariables = {
    input: {
      resourceType: 'FILE',
      name: savedSearchName,
      query: `filename:${filenamePrefix}*`,
    },
  };
  const savedSearchCreate = await run<SavedSearchCreateData>(
    'files query sort savedSearchCreate',
    savedSearchCreateDocument,
    savedSearchCreateVariables,
  );
  expectNoUserErrors('files query sort savedSearchCreate', savedSearchCreate.data?.savedSearchCreate?.userErrors);
  const savedSearchId = requireString(
    savedSearchCreate.data?.savedSearchCreate?.savedSearch?.id,
    'savedSearchCreate savedSearch id',
  );
  savedSearchIdForCleanup = savedSearchId;

  const readVariables = {
    exactFilenameQuery: `filename:${filenamePrefix}_alpha*`,
    imageQuery: `filename:${filenamePrefix}* media_type:IMAGE`,
    prefixQuery: `filename:${filenamePrefix}*`,
    savedSearchId,
  };
  const read = await waitForIndexedFiles(readVariables);

  const capturedAt = new Date().toISOString();
  const fixturePath = path.join(fixtureDir, 'files-query-sort-saved-searches.json');

  await writeJson(fixturePath, {
    capturedAt,
    storeDomain,
    apiVersion,
    scenarioId: 'media-files-query-sort-saved-searches',
    create: { request: { query: fileCreateDocument, variables: createVariables }, response: create },
    savedSearchCreate: {
      request: { query: savedSearchCreateDocument, variables: savedSearchCreateVariables },
      response: savedSearchCreate,
    },
    read: { request: { query: filesReadDocument, variables: readVariables }, response: read },
    upstreamCalls: [],
  });

  await writeText(path.join(requestDir, 'files-query-sort-saved-searches-create.graphql'), fileCreateDocument);
  await writeText(
    path.join(requestDir, 'files-query-sort-saved-searches-saved-search-create.graphql'),
    savedSearchCreateDocument,
  );
  await writeText(path.join(requestDir, 'files-query-sort-saved-searches-read.graphql'), filesReadDocument);

  const readTargetProxyRequest = {
    documentPath: 'config/parity-requests/media/files-query-sort-saved-searches-read.graphql',
    variables: {
      exactFilenameQuery: { fromCapturePath: '$.read.request.variables.exactFilenameQuery' },
      imageQuery: { fromCapturePath: '$.read.request.variables.imageQuery' },
      prefixQuery: { fromCapturePath: '$.read.request.variables.prefixQuery' },
      savedSearchId: {
        fromProxyResponse: 'saved-search-create-setup',
        path: '$.data.savedSearchCreate.savedSearch.id',
      },
    },
    apiVersion,
  };

  await writeJson(path.join(specDir, 'files-query-sort-saved-searches.json'), {
    scenarioId: 'media-files-query-sort-saved-searches',
    operationNames: ['files', 'fileSavedSearches', 'fileCreate', 'savedSearchCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['downstream-read-parity', 'search-filter-parity', 'sort-order-parity'],
    liveCaptureFiles: [fixturePath],
    runtimeTestFiles: ['tests/graphql_routes/marketing_inventory_online_store.rs'],
    proxyRequest: {
      documentPath: 'config/parity-requests/media/files-query-sort-saved-searches-create.graphql',
      variablesCapturePath: '$.create.request.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'Captured 2026-04 Files API evidence for files(query:) filename/media_type filters, FileSortKeys.FILENAME ordering with reverse, FILE saved-search reads, and files(savedSearchId:) resolution. The read query scopes records by a unique filename prefix so the live store catalog does not leak unrelated files into the comparison.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'file-create-setup-user-errors',
          capturePath: '$.create.response.data.fileCreate.userErrors',
          proxyPath: '$.data.fileCreate.userErrors',
        },
        {
          name: 'saved-search-create-setup',
          capturePath: '$.savedSearchCreate.response.data.savedSearchCreate',
          proxyPath: '$.data.savedSearchCreate',
          proxyRequest: {
            documentPath: 'config/parity-requests/media/files-query-sort-saved-searches-saved-search-create.graphql',
            variablesCapturePath: '$.savedSearchCreate.request.variables',
            apiVersion,
          },
          expectedDifferences: [
            {
              path: '$.savedSearch.id',
              matcher: 'shopify-gid:SavedSearch',
              reason: 'The proxy generates a local SavedSearch GID; Shopify returns the dev-store SavedSearch GID.',
            },
          ],
        },
        {
          name: 'files-query-filename-filter',
          capturePath: '$.read.response.data.filename',
          proxyPath: '$.data.filename',
          proxyRequest: readTargetProxyRequest,
          expectedDifferences: [
            {
              path: '$.nodes[*].id',
              matcher: 'shopify-gid:GenericFile',
              reason: 'The proxy generates local file GIDs; Shopify returns dev-store file GIDs.',
            },
            {
              path: '$.nodes[*].fileStatus',
              ignore: true,
              regrettable: true,
              reason:
                'Shopify file processing status can advance asynchronously; the proxy keeps deterministic staged status.',
            },
          ],
        },
        {
          name: 'files-query-media-type-filter',
          capturePath: '$.read.response.data.imageType',
          proxyPath: '$.data.imageType',
          proxyRequest: readTargetProxyRequest,
        },
        {
          name: 'files-sort-filename',
          capturePath: '$.read.response.data.byFilename',
          proxyPath: '$.data.byFilename',
          proxyRequest: readTargetProxyRequest,
        },
        {
          name: 'files-sort-filename-reverse',
          capturePath: '$.read.response.data.byFilenameReverse',
          proxyPath: '$.data.byFilenameReverse',
          proxyRequest: readTargetProxyRequest,
        },
        {
          name: 'files-saved-search-id-filter',
          capturePath: '$.read.response.data.savedSearchFiles',
          proxyPath: '$.data.savedSearchFiles',
          proxyRequest: readTargetProxyRequest,
        },
        {
          name: 'file-saved-searches-read',
          capturePath: '$.read.response.data.fileSavedSearches',
          proxyPath: '$.data.fileSavedSearches',
          proxyRequest: readTargetProxyRequest,
          expectedDifferences: [
            {
              path: '$.nodes[*].id',
              matcher: 'shopify-gid:SavedSearch',
              reason: 'The proxy generates a local SavedSearch GID; Shopify returns the dev-store SavedSearch GID.',
            },
          ],
        },
      ],
    },
  });
} finally {
  if (savedSearchIdForCleanup) {
    try {
      const cleanup = await run<SavedSearchDeleteData>(
        'files query sort savedSearchDelete',
        savedSearchDeleteDocument,
        {
          input: { id: savedSearchIdForCleanup },
        },
      );
      expectNoUserErrors('files query sort savedSearchDelete', cleanup.data?.savedSearchDelete?.userErrors);
    } catch (error) {
      console.warn(`[capture-media-files-query-sort-saved-searches] saved-search cleanup failed: ${String(error)}`);
    }
  }
  if (createdFileIdsForCleanup.length > 0) {
    try {
      const cleanup = await run<FileDeleteData>('files query sort fileDelete', fileDeleteDocument, {
        fileIds: createdFileIdsForCleanup,
      });
      expectNoUserErrors('files query sort fileDelete', cleanup.data?.fileDelete?.userErrors);
    } catch (error) {
      console.warn(`[capture-media-files-query-sort-saved-searches] file cleanup failed: ${String(error)}`);
    }
  }
}
