/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-update-error-codes.json');
const requestDir = path.join('config', 'parity-requests', 'metaobjects');
const badIdDocument = await readFile(
  path.join(requestDir, 'metaobject-update-error-codes-update-bad-id.graphql'),
  'utf8',
);
const duplicateCreateDocument = await readFile(
  path.join(requestDir, 'metaobject-update-error-codes-duplicate-create.graphql'),
  'utf8',
);
const displayUpdateDocument = await readFile(
  path.join(requestDir, 'metaobject-update-error-codes-display-update.graphql'),
  'utf8',
);

const runId = Date.now().toString();
const type = `codex_update_errors_${runId}`;
const displayHandle = `update-errors-display-${runId}`;
const badMetaobjectId = `gid://shopify/Metaobject/999999${runId}`;

const definitionCreateMutation = `#graphql
  mutation MetaobjectUpdateErrorCodesDefinitionCreate($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        id
        type
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const metaobjectCreateMutation = `#graphql
  mutation MetaobjectUpdateErrorCodesSetupCreate($metaobject: MetaobjectCreateInput!) {
    metaobjectCreate(metaobject: $metaobject) {
      metaobject {
        id
        handle
        displayName
        updatedAt
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const metaobjectDeleteMutation = `#graphql
  mutation MetaobjectUpdateErrorCodesDelete($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const definitionDeleteMutation = `#graphql
  mutation MetaobjectUpdateErrorCodesDefinitionDelete($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const definitionHydrateQuery =
  'query MetaobjectDefinitionHydrateByType($type: String!) { metaobjectDefinitionByType(type: $type) { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }';

const metaobjectHydrateByIdQuery =
  'query MetaobjectHydrateById($id: ID!) { metaobject(id: $id) { id handle type displayName createdAt updatedAt capabilities { publishable { status } onlineStore { templateSuffix } } fields { key type value jsonValue definition { key name required type { name category } } } definition { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } } }';

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (!object) {
      return undefined;
    }
    current = object[part];
  }
  return current;
}

function extractString(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }
  return value;
}

function extractUserErrors(payload: unknown, pathParts: string[]): unknown[] {
  const value = readPath(payload, pathParts);
  return Array.isArray(value) ? value : [];
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const errors = extractUserErrors(payload, pathParts);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors, null, 2)}`);
  }
}

function assertHasUserErrorCode(payload: unknown, pathParts: string[], code: string, label: string): void {
  const errors = extractUserErrors(payload, pathParts);
  if (!errors.some((error) => readObject(error)?.['code'] === code)) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(errors, null, 2)}`);
  }
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const client = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

let definitionId: string | null = null;
let displayMetaobjectId: string | null = null;

try {
  const definitionVariables = {
    definition: {
      type,
      name: `Update errors ${runId}`,
      displayNameKey: 'title',
      fieldDefinitions: [
        {
          key: 'title',
          name: 'Title',
          type: 'single_line_text_field',
          required: true,
        },
        {
          key: 'body',
          name: 'Body',
          type: 'multi_line_text_field',
          required: false,
        },
      ],
    },
  };
  const definitionCreate = await client.runGraphqlRequest(definitionCreateMutation, definitionVariables);
  assertGraphqlOk(definitionCreate, 'definition create');
  assertNoUserErrors(
    definitionCreate.payload,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'definition create',
  );
  definitionId = extractString(
    definitionCreate.payload,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition create',
  );

  const displayCreateVariables = {
    metaobject: {
      type,
      handle: displayHandle,
      fields: [
        { key: 'title', value: `Display ${runId}` },
        { key: 'body', value: `Body ${runId}` },
      ],
    },
  };
  const displayCreate = await client.runGraphqlRequest(metaobjectCreateMutation, displayCreateVariables);
  assertGraphqlOk(displayCreate, 'display setup create');
  assertNoUserErrors(displayCreate.payload, ['data', 'metaobjectCreate', 'userErrors'], 'display setup create');
  displayMetaobjectId = extractString(
    displayCreate.payload,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'display setup create',
  );

  const badIdHydrate = await client.runGraphqlRequest(metaobjectHydrateByIdQuery, { id: badMetaobjectId });
  assertGraphqlOk(badIdHydrate, 'bad id hydrate');

  const definitionHydrate = await client.runGraphqlRequest(definitionHydrateQuery, { type });
  assertGraphqlOk(definitionHydrate, 'definition hydrate');

  const displayHydrate = await client.runGraphqlRequest(metaobjectHydrateByIdQuery, { id: displayMetaobjectId });
  assertGraphqlOk(displayHydrate, 'display hydrate');

  const updateWithBadIdVariables = {
    id: badMetaobjectId,
    metaobject: {
      fields: [{ key: 'title', value: 'Nope' }],
    },
  };
  const updateWithBadId = await client.runGraphqlRequest(badIdDocument, updateWithBadIdVariables);
  assertGraphqlOk(updateWithBadId, 'update with bad id');
  assertHasUserErrorCode(
    updateWithBadId.payload,
    ['data', 'metaobjectUpdate', 'userErrors'],
    'RECORD_NOT_FOUND',
    'update with bad id',
  );

  const duplicateFieldInputVariables = {
    metaobject: {
      type,
      fields: [
        { key: 'title', value: `First ${runId}` },
        { key: 'title', value: `Second ${runId}` },
      ],
    },
  };
  const duplicateFieldInput = await client.runGraphqlRequest(duplicateCreateDocument, duplicateFieldInputVariables);
  assertGraphqlOk(duplicateFieldInput, 'duplicate field input');
  assertHasUserErrorCode(
    duplicateFieldInput.payload,
    ['data', 'metaobjectCreate', 'userErrors'],
    'DUPLICATE_FIELD_INPUT',
    'duplicate field input',
  );

  const displayNameUntouchedUpdateVariables = {
    id: displayMetaobjectId,
    metaobject: {
      fields: [{ key: 'body', value: `Changed body ${runId}` }],
    },
  };
  const displayNameUntouchedUpdate = await client.runGraphqlRequest(
    displayUpdateDocument,
    displayNameUntouchedUpdateVariables,
  );
  assertGraphqlOk(displayNameUntouchedUpdate, 'display name untouched update');
  assertNoUserErrors(
    displayNameUntouchedUpdate.payload,
    ['data', 'metaobjectUpdate', 'userErrors'],
    'display name untouched update',
  );

  const fixture = {
    apiVersion,
    storeDomain,
    capturedAt: new Date().toISOString(),
    scenarioId: 'metaobject-update-error-codes',
    notes:
      'Captured live 2026-04 evidence for metaobjectUpdate bad-id RECORD_NOT_FOUND, duplicate fields[] key validation, and non-display-field update displayName preservation.',
    setup: {
      definitionCreate: captureFromResult(
        'definition-create',
        definitionCreateMutation,
        definitionVariables,
        definitionCreate,
      ),
      displayCreate: captureFromResult(
        'display-create',
        metaobjectCreateMutation,
        displayCreateVariables,
        displayCreate,
      ),
    },
    cases: {
      updateWithBadId: captureFromResult(
        'update-with-bad-id',
        badIdDocument,
        updateWithBadIdVariables,
        updateWithBadId,
      ),
      duplicateFieldInput: captureFromResult(
        'duplicate-field-input',
        duplicateCreateDocument,
        duplicateFieldInputVariables,
        duplicateFieldInput,
      ),
      displayNameUntouchedUpdate: captureFromResult(
        'display-name-untouched-update',
        displayUpdateDocument,
        displayNameUntouchedUpdateVariables,
        displayNameUntouchedUpdate,
      ),
    },
    upstreamCalls: [
      {
        operationName: 'MetaobjectHydrateById',
        variables: { id: badMetaobjectId },
        query: 'sha:captured-by-script',
        response: { status: badIdHydrate.status, body: badIdHydrate.payload },
      },
      {
        operationName: 'MetaobjectDefinitionHydrateByType',
        variables: { type },
        query: 'sha:captured-by-script',
        response: { status: definitionHydrate.status, body: definitionHydrate.payload },
      },
      {
        operationName: 'MetaobjectHydrateById',
        variables: { id: displayMetaobjectId },
        query: 'sha:captured-by-script',
        response: { status: displayHydrate.status, body: displayHydrate.payload },
      },
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
} finally {
  if (displayMetaobjectId) {
    try {
      await client.runGraphqlRequest(metaobjectDeleteMutation, { id: displayMetaobjectId });
    } catch (error) {
      console.warn(`Failed to cleanup metaobject ${displayMetaobjectId}:`, error);
    }
  }
  if (definitionId) {
    try {
      await client.runGraphqlRequest(definitionDeleteMutation, { id: definitionId });
    } catch (error) {
      console.warn(`Failed to cleanup metaobject definition ${definitionId}:`, error);
    }
  }
}
