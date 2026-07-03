/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type UserError = { field?: string[] | null; message?: string | null; code?: string | null };
type FileUpdateData = {
  fileUpdate?: {
    files?: Array<{ id?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

if (apiVersion !== '2026-04') {
  throw new Error(
    `media-file-update-missing-id-coercion requires SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`,
  );
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const outputFile = path.join(outputDir, 'file-update-missing-id-coercion.json');
const variableDocumentPath = 'config/parity-requests/media/file_update_missing_id_coercion.graphql';
const inlineDocumentPath = 'config/parity-requests/media/file_update_missing_id_coercion_inline.graphql';
const variableDocument = await readFile(variableDocumentPath, 'utf8');
const inlineDocument = await readFile(inlineDocumentPath, 'utf8');
const mediaFileUpdateHydrateQuery = `query MediaFileUpdateHydrate($fileIds: [ID!]!) {
  nodes(ids: $fileIds) {
    id
    __typename
    ... on File {
      alt
      createdAt
      fileStatus
    }
    ... on MediaImage {
      image { url width height }
      preview { image { url width height } }
    }
    ... on GenericFile {
      url
    }
  }
}`;
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const missingImageId = 'gid://shopify/MediaImage/999999999996';
const omittedIdVariables = {
  files: [{ alt: 'new alt' }],
};
const batchOmittedIdVariables = {
  files: [{ id: missingImageId, alt: 'supplied id' }, { alt: 'missing id' }],
};
const suppliedMissingIdVariables = {
  files: [{ id: missingImageId, alt: 'supplied missing id' }],
};

function requireHttpOk(label: string, response: ConformanceGraphqlResult<FileUpdateData>): void {
  if (response.status >= 200 && response.status < 300) {
    return;
  }

  throw new Error(`${label} returned HTTP ${response.status}: ${JSON.stringify(response.payload, null, 2)}`);
}

function requireTopLevelCoercionError(
  label: string,
  response: ConformanceGraphqlResult<FileUpdateData>,
  expectedCodes: string[],
): void {
  requireHttpOk(label, response);
  const errors = response.payload.errors;
  if (!Array.isArray(errors) || errors.length === 0) {
    throw new Error(`${label} did not return top-level GraphQL errors: ${JSON.stringify(response.payload, null, 2)}`);
  }

  const firstError = errors[0];
  const code =
    typeof firstError === 'object' &&
    firstError !== null &&
    'extensions' in firstError &&
    typeof firstError.extensions === 'object' &&
    firstError.extensions !== null &&
    'code' in firstError.extensions
      ? firstError.extensions.code
      : null;
  if (typeof code !== 'string' || !expectedCodes.includes(code)) {
    throw new Error(`${label} returned unexpected error code ${String(code)}: ${JSON.stringify(errors, null, 2)}`);
  }

  if (response.payload.data !== undefined && response.payload.data.fileUpdate !== null) {
    throw new Error(`${label} reached fileUpdate payload: ${JSON.stringify(response.payload.data, null, 2)}`);
  }
}

function requireSuppliedMissingUserError(label: string, response: ConformanceGraphqlResult<FileUpdateData>): void {
  requireHttpOk(label, response);
  if (response.payload.errors !== undefined) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(response.payload.errors, null, 2)}`);
  }

  const userErrors = response.payload.data?.fileUpdate?.userErrors ?? null;
  if (!Array.isArray(userErrors) || userErrors.length !== 1 || userErrors[0]?.code !== 'FILE_DOES_NOT_EXIST') {
    throw new Error(
      `${label} did not return one FILE_DOES_NOT_EXIST userError: ${JSON.stringify(userErrors, null, 2)}`,
    );
  }
}

const omittedId = await runGraphqlRaw<FileUpdateData>(variableDocument, omittedIdVariables);
requireTopLevelCoercionError('fileUpdate omitted id variables', omittedId, ['INVALID_VARIABLE']);

const batchOmittedId = await runGraphqlRaw<FileUpdateData>(variableDocument, batchOmittedIdVariables);
requireTopLevelCoercionError('fileUpdate batch omitted id variables', batchOmittedId, ['INVALID_VARIABLE']);

const inlineOmittedId = await runGraphqlRaw<FileUpdateData>(inlineDocument, {});
requireTopLevelCoercionError('fileUpdate inline omitted id', inlineOmittedId, ['missingRequiredInputObjectAttribute']);

const suppliedMissingId = await runGraphqlRaw<FileUpdateData>(variableDocument, suppliedMissingIdVariables);
requireSuppliedMissingUserError('fileUpdate supplied missing id', suppliedMissingId);

const capture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scenarioId: 'media-file-update-missing-id-coercion',
  documents: {
    variable: variableDocumentPath,
    inline: inlineDocumentPath,
  },
  cases: {
    omittedId: {
      variables: omittedIdVariables,
      response: omittedId,
    },
    batchOmittedId: {
      variables: batchOmittedIdVariables,
      response: batchOmittedId,
    },
    inlineOmittedId: {
      variables: {},
      response: inlineOmittedId,
    },
    suppliedMissingId: {
      variables: suppliedMissingIdVariables,
      response: suppliedMissingId,
    },
  },
  upstreamCalls: [
    {
      operationName: 'MediaFileUpdateHydrate',
      variables: { fileIds: [missingImageId] },
      query: mediaFileUpdateHydrateQuery,
      response: {
        status: 200,
        body: { data: { nodes: [null] } },
      },
    },
  ],
  notes:
    'Validation-only capture. Omitted FileUpdateInput.id fails GraphQL input coercion before fileUpdate resolver execution; supplied unknown id remains a resolver-level FILE_DOES_NOT_EXIST userError.',
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputFile, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({ ok: true, outputFile }, null, 2));
