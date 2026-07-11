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

type SeedState = {
  type: string;
  firstHandle: string;
  secondHandle: string;
  missingId: string;
  definitionId?: string;
  firstId?: string;
  secondId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-bulk-delete-ids-lifecycle.json');
const runId = Date.now().toString();
const seed: SeedState = {
  type: `codex_bulk_delete_ids_${runId}`,
  firstHandle: `codex-bulk-delete-first-${runId}`,
  secondHandle: `codex-bulk-delete-second-${runId}`,
  missingId: `gid://shopify/Metaobject/999${runId}`,
};

const bulkDeleteMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-ids-delete.graphql',
  'utf8',
);
const bulkReadQuery = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-ids-read.graphql',
  'utf8',
);

const hydrateByIdsQuery = `#graphql
  query MetaobjectBulkDeleteHydrateByIds($ids: [ID!]!) {
    nodes(ids: $ids) {
      __typename
      ... on Metaobject {
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
        titleField: field(key: "title") {
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
  }
`;

const definitionFields = `#graphql
  fragment MetaobjectBulkDeleteIdsDefinitionFields on MetaobjectDefinition {
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
`;

const entryFields = `#graphql
  fragment MetaobjectBulkDeleteIdsEntryFields on Metaobject {
    id
    handle
    type
    displayName
    updatedAt
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
`;

const definitionCreateMutation = `#graphql
  ${definitionFields}
  mutation MetaobjectBulkDeleteIdsDefinitionCreate($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        ...MetaobjectBulkDeleteIdsDefinitionFields
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
  ${entryFields}
  mutation MetaobjectBulkDeleteIdsEntryCreate($metaobject: MetaobjectCreateInput!) {
    metaobjectCreate(metaobject: $metaobject) {
      metaobject {
        ...MetaobjectBulkDeleteIdsEntryFields
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
  mutation MetaobjectBulkDeleteIdsEntryCleanup($id: ID!) {
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
  mutation MetaobjectBulkDeleteIdsDefinitionCleanup($id: ID!) {
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

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
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
    request: {
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
  };
}

function upstreamCallFromCapture(capture: Capture): {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: { status: number; body: unknown };
} {
  return {
    operationName: capture.name,
    variables: capture.request.variables,
    query: capture.request.query,
    response: {
      status: capture.status,
      body: capture.response,
    },
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

async function runSuccessMutation(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  userErrorPath: string[],
): Promise<Capture> {
  const capture = await captureGraphql(name, query, variables);
  assertNoUserErrors(capture.response, userErrorPath, name);
  return capture;
}

function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function metaobjectReadsAreDeleted(capture: Capture): boolean {
  return (
    readPath(capture.response, ['data', 'first']) === null &&
    readPath(capture.response, ['data', 'missing']) === null &&
    readPath(capture.response, ['data', 'second']) === null
  );
}

async function captureSettledDeletedRead(variables: Record<string, unknown>): Promise<Capture> {
  const captures: Capture[] = [];
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const capture = await captureGraphql('downstream-after-bulk-delete-settled-read', bulkReadQuery, variables);
    captures.push(capture);
    if (metaobjectReadsAreDeleted(capture)) {
      return capture;
    }
    await sleep(1000);
  }

  throw new Error(
    `Timed out waiting for explicit-ID bulk delete job to hide all selected rows: ${JSON.stringify(captures.at(-1), null, 2)}`,
  );
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobject-bulk-delete-ids-lifecycle-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command:
          'SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm tsx scripts/capture-metaobject-bulk-delete-ids-conformance.ts',
        blocker: {
          stage,
          message,
        },
        seed,
        partialCaptures: captures,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
}

async function captureCleanup(cleanup: Capture[]): Promise<void> {
  for (const entryId of [seed.firstId, seed.secondId]) {
    if (!entryId) {
      continue;
    }

    cleanup.push(await captureGraphql('cleanup-metaobject-delete', entryDeleteMutation, { id: entryId }));
  }

  if (seed.definitionId) {
    cleanup.push(
      await captureGraphql('cleanup-metaobject-definition-delete', definitionDeleteMutation, { id: seed.definitionId }),
    );
  }
}

const setupCaptures: Capture[] = [];
const seededReads: Capture[] = [];
const upstreamCalls: ReturnType<typeof upstreamCallFromCapture>[] = [];
const bulkDeleteCaptures: Capture[] = [];
const downstreamReads: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let fatalError: unknown = null;

try {
  const definitionCreate = await runSuccessMutation(
    'setup-definition-create',
    definitionCreateMutation,
    {
      definition: {
        type: seed.type,
        name: `Codex Bulk IDs ${runId}`,
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
    },
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
  );
  setupCaptures.push(definitionCreate);
  seed.definitionId = extractId(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition create',
  );

  const firstCreate = await runSuccessMutation(
    'setup-first-entry-create',
    entryCreateMutation,
    {
      metaobject: {
        type: seed.type,
        handle: seed.firstHandle,
        fields: [
          { key: 'title', value: 'Bulk delete first' },
          { key: 'body', value: 'First entry selected by ID bulk delete.' },
        ],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  setupCaptures.push(firstCreate);
  seed.firstId = extractId(
    firstCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'first entry create',
  );

  const secondCreate = await runSuccessMutation(
    'setup-second-entry-create',
    entryCreateMutation,
    {
      metaobject: {
        type: seed.type,
        handle: seed.secondHandle,
        fields: [
          { key: 'title', value: 'Bulk delete second' },
          { key: 'body', value: 'Second entry selected by ID bulk delete.' },
        ],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  setupCaptures.push(secondCreate);
  seed.secondId = extractId(
    secondCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'second entry create',
  );

  const readVariables = {
    firstId: seed.firstId,
    missingId: seed.missingId,
    secondId: seed.secondId,
  };
  const deleteVariables = {
    ids: [seed.firstId, seed.missingId, seed.secondId],
  };

  seededReads.push(await captureGraphql('seeded-before-bulk-delete-read', bulkReadQuery, readVariables));
  const hydrateByIds = await captureGraphql('MetaobjectBulkDeleteHydrateByIds', hydrateByIdsQuery, deleteVariables);
  upstreamCalls.push(upstreamCallFromCapture(hydrateByIds));
  bulkDeleteCaptures.push(
    await runSuccessMutation('bulk-delete-by-ids', bulkDeleteMutation, deleteVariables, [
      'data',
      'metaobjectBulkDelete',
      'userErrors',
    ]),
  );
  downstreamReads.push(await captureSettledDeletedRead(readVariables));
} catch (error) {
  fatalError = error;
  await writeBlocker('capture', error, [...setupCaptures, ...seededReads, ...bulkDeleteCaptures, ...downstreamReads]);
}

try {
  await captureCleanup(cleanupCaptures);
} catch (error) {
  await writeBlocker('cleanup', error, [
    ...setupCaptures,
    ...seededReads,
    ...bulkDeleteCaptures,
    ...downstreamReads,
    ...cleanupCaptures,
  ]);
  fatalError ??= error;
}

if (fatalError) {
  throw fatalError;
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
      setup: setupCaptures,
      seededReads,
      bulkDelete: bulkDeleteCaptures,
      downstreamReads,
      cleanup: cleanupCaptures,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote metaobject bulk-delete IDs conformance fixture to ${outputPath}`);
