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

type RecordedCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

type SeedState = {
  type: string;
  handle: string;
  definitionId?: string;
  rowId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-bulk-delete-edge-cases.json');
const runId = Date.now().toString();
const seed: SeedState = {
  type: `codex_har_680_bulk_delete_empty_${runId}`,
  handle: `codex-har-680-empty-${runId}`,
};

const emptyIdsMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-edge-empty-ids.graphql',
  'utf8',
);
const unknownTypeMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-edge-unknown-type.graphql',
  'utf8',
);
const knownEmptyTypeMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-edge-known-empty-type.graphql',
  'utf8',
);
const bothTypeAndIdsMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-edge-both-type-and-ids.graphql',
  'utf8',
);

const definitionCreateMutation = `#graphql
  mutation MetaobjectBulkDeleteEdgeDefinitionCreate($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        id
        type
        name
        displayNameKey
        fieldDefinitions {
          key
          name
          required
          type {
            name
            category
          }
        }
        metaobjectsCount
      }
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const entryCreateMutation = `#graphql
  mutation MetaobjectBulkDeleteEdgeEntryCreate($metaobject: MetaobjectCreateInput!) {
    metaobjectCreate(metaobject: $metaobject) {
      metaobject {
        id
        handle
        type
      }
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const entryDeleteMutation = `#graphql
  mutation MetaobjectBulkDeleteEdgeEntryDelete($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const definitionDeleteMutation = `#graphql
  mutation MetaobjectBulkDeleteEdgeDefinitionDelete($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const bulkDeleteHydrateByTypeQuery = `#graphql
  query MetaobjectBulkDeleteHydrateByType($type: String!) {
    catalog: metaobjects(type: $type, first: 250) {
      nodes {
        id
        handle
        type
        displayName
        createdAt
        updatedAt
        capabilities {
          publishable {
            status
          }
          onlineStore {
            templateSuffix
          }
        }
        fields {
          key
          type
          value
          jsonValue
          definition {
            key
            name
            required
            type {
              name
              category
            }
          }
        }
      }
    }
    definition: metaobjectDefinitionByType(type: $type) {
      id
      type
      name
      description
      displayNameKey
      access {
        admin
        storefront
      }
      capabilities {
        publishable {
          enabled
        }
        translatable {
          enabled
        }
        renderable {
          enabled
        }
        onlineStore {
          enabled
        }
      }
      fieldDefinitions {
        key
        name
        description
        required
        type {
          name
          category
        }
        validations {
          name
          value
        }
      }
      hasThumbnailField
      metaobjectsCount
      standardTemplate {
        type
        name
      }
      createdAt
      updatedAt
    }
  }
`;

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (object === null) {
      return undefined;
    }
    current = object[part];
  }

  return current;
}

function readUserErrors(payload: unknown, pathParts: string[]): unknown[] {
  const value = readPath(payload, pathParts);
  return Array.isArray(value) ? value : [];
}

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} failed with HTTP ${result.status}: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  assertHttpOk(result, label);
  if (result.payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(result.payload.errors, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function extractId(payload: unknown, pathParts: string[], label: string): string {
  const id = readPath(payload, pathParts);
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return an id: ${JSON.stringify(payload, null, 2)}`);
  }

  return id;
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
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

async function captureGraphqlAllowErrors(
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, name);
  return captureFromResult(name, query, variables, result);
}

async function captureUpstreamCall(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<RecordedCall> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, operationName);
  return {
    operationName,
    variables,
    query,
    response: {
      status: result.status,
      body: result.payload,
    },
  };
}

async function captureCleanup(cleanup: Capture[]): Promise<void> {
  if (seed.rowId) {
    cleanup.push(await captureGraphqlAllowErrors('cleanup-metaobject-delete', entryDeleteMutation, { id: seed.rowId }));
  }

  if (seed.definitionId) {
    cleanup.push(
      await captureGraphqlAllowErrors('cleanup-metaobject-definition-delete', definitionDeleteMutation, {
        id: seed.definitionId,
      }),
    );
  }
}

const setup: Capture[] = [];
const branches: Record<string, Capture | undefined> = {};
const cleanup: Capture[] = [];
const upstreamCalls: RecordedCall[] = [];
let fatalError: unknown = null;

try {
  branches['emptyIds'] = await captureGraphql('bulk-delete-empty-ids', emptyIdsMutation, {});
  assertNoUserErrors(
    branches['emptyIds'].response,
    ['data', 'metaobjectBulkDelete', 'userErrors'],
    'bulk-delete-empty-ids',
  );

  const unknownType = `codex_har_680_missing_${runId}`;
  branches['unknownType'] = await captureGraphql('bulk-delete-unknown-type', unknownTypeMutation, {
    type: unknownType,
  });

  const definitionCreate = await captureGraphql('setup-definition-create', definitionCreateMutation, {
    definition: {
      type: seed.type,
      name: `Codex HAR-680 Empty ${runId}`,
      displayNameKey: 'title',
      fieldDefinitions: [
        {
          key: 'title',
          name: 'Title',
          type: 'single_line_text_field',
          required: true,
        },
      ],
    },
  });
  setup.push(definitionCreate);
  seed.definitionId = extractId(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition create',
  );

  const entryCreate = await captureGraphql('setup-entry-create', entryCreateMutation, {
    metaobject: {
      type: seed.type,
      handle: seed.handle,
      fields: [{ key: 'title', value: 'Deleted before type bulk delete' }],
    },
  });
  setup.push(entryCreate);
  seed.rowId = extractId(entryCreate.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'entry create');

  const entryDelete = await captureGraphql('setup-entry-delete', entryDeleteMutation, { id: seed.rowId });
  setup.push(entryDelete);
  assertNoUserErrors(entryDelete.response, ['data', 'metaobjectDelete', 'userErrors'], 'setup-entry-delete');

  upstreamCalls.push(
    await captureUpstreamCall('MetaobjectBulkDeleteHydrateByType', bulkDeleteHydrateByTypeQuery, { type: seed.type }),
  );

  branches['knownEmptyType'] = await captureGraphql('bulk-delete-known-empty-type', knownEmptyTypeMutation, {
    type: seed.type,
  });
  assertNoUserErrors(
    branches['knownEmptyType'].response,
    ['data', 'metaobjectBulkDelete', 'userErrors'],
    'bulk-delete-known-empty-type',
  );

  branches['bothTypeAndIds'] = await captureGraphqlAllowErrors(
    'bulk-delete-both-type-and-ids',
    bothTypeAndIdsMutation,
    {},
  );
} catch (error) {
  fatalError = error;
}

try {
  await captureCleanup(cleanup);
} catch (error) {
  fatalError ??= error;
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      seed,
      setup,
      branches,
      cleanup,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote metaobject bulk-delete edge-case conformance fixture to ${outputPath}`);

if (fatalError) {
  throw fatalError;
}
