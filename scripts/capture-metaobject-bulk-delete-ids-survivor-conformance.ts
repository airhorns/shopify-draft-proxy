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
  survivorHandle: string;
  definitionId?: string;
  firstId?: string;
  secondId?: string;
  survivorId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-bulk-delete-ids-survivor-lifecycle.json');
const runId = Date.now().toString();
const seed: SeedState = {
  type: `codex_bulk_delete_ids_${runId}`,
  firstHandle: `codex-bulk-delete-ids-first-${runId}`,
  secondHandle: `codex-bulk-delete-ids-second-${runId}`,
  survivorHandle: `codex-bulk-delete-ids-survivor-${runId}`,
};

const definitionCreateMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-ids-definition-create.graphql',
  'utf8',
);
const entryCreateMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-ids-entry-create.graphql',
  'utf8',
);
const bulkDeleteMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-ids-delete.graphql',
  'utf8',
);
const downstreamReadQuery = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-ids-survivor-read.graphql',
  'utf8',
);

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
  const blockerPath = path.join(outputDir, `metaobject-bulk-delete-ids-survivor-lifecycle-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command:
          'SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm tsx scripts/capture-metaobject-bulk-delete-ids-survivor-conformance.ts',
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

function entryVariables(handle: string, title: string, body: string): Record<string, unknown> {
  return {
    metaobject: {
      type: seed.type,
      handle,
      fields: [
        { key: 'title', value: title },
        { key: 'body', value: body },
      ],
    },
  };
}

async function captureCleanup(cleanup: Capture[]): Promise<void> {
  for (const entryId of [seed.firstId, seed.secondId, seed.survivorId]) {
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

let definitionCreate: Capture | null = null;
let firstEntryCreate: Capture | null = null;
let secondEntryCreate: Capture | null = null;
let survivorEntryCreate: Capture | null = null;
let bulkDelete: Capture | null = null;
let downstreamRead: Capture | null = null;
const cleanupCaptures: Capture[] = [];
let fatalError: unknown = null;

try {
  definitionCreate = await runSuccessMutation(
    'setup-definition-create',
    definitionCreateMutation,
    {
      definition: {
        type: seed.type,
        name: `Codex Bulk Delete IDs ${runId}`,
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
  seed.definitionId = extractId(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition create',
  );

  firstEntryCreate = await runSuccessMutation(
    'setup-first-entry-create',
    entryCreateMutation,
    entryVariables(seed.firstHandle, 'Bulk delete IDs first', 'First entry selected by the ids bulk delete branch.'),
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.firstId = extractId(
    firstEntryCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'first entry create',
  );

  secondEntryCreate = await runSuccessMutation(
    'setup-second-entry-create',
    entryCreateMutation,
    entryVariables(
      seed.secondHandle,
      'Bulk delete IDs second',
      'Second entry left unselected by the ids bulk delete branch.',
    ),
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.secondId = extractId(
    secondEntryCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'second entry create',
  );

  survivorEntryCreate = await runSuccessMutation(
    'setup-survivor-entry-create',
    entryCreateMutation,
    entryVariables(
      seed.survivorHandle,
      'Bulk delete IDs survivor',
      'Surviving entry proves id-scoped bulk delete does not delete the whole type.',
    ),
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.survivorId = extractId(
    survivorEntryCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'survivor entry create',
  );

  bulkDelete = await runSuccessMutation('bulk-delete-by-ids', bulkDeleteMutation, { ids: [seed.firstId] }, [
    'data',
    'metaobjectBulkDelete',
    'userErrors',
  ]);

  downstreamRead = await captureGraphql('downstream-after-bulk-delete-ids-read', downstreamReadQuery, {
    type: seed.type,
    firstId: seed.firstId,
    secondId: seed.secondId,
    survivorId: seed.survivorId,
  });
} catch (error) {
  fatalError = error;
  await writeBlocker(
    'capture',
    error,
    [definitionCreate, firstEntryCreate, secondEntryCreate, survivorEntryCreate, bulkDelete, downstreamRead].filter(
      (capture): capture is Capture => capture !== null,
    ),
  );
}

try {
  await captureCleanup(cleanupCaptures);
} catch (error) {
  await writeBlocker(
    'cleanup',
    error,
    [
      definitionCreate,
      firstEntryCreate,
      secondEntryCreate,
      survivorEntryCreate,
      bulkDelete,
      downstreamRead,
      ...cleanupCaptures,
    ].filter((capture): capture is Capture => capture !== null),
  );
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
      setup: {
        definitionCreate,
        firstEntryCreate,
        secondEntryCreate,
        survivorEntryCreate,
      },
      bulkDelete,
      downstreamRead,
      cleanup: cleanupCaptures,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote metaobject bulk-delete ids conformance fixture to ${outputPath}`);
