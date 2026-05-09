/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;

type FileCreateData = {
  fileCreate?: {
    files?: Array<{ id?: string | null } | null> | null;
    userErrors?: Array<{ field?: string[] | null; message?: string | null; code?: string | null }> | null;
  } | null;
};

type Scenario = {
  name: string;
  variables: GraphqlVariables;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fileCreateInputValidationMutation = `#graphql
  mutation MediaFileCreateInputValidation($files: [FileCreateInput!]!) {
    fileCreate(files: $files) {
      files {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function overLengthSource(): string {
  const prefix = 'https://cdn.example.com/';
  const suffix = '.png';
  return `${prefix}${'a'.repeat(2049 - prefix.length - suffix.length)}${suffix}`;
}

const scenarios: Scenario[] = [
  {
    name: 'missing-original-source',
    variables: {
      files: [{ contentType: 'IMAGE' }],
    },
  },
  {
    name: 'empty-original-source',
    variables: {
      files: [{ originalSource: '' }],
    },
  },
  {
    name: 'over-length-original-source',
    variables: {
      files: [{ originalSource: overLengthSource() }],
    },
  },
];

function firstErrorCode(response: { payload: { errors?: unknown } }): string | null {
  const errors = response.payload.errors;
  if (!Array.isArray(errors)) {
    return null;
  }
  const first = errors[0];
  if (typeof first !== 'object' || first === null || !('extensions' in first)) {
    return null;
  }
  const extensions = (first as { extensions?: unknown }).extensions;
  if (typeof extensions !== 'object' || extensions === null || !('code' in extensions)) {
    return null;
  }
  const code = (extensions as { code?: unknown }).code;
  return typeof code === 'string' ? code : null;
}

function expectTopLevelError(
  scenario: string,
  response: { payload: { data?: FileCreateData; errors?: unknown } },
  expectedCode: string,
): void {
  const code = firstErrorCode(response);
  if (code !== expectedCode) {
    throw new Error(`${scenario} expected top-level ${expectedCode}, got ${JSON.stringify(response.payload)}`);
  }

  if (response.payload.data?.fileCreate !== null && response.payload.data?.fileCreate !== undefined) {
    throw new Error(
      `${scenario} expected null or absent data.fileCreate, got ${JSON.stringify(response.payload.data)}`,
    );
  }
}

await mkdir(outputDir, { recursive: true });

const capturedScenarios = [];
for (const scenario of scenarios) {
  const response = await runGraphqlRaw<FileCreateData>(fileCreateInputValidationMutation, scenario.variables);
  capturedScenarios.push({
    name: scenario.name,
    operationName: 'fileCreate',
    variables: scenario.variables,
    response,
  });

  expectTopLevelError(
    scenario.name,
    response,
    scenario.name === 'missing-original-source' ? 'INVALID_VARIABLE' : 'INVALID_FIELD_ARGUMENTS',
  );
}

const capture = {
  storeDomain,
  apiVersion,
  capturedAt: new Date().toISOString(),
  scenarioId: 'media-file-create-input-validation',
  operations: ['fileCreate'],
  notes: [
    'Captured Admin GraphQL 2026-04 FileCreateInput originalSource input-class validation for missing, empty, and 2049-character values.',
    'Missing originalSource is rejected during variable coercion before data.fileCreate is materialized; empty and over-length originalSource return INVALID_FIELD_ARGUMENTS with data.fileCreate null.',
  ],
  scenarios: capturedScenarios,
  upstreamCalls: [],
};

const filename = 'media-file-create-input-validation.json';
await writeFile(path.join(outputDir, filename), `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputDir,
      file: filename,
      scenarios: capturedScenarios.map((scenario) => scenario.name),
    },
    null,
    2,
  ),
);
