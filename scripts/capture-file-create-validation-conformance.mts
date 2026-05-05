/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fileCreateValidationMutation = `#graphql
  mutation MediaFileCreateValidationBranches($files: [FileCreateInput!]!) {
    fileCreate(files: $files) {
      files {
        id
        alt
        createdAt
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
  mutation MediaFileCreateValidationCleanup($fileIds: [ID!]!) {
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

function buildScenarios(runId: string): Array<{ name: string; variables: GraphqlVariables }> {
  return [
    {
      name: 'data-url-source',
      variables: {
        files: [{ originalSource: 'data:image/png;base64,iVBORw0KGgo=' }],
      },
    },
    {
      name: 'filename-extension-mismatch',
      variables: {
        files: [{ originalSource: 'https://placehold.co/600x400/har700-mismatch.png', filename: 'har700.jpg' }],
      },
    },
    {
      name: 'replace-mode-missing-content-type',
      variables: {
        files: [
          {
            originalSource: `https://placehold.co/600x400/har700-replace-missing-${runId}.png`,
            duplicateResolutionMode: 'REPLACE',
          },
        ],
      },
    },
    {
      name: 'replace-mode-missing-filename',
      variables: {
        files: [
          {
            originalSource: `https://placehold.co/600x400/har700-replace-filename-${runId}.png`,
            contentType: 'IMAGE',
            duplicateResolutionMode: 'REPLACE',
          },
        ],
      },
    },
    {
      name: 'successful-create',
      variables: {
        files: [
          {
            originalSource: `https://placehold.co/600x400/har700-success-${runId}.png`,
            filename: `har700-success-${runId}.png`,
            contentType: 'IMAGE',
            duplicateResolutionMode: 'RAISE_ERROR',
            alt: `HAR-700 fileCreate validation success ${runId}`,
          },
        ],
      },
    },
  ];
}

await mkdir(outputDir, { recursive: true });

const runId = `${Date.now()}`;
const scenarios = [];
const createdFileIds: string[] = [];

for (const scenario of buildScenarios(runId)) {
  const response = await runGraphqlRaw<FileCreateData>(fileCreateValidationMutation, scenario.variables);
  scenarios.push({
    name: scenario.name,
    operationName: 'fileCreate',
    variables: scenario.variables,
    response,
  });

  for (const file of response.payload.data?.fileCreate?.files ?? []) {
    if (typeof file?.id === 'string' && file.id.length > 0) {
      createdFileIds.push(file.id);
    }
  }
}

const referencesToAddSchemaProbe = await runGraphqlRaw<FileCreateData>(fileCreateValidationMutation, {
  files: [
    {
      originalSource: 'https://placehold.co/600x400/har700-reference-probe.png',
      referencesToAdd: ['gid://shopify/Product/1', 'gid://shopify/Product/2'],
    },
  ],
});

const longAltPublicProbe = await runGraphqlRaw<FileCreateData>(fileCreateValidationMutation, {
  files: [
    {
      originalSource: `https://placehold.co/600x400/har700-long-alt-${runId}.png`,
      filename: `har700-long-alt-${runId}.png`,
      contentType: 'IMAGE',
      alt: `HAR-700 long alt ${'a'.repeat(513)}`,
    },
  ],
});

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
    'Captured fileCreate validation branches that the public Admin schema accepts for execution.',
    'Current public Admin API schemas through unstable do not expose referencesToAdd on FileCreateInput; the referencesToAdd probe is recorded as a schema-level INVALID_VARIABLE response and local runtime tests cover the ticket-required private-core guardrail.',
    'Current public Admin GraphQL 2026-04 still emits ALT_VALUE_LIMIT_EXCEEDED for 513-character alt input; the long-alt probe records that public response while local runtime tests cover the ticket-requested private-core removal.',
  ],
  scenarios,
  referencesToAddSchemaProbe,
  longAltPublicProbe,
  cleanup: {
    fileIds: createdFileIds,
    response: cleanup,
  },
  upstreamCalls: [],
};

const filename = 'media-file-create-validation-branches.json';
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
