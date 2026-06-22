/* oxlint-disable no-console -- CLI script reports capture status to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = { field?: string[] | null; message?: string | null; code?: string | null };
type FileCreateFile = {
  id?: string | null;
  alt?: string | null;
  createdAt?: string | null;
  updatedAt?: string | null;
  fileStatus?: string | null;
};
type FileCreateData = {
  fileCreate?: {
    files?: Array<FileCreateFile | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileDeleteData = {
  fileDelete?: {
    deletedFileIds?: string[] | null;
    userErrors?: UserError[] | null;
  } | null;
};
type JsonRecord = Record<string, unknown>;

const fileCount = 60;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const requestPath = path.join('config', 'parity-requests', 'media', 'media-file-create-large-batch-timestamps.graphql');

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'media',
  'media-file-create-large-batch-timestamps.json',
);

const fileDeleteMutation = `#graphql
  mutation MediaFileCreateLargeBatchCleanup($fileIds: [ID!]!) {
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

function absolutePath(relativePath: string): string {
  return path.join(repoRoot, relativePath);
}

function files(count: number): JsonRecord[] {
  return Array.from({ length: count }, (_, index) => {
    const fileNumber = index + 1;
    return {
      originalSource: `https://placehold.co/600x400/media-file-create-large-batch-timestamps-${fileNumber}.png`,
      filename: `media-file-create-large-batch-timestamps-${fileNumber}.png`,
      contentType: 'IMAGE',
      alt: `Media file create large batch timestamp ${fileNumber}`,
    };
  });
}

function expectNoUserErrors(pathLabel: string, userErrors: UserError[] | null | undefined): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${pathLabel} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function expectTimestamp(pathLabel: string, value: unknown): string {
  if (typeof value !== 'string') {
    throw new Error(`${pathLabel} should be a string timestamp, got ${JSON.stringify(value)}`);
  }

  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:[0-5]\d(?:\.\d+)?Z$/u.test(value)) {
    throw new Error(`${pathLabel} should be a well-formed UTC timestamp, got ${value}`);
  }

  if (Number.isNaN(Date.parse(value))) {
    throw new Error(`${pathLabel} should parse as a Date, got ${value}`);
  }

  return value;
}

function assertFileCreateResponse(response: ConformanceGraphqlResult<FileCreateData>): string[] {
  if (response.status !== 200) {
    throw new Error(`fileCreate returned HTTP ${response.status}: ${JSON.stringify(response.payload, null, 2)}`);
  }

  if (response.payload.errors) {
    throw new Error(`fileCreate returned top-level errors: ${JSON.stringify(response.payload.errors, null, 2)}`);
  }

  const payload = response.payload.data?.fileCreate;
  expectNoUserErrors('fileCreate', payload?.userErrors);

  const createdFiles = payload?.files;
  if (!Array.isArray(createdFiles) || createdFiles.length !== fileCount) {
    throw new Error(`Expected ${fileCount} created files, got ${JSON.stringify(createdFiles, null, 2)}`);
  }

  return createdFiles.map((file, index) => {
    if (!file) {
      throw new Error(`fileCreate files[${index}] is null`);
    }

    const id = file.id;
    if (typeof id !== 'string' || !id.startsWith('gid://shopify/MediaImage/')) {
      throw new Error(`fileCreate files[${index}].id should be a MediaImage gid, got ${JSON.stringify(id)}`);
    }

    const createdAt = expectTimestamp(`fileCreate files[${index}].createdAt`, file.createdAt);
    const updatedAt = expectTimestamp(`fileCreate files[${index}].updatedAt`, file.updatedAt);
    if (Date.parse(createdAt) > Date.parse(updatedAt)) {
      throw new Error(`fileCreate files[${index}] has createdAt after updatedAt`);
    }

    return id;
  });
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const document = await readFile(absolutePath(requestPath), 'utf8');
const variables = { files: files(fileCount) };
let createdFileIds: string[] = [];
let cleanupResponse: ConformanceGraphqlResult<FileDeleteData> | null = null;
let createResponse: ConformanceGraphqlResult<FileCreateData> | null = null;

try {
  createResponse = await runGraphqlRequest<FileCreateData>(document, variables);
  createdFileIds = assertFileCreateResponse(createResponse);
} finally {
  if (createdFileIds.length > 0) {
    cleanupResponse = await runGraphqlRequest<FileDeleteData>(fileDeleteMutation, { fileIds: createdFileIds });
    expectNoUserErrors('fileDelete cleanup', cleanupResponse.payload.data?.fileDelete?.userErrors);
  }
}

if (createResponse === null) {
  throw new Error('fileCreate did not run');
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  document,
  create: {
    variables,
    response: createResponse,
  },
  cleanup: cleanupResponse
    ? {
        variables: { fileIds: createdFileIds },
        response: cleanupResponse,
      }
    : null,
  upstreamCalls: [],
};

const absoluteFixturePath = absolutePath(fixturePath);
await mkdir(path.dirname(absoluteFixturePath), { recursive: true });
await writeFile(absoluteFixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

process.stdout.write(`${JSON.stringify({ fixturePath, createdFileCount: createdFileIds.length }, null, 2)}\n`);
