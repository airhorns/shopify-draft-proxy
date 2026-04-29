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
  definitionId?: string;
  firstId?: string;
  secondId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-bulk-delete-type-lifecycle.json');
const runId = Date.now().toString();
const seed: SeedState = {
  type: `codex_har_450_bulk_delete_${runId}`,
  firstHandle: `codex-har-450-bulk-first-${runId}`,
  secondHandle: `codex-har-450-bulk-second-${runId}`,
};

const bulkDeleteMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-type-delete.graphql',
  'utf8',
);
const bulkReadQuery = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-type-read.graphql',
  'utf8',
);

const definitionFields = `#graphql
  fragment MetaobjectBulkDeleteDefinitionFields on MetaobjectDefinition {
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
  fragment MetaobjectBulkDeleteEntryFields on Metaobject {
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
  mutation MetaobjectBulkDeleteDefinitionCreate($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        ...MetaobjectBulkDeleteDefinitionFields
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
  mutation MetaobjectBulkDeleteEntryCreate($metaobject: MetaobjectCreateInput!) {
    metaobjectCreate(metaobject: $metaobject) {
      metaobject {
        ...MetaobjectBulkDeleteEntryFields
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
  mutation MetaobjectBulkDeleteEntryCleanup($id: ID!) {
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
  mutation MetaobjectBulkDeleteDefinitionCleanup($id: ID!) {
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

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobject-bulk-delete-type-lifecycle-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command:
          'SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm tsx scripts/capture-metaobject-bulk-delete-conformance.ts',
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
        name: `Codex HAR-450 Bulk ${runId}`,
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
          { key: 'body', value: 'First entry selected by type bulk delete.' },
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
          { key: 'body', value: 'Second entry selected by type bulk delete.' },
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
    type: seed.type,
    firstId: seed.firstId,
    secondId: seed.secondId,
  };

  seededReads.push(await captureGraphql('seeded-before-bulk-delete-read', bulkReadQuery, readVariables));
  bulkDeleteCaptures.push(
    await runSuccessMutation('bulk-delete-by-type', bulkDeleteMutation, { type: seed.type }, [
      'data',
      'metaobjectBulkDelete',
      'userErrors',
    ]),
  );
  downstreamReads.push(await captureGraphql('downstream-after-bulk-delete-read', bulkReadQuery, readVariables));
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
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote metaobject bulk-delete conformance fixture to ${outputPath}`);
