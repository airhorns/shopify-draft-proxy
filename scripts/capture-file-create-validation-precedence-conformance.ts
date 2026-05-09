/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;
type UserError = { field?: string[] | null; message?: string | null; code?: string | null };

type FileCreateData = {
  fileCreate?: {
    files?: Array<{ id?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};

type FileDeleteData = {
  fileDelete?: {
    deletedFileIds?: string[] | null;
    userErrors?: UserError[] | null;
  } | null;
};

const requestPath = 'config/parity-requests/media/media-file-create-validation-precedence.graphql';
const fileDeleteMutation = `#graphql
  mutation MediaFileCreateValidationPrecedenceCleanup($fileIds: [ID!]!) {
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const fileCreateValidationMutation = await readFile(requestPath, 'utf8');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function buildScenarios(runId: string): Array<{ name: string; variables: GraphqlVariables }> {
  return [
    {
      name: 'url-before-filename-mismatch',
      variables: {
        files: [
          {
            originalSource: 'data:image/jpeg;base64,abc',
            filename: 'file-create-validation-precedence-source.png',
          },
        ],
      },
    },
    {
      name: 'filename-mismatch-before-duplicate-mode',
      variables: {
        files: [
          {
            originalSource: 'https://placehold.co/600x400/file-create-validation-precedence-mismatch.png',
            filename: 'file-create-validation-precedence-mismatch.gif',
            contentType: 'VIDEO',
            duplicateResolutionMode: 'REPLACE',
          },
        ],
      },
    },
    {
      name: 'successful-create-baseline',
      variables: {
        files: [
          {
            originalSource: `https://placehold.co/600x400/file-create-validation-precedence-${runId}.png`,
            filename: `file-create-validation-precedence-${runId}.png`,
            contentType: 'IMAGE',
            duplicateResolutionMode: 'RAISE_ERROR',
            alt: `fileCreate validation precedence baseline ${runId}`,
          },
        ],
      },
    },
  ];
}

function assertNoTopLevelErrors(name: string, response: ConformanceGraphqlResult<unknown>): void {
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`${name} failed: ${JSON.stringify(response, null, 2)}`);
  }
}

function collectCreatedFileIds(response: ConformanceGraphqlResult<FileCreateData>): string[] {
  return (response.payload.data?.fileCreate?.files ?? []).flatMap((file) =>
    typeof file?.id === 'string' && file.id.length > 0 ? [file.id] : [],
  );
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const scenarios = [];
const createdFileIds: string[] = [];

for (const scenario of buildScenarios(runId)) {
  const response = await runGraphqlRaw<FileCreateData>(fileCreateValidationMutation, scenario.variables);
  assertNoTopLevelErrors(scenario.name, response);
  scenarios.push({
    name: scenario.name,
    operationName: 'fileCreate',
    variables: scenario.variables,
    response,
  });
  createdFileIds.push(...collectCreatedFileIds(response));
}

let cleanup: unknown = null;
if (createdFileIds.length > 0) {
  cleanup = await runGraphql<FileDeleteData>(fileDeleteMutation, { fileIds: createdFileIds });
}

const capture = {
  storeDomain,
  apiVersion,
  capturedAt: new Date().toISOString(),
  operations: ['fileCreate'],
  notes: [
    'Captured fileCreate validation precedence for multi-fault inputs that public Admin GraphQL 2026-04 accepts for resolver execution.',
    'Each invalid scenario returns exactly one userError for the input: URL validation short-circuits filename validation, and filename validation short-circuits duplicate-resolution-mode validation.',
  ],
  requestPath,
  scenarios,
  cleanup: {
    fileIds: createdFileIds,
    response: cleanup,
  },
  upstreamCalls: [],
};

const filename = 'media-file-create-validation-precedence.json';
await writeFile(path.join(outputDir, filename), `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      file: filename,
      scenarios: scenarios.map((scenario) => scenario.name),
      createdFileIds,
    },
    null,
    2,
  ),
);
