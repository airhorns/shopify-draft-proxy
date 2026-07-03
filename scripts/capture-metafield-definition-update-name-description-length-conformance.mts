/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedInteraction = {
  request: {
    documentPath?: string;
    query?: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type UserError = {
  field?: unknown;
  message?: unknown;
  code?: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const requestDir = path.join('config', 'parity-requests', 'metafields');
const specDir = path.join('config', 'parity-specs', 'metafields');
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');

const createDocumentPath = path.join(requestDir, 'metafield-definition-update-name-description-length-create.graphql');
const updateDocumentPath = path.join(requestDir, 'metafield-definition-update-name-description-length-update.graphql');
const readDocumentPath = path.join(requestDir, 'metafield-definition-update-name-description-length-read.graphql');
const specPath = path.join(specDir, 'metafield-definition-update-name-description-length.json');
const fixturePath = path.join(fixtureDir, 'metafield-definition-update-name-description-length.json');

const createDocument = `mutation MetafieldDefinitionUpdateNameDescriptionLengthCreate($definition: MetafieldDefinitionInput!) {
  metafieldDefinitionCreate(definition: $definition) {
    createdDefinition {
      id
      namespace
      key
      name
      description
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const updateDocument = `mutation MetafieldDefinitionUpdateNameDescriptionLengthUpdate($definition: MetafieldDefinitionUpdateInput!) {
  metafieldDefinitionUpdate(definition: $definition) {
    updatedDefinition {
      namespace
      key
      name
      description
    }
    userErrors {
      __typename
      field
      message
      code
    }
    validationJob {
      id
    }
  }
}
`;

const readDocument = `query MetafieldDefinitionUpdateNameDescriptionLengthRead($identifier: MetafieldDefinitionIdentifierInput!) {
  metafieldDefinition(identifier: $identifier) {
    namespace
    key
    name
    description
  }
}
`;

const deleteDefinitionMutation = `#graphql
mutation MetafieldDefinitionUpdateNameDescriptionLengthCleanup($id: ID!) {
  metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
    deletedDefinitionId
    userErrors {
      field
      message
      code
    }
  }
}
`;

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, parts: string[]): unknown {
  let cursor: unknown = value;
  for (const part of parts) {
    cursor = readObject(cursor)?.[part];
  }
  return cursor;
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} was not returned: ${JSON.stringify(value)}`);
  }
  return value;
}

function userErrors(capture: CapturedInteraction, root: string): UserError[] {
  const errors = readPath(capture.response, ['data', root, 'userErrors']);
  return Array.isArray(errors) ? (errors as UserError[]) : [];
}

function assertNoUserErrors(capture: CapturedInteraction, root: string, label: string): void {
  const errors = userErrors(capture, root);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function assertUpdateUserError(capture: CapturedInteraction, expectedField: string[], expectedMessage: string): void {
  const errors = userErrors(capture, 'metafieldDefinitionUpdate');
  const matched = errors.some(
    (error) =>
      error.code === 'TOO_LONG' &&
      JSON.stringify(error.field) === JSON.stringify(expectedField) &&
      error.message === expectedMessage,
  );
  if (!matched) {
    throw new Error(`missing expected update userError: ${JSON.stringify(errors)}`);
  }
  const updatedDefinition = readPath(capture.response, ['data', 'metafieldDefinitionUpdate', 'updatedDefinition']);
  if (updatedDefinition !== null) {
    throw new Error(`invalid update returned a definition: ${JSON.stringify(updatedDefinition)}`);
  }
}

function assertDefinition(capture: CapturedInteraction, expectedName: string, expectedDescription: string): void {
  const definition = readObject(readPath(capture.response, ['data', 'metafieldDefinition']));
  if (definition?.['name'] !== expectedName || definition?.['description'] !== expectedDescription) {
    throw new Error(
      `definition readback mismatch: ${JSON.stringify({
        actual: definition,
        expectedName,
        expectedDescription,
      })}`,
    );
  }
}

async function captureDocument(
  label: string,
  documentPath: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    request: { documentPath, variables },
    status: result.status,
    response: result.payload,
  };
}

async function captureQuery(
  label: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<CapturedInteraction> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function requireCapture(value: CapturedInteraction | null, label: string): CapturedInteraction {
  if (!value) {
    throw new Error(`${label} was not captured`);
  }
  return value;
}

const suffix = Date.now().toString(36);
const namespace = `length_update_${suffix}`;
const key = 'season';
const originalName = 'Season';
const originalDescription = 'Original description';
const tooLongName = 'N'.repeat(256);
const tooLongDescription = 'D'.repeat(256);
const boundaryName = 'N'.repeat(255);
const boundaryDescription = 'D'.repeat(255);
const identifier = { ownerType: 'PRODUCT', namespace, key };

let definitionId: string | null = null;
let create: CapturedInteraction | null = null;
let nameTooLongUpdate: CapturedInteraction | null = null;
let readAfterNameReject: CapturedInteraction | null = null;
let descriptionTooLongUpdate: CapturedInteraction | null = null;
let readAfterDescriptionReject: CapturedInteraction | null = null;
let boundaryUpdate: CapturedInteraction | null = null;
let readAfterBoundary: CapturedInteraction | null = null;
const cleanup: CapturedInteraction[] = [];

try {
  create = await captureDocument('metafieldDefinitionCreate setup', createDocumentPath, createDocument, {
    definition: {
      namespace,
      key,
      ownerType: 'PRODUCT',
      name: originalName,
      description: originalDescription,
      type: 'single_line_text_field',
    },
  });
  assertNoUserErrors(create, 'metafieldDefinitionCreate', 'metafieldDefinitionCreate setup');
  definitionId = requireString(
    readPath(create.response, ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id']),
    'definition id',
  );

  nameTooLongUpdate = await captureDocument(
    'metafieldDefinitionUpdate name too long',
    updateDocumentPath,
    updateDocument,
    {
      definition: {
        ...identifier,
        name: tooLongName,
      },
    },
  );
  assertUpdateUserError(nameTooLongUpdate, ['definition', 'name'], 'Name is too long (maximum is 255 characters)');

  readAfterNameReject = await captureDocument('read after name too long reject', readDocumentPath, readDocument, {
    identifier,
  });
  assertDefinition(readAfterNameReject, originalName, originalDescription);

  descriptionTooLongUpdate = await captureDocument(
    'metafieldDefinitionUpdate description too long',
    updateDocumentPath,
    updateDocument,
    {
      definition: {
        ...identifier,
        description: tooLongDescription,
      },
    },
  );
  assertUpdateUserError(
    descriptionTooLongUpdate,
    ['definition', 'description'],
    'Description is too long (maximum is 255 characters)',
  );

  readAfterDescriptionReject = await captureDocument(
    'read after description too long reject',
    readDocumentPath,
    readDocument,
    { identifier },
  );
  assertDefinition(readAfterDescriptionReject, originalName, originalDescription);

  boundaryUpdate = await captureDocument(
    'metafieldDefinitionUpdate name and description boundary success',
    updateDocumentPath,
    updateDocument,
    {
      definition: {
        ...identifier,
        name: boundaryName,
        description: boundaryDescription,
      },
    },
  );
  assertNoUserErrors(boundaryUpdate, 'metafieldDefinitionUpdate', 'boundary update');
  const boundaryPayload = readObject(
    readPath(boundaryUpdate.response, ['data', 'metafieldDefinitionUpdate', 'updatedDefinition']),
  );
  if (boundaryPayload?.['name'] !== boundaryName || boundaryPayload?.['description'] !== boundaryDescription) {
    throw new Error(`boundary update did not apply: ${JSON.stringify(boundaryPayload)}`);
  }

  readAfterBoundary = await captureDocument('read after boundary update', readDocumentPath, readDocument, {
    identifier,
  });
  assertDefinition(readAfterBoundary, boundaryName, boundaryDescription);
} finally {
  if (definitionId) {
    cleanup.push(
      await captureQuery('cleanup metafieldDefinitionDelete', deleteDefinitionMutation, { id: definitionId }).catch(
        (error: unknown) => ({
          request: { query: deleteDefinitionMutation, variables: { id: definitionId } },
          status: 0,
          response: { error: String(error) },
        }),
      ),
    );
  }
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  namespace,
  key,
  lengths: {
    tooLongName: tooLongName.length,
    tooLongDescription: tooLongDescription.length,
    boundaryName: boundaryName.length,
    boundaryDescription: boundaryDescription.length,
  },
  create: requireCapture(create, 'create'),
  nameTooLongUpdate: requireCapture(nameTooLongUpdate, 'nameTooLongUpdate'),
  readAfterNameReject: requireCapture(readAfterNameReject, 'readAfterNameReject'),
  descriptionTooLongUpdate: requireCapture(descriptionTooLongUpdate, 'descriptionTooLongUpdate'),
  readAfterDescriptionReject: requireCapture(readAfterDescriptionReject, 'readAfterDescriptionReject'),
  boundaryUpdate: requireCapture(boundaryUpdate, 'boundaryUpdate'),
  readAfterBoundary: requireCapture(readAfterBoundary, 'readAfterBoundary'),
  cleanup,
  upstreamCalls: [],
};

const spec = {
  scenarioId: 'metafield-definition-update-name-description-length',
  operationNames: ['metafieldDefinitionCreate', 'metafieldDefinitionUpdate', 'metafieldDefinition'],
  scenarioStatus: 'captured',
  assertionKinds: ['payload-shape', 'user-errors-parity', 'read-after-write', 'runtime-staging'],
  liveCaptureFiles: [fixturePath],
  runtimeTestFiles: ['tests/graphql_routes/metafield_definitions.rs'],
  proxyRequest: {
    documentPath: createDocumentPath,
    variablesCapturePath: '$.create.request.variables',
    apiVersion,
  },
  comparisonMode: 'captured-vs-proxy-request',
  comparison: {
    mode: 'strict-json',
    expectedDifferences: [],
    targets: [
      {
        name: 'create-definition-setup',
        capturePath: '$.create.response.data.metafieldDefinitionCreate',
        proxyPath: '$.data.metafieldDefinitionCreate',
        expectedDifferences: [
          {
            path: '$.createdDefinition.id',
            matcher: 'shopify-gid:MetafieldDefinition',
            reason:
              'The proxy stages a deterministic synthetic definition ID while Shopify returned the live-store definition ID.',
          },
        ],
      },
      {
        name: 'too-long-name-update-user-error',
        capturePath: '$.nameTooLongUpdate.response.data.metafieldDefinitionUpdate',
        proxyPath: '$.data.metafieldDefinitionUpdate',
        proxyRequest: {
          documentPath: updateDocumentPath,
          variablesCapturePath: '$.nameTooLongUpdate.request.variables',
          apiVersion,
        },
      },
      {
        name: 'read-after-name-too-long-reject',
        capturePath: '$.readAfterNameReject.response.data.metafieldDefinition',
        proxyPath: '$.data.metafieldDefinition',
        proxyRequest: {
          documentPath: readDocumentPath,
          variablesCapturePath: '$.readAfterNameReject.request.variables',
          apiVersion,
        },
      },
      {
        name: 'too-long-description-update-user-error',
        capturePath: '$.descriptionTooLongUpdate.response.data.metafieldDefinitionUpdate',
        proxyPath: '$.data.metafieldDefinitionUpdate',
        proxyRequest: {
          documentPath: updateDocumentPath,
          variablesCapturePath: '$.descriptionTooLongUpdate.request.variables',
          apiVersion,
        },
      },
      {
        name: 'read-after-description-too-long-reject',
        capturePath: '$.readAfterDescriptionReject.response.data.metafieldDefinition',
        proxyPath: '$.data.metafieldDefinition',
        proxyRequest: {
          documentPath: readDocumentPath,
          variablesCapturePath: '$.readAfterDescriptionReject.request.variables',
          apiVersion,
        },
      },
      {
        name: 'boundary-update-accepted',
        capturePath: '$.boundaryUpdate.response.data.metafieldDefinitionUpdate',
        proxyPath: '$.data.metafieldDefinitionUpdate',
        proxyRequest: {
          documentPath: updateDocumentPath,
          variablesCapturePath: '$.boundaryUpdate.request.variables',
          apiVersion,
        },
      },
      {
        name: 'read-after-boundary-update',
        capturePath: '$.readAfterBoundary.response.data.metafieldDefinition',
        proxyPath: '$.data.metafieldDefinition',
        proxyRequest: {
          documentPath: readDocumentPath,
          variablesCapturePath: '$.readAfterBoundary.request.variables',
          apiVersion,
        },
      },
    ],
  },
  notes:
    'Live Shopify Admin GraphQL evidence for metafieldDefinitionUpdate name/description length validation. Shopify rejects 256-character name and description inputs with TOO_LONG userErrors, returns updatedDefinition null, and readback keeps the original definition values. The same scenario records 255-character name and description update success and downstream readback.',
};

await mkdir(requestDir, { recursive: true });
await mkdir(specDir, { recursive: true });
await mkdir(fixtureDir, { recursive: true });
await writeFile(createDocumentPath, createDocument, 'utf8');
await writeFile(updateDocumentPath, updateDocument, 'utf8');
await writeFile(readDocumentPath, readDocument, 'utf8');
await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath: fixturePath,
      specPath,
      requestPaths: [createDocumentPath, updateDocumentPath, readDocumentPath],
      storeDomain,
      apiVersion,
      cleanup: cleanup.map((entry) => ({ status: entry.status })),
    },
    null,
    2,
  ),
);
