/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { setTimeout as sleep } from 'node:timers/promises';
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
  name: string;
  oldHandle: string;
  newFirstHandle: string;
  newSecondHandle: string;
  oldDefinitionId?: string;
  oldEntryId?: string;
  newDefinitionId?: string;
  newFirstId?: string;
  newSecondId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-definition-recreate-lifecycle.json');
const runId = Date.now().toString();
const seed: SeedState = {
  type: `codex_recreate_definition_${runId}`,
  name: `Codex Recreate Definition ${runId}`,
  oldHandle: `codex-recreate-old-${runId}`,
  newFirstHandle: `codex-recreate-new-first-${runId}`,
  newSecondHandle: `codex-recreate-new-second-${runId}`,
};

const requestPaths = {
  definitionCreate: 'config/parity-requests/metaobjects/metaobject-definition-lifecycle-create.graphql',
  definitionDelete: 'config/parity-requests/metaobjects/metaobject-definition-lifecycle-delete.graphql',
  entryCreate: 'config/parity-requests/metaobjects/metaobject-definition-recreate-entry-create.graphql',
  read: 'config/parity-requests/metaobjects/metaobject-definition-recreate-read.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const hydrateDefinitionByTypeQuery = `#graphql
  query MetaobjectDefinitionHydrateByType($type: String!) {
    metaobjectDefinitionByType(type: $type) {
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
  const blockerPath = path.join(outputDir, `metaobject-definition-recreate-lifecycle-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command:
          'SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm conformance:capture -- --run metaobject-definition-recreate-lifecycle',
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

function oldDefinitionInput(): Record<string, unknown> {
  return {
    type: seed.type,
    name: seed.name,
    description: 'Temporary definition before delete/recreate lifecycle capture.',
    displayNameKey: 'old_title',
    fieldDefinitions: [
      {
        key: 'old_title',
        name: 'Old Title',
        description: 'Display title on the first definition generation.',
        type: 'single_line_text_field',
        required: true,
      },
      {
        key: 'old_body',
        name: 'Old Body',
        description: 'Body text on the first definition generation.',
        type: 'multi_line_text_field',
        required: false,
      },
    ],
  };
}

function newDefinitionInput(): Record<string, unknown> {
  return {
    type: seed.type,
    name: seed.name,
    description: 'Temporary definition after delete/recreate lifecycle capture.',
    displayNameKey: 'new_title',
    fieldDefinitions: [
      {
        key: 'new_title',
        name: 'New Title',
        description: 'Display title on the recreated definition generation.',
        type: 'single_line_text_field',
        required: true,
      },
      {
        key: 'new_summary',
        name: 'New Summary',
        description: 'Summary text only present on the recreated definition generation.',
        type: 'multi_line_text_field',
        required: false,
      },
    ],
  };
}

function finalReadVariables(): Record<string, unknown> {
  if (!seed.oldDefinitionId || !seed.newDefinitionId || !seed.oldEntryId || !seed.newFirstId || !seed.newSecondId) {
    throw new Error(`Cannot build final read variables before all ids exist: ${JSON.stringify(seed, null, 2)}`);
  }

  return {
    oldDefinitionId: seed.oldDefinitionId,
    newDefinitionId: seed.newDefinitionId,
    type: seed.type,
    oldEntryId: seed.oldEntryId,
    oldHandle: {
      type: seed.type,
      handle: seed.oldHandle,
    },
    newFirstId: seed.newFirstId,
    newSecondId: seed.newSecondId,
    newFirstHandle: {
      type: seed.type,
      handle: seed.newFirstHandle,
    },
    newSecondHandle: {
      type: seed.type,
      handle: seed.newSecondHandle,
    },
  };
}

function hydrateUpstreamCall(hydrateCapture: Capture): unknown {
  return {
    operationName: 'MetaobjectDefinitionHydrateByType',
    variables: {
      type: seed.type,
    },
    query: 'sha:hand-synthesized-from-capture',
    response: {
      status: hydrateCapture.status,
      body: hydrateCapture.response,
    },
  };
}

async function captureCleanup(cleanup: Capture[]): Promise<void> {
  for (const entryId of [seed.newFirstId, seed.newSecondId, seed.oldEntryId]) {
    if (!entryId) {
      continue;
    }

    cleanup.push(
      await captureGraphql(
        'cleanup-metaobject-delete',
        'mutation DeleteMetaobject($id: ID!) { metaobjectDelete(id: $id) { deletedId userErrors { field message code elementKey elementIndex } } }',
        { id: entryId },
      ),
    );
  }

  for (const definitionId of [seed.newDefinitionId, seed.oldDefinitionId]) {
    if (!definitionId) {
      continue;
    }

    cleanup.push(
      await captureGraphql('cleanup-metaobject-definition-delete', queries.definitionDelete, { id: definitionId }),
    );
  }
}

const hydrateCaptures: Capture[] = [];
const setupCaptures: Capture[] = [];
const deleteCaptures: Capture[] = [];
const recreateCaptures: Capture[] = [];
const postRecreateEntryCaptures: Capture[] = [];
const finalReadCaptures: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let fatalError: unknown = null;

try {
  hydrateCaptures.push(
    await captureGraphql('hydrate-definition-by-type-before-create', hydrateDefinitionByTypeQuery, { type: seed.type }),
  );

  const oldDefinitionCreate = await runSuccessMutation(
    'setup-old-definition-create',
    queries.definitionCreate,
    { definition: oldDefinitionInput() },
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
  );
  seed.oldDefinitionId = extractId(
    oldDefinitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'old metaobjectDefinitionCreate',
  );
  setupCaptures.push(oldDefinitionCreate);

  const oldEntryCreate = await runSuccessMutation(
    'setup-old-entry-create',
    queries.entryCreate,
    {
      metaobject: {
        type: seed.type,
        handle: seed.oldHandle,
        fields: [
          { key: 'old_title', value: `Old title ${runId}` },
          { key: 'old_body', value: `Old body ${runId}` },
        ],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.oldEntryId = extractId(
    oldEntryCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'old metaobjectCreate',
  );
  setupCaptures.push(oldEntryCreate);

  deleteCaptures.push(
    await runSuccessMutation('delete-old-definition', queries.definitionDelete, { id: seed.oldDefinitionId }, [
      'data',
      'metaobjectDefinitionDelete',
      'userErrors',
    ]),
  );

  await sleep(5_000);

  const newDefinitionCreate = await runSuccessMutation(
    'recreate-definition-same-type-and-name',
    queries.definitionCreate,
    { definition: newDefinitionInput() },
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
  );
  seed.newDefinitionId = extractId(
    newDefinitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'recreated metaobjectDefinitionCreate',
  );
  recreateCaptures.push(newDefinitionCreate);

  const newFirstCreate = await runSuccessMutation(
    'post-recreate-first-entry-create',
    queries.entryCreate,
    {
      metaobject: {
        type: seed.type,
        handle: seed.newFirstHandle,
        fields: [
          { key: 'new_title', value: `New first title ${runId}` },
          { key: 'new_summary', value: `New first summary ${runId}` },
        ],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.newFirstId = extractId(
    newFirstCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'post-recreate first metaobjectCreate',
  );
  postRecreateEntryCaptures.push(newFirstCreate);

  const newSecondCreate = await runSuccessMutation(
    'post-recreate-second-entry-create',
    queries.entryCreate,
    {
      metaobject: {
        type: seed.type,
        handle: seed.newSecondHandle,
        fields: [
          { key: 'new_title', value: `New second title ${runId}` },
          { key: 'new_summary', value: `New second summary ${runId}` },
        ],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.newSecondId = extractId(
    newSecondCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'post-recreate second metaobjectCreate',
  );
  postRecreateEntryCaptures.push(newSecondCreate);

  await sleep(15_000);
  finalReadCaptures.push(await captureGraphql('final-post-recreate-read', queries.read, finalReadVariables()));
} catch (error) {
  fatalError = error;
  await writeBlocker('capture', error, [
    ...hydrateCaptures,
    ...setupCaptures,
    ...deleteCaptures,
    ...recreateCaptures,
    ...postRecreateEntryCaptures,
    ...finalReadCaptures,
  ]);
}

try {
  await captureCleanup(cleanupCaptures);
} catch (error) {
  await writeBlocker('cleanup', error, [
    ...hydrateCaptures,
    ...setupCaptures,
    ...deleteCaptures,
    ...recreateCaptures,
    ...postRecreateEntryCaptures,
    ...finalReadCaptures,
    ...cleanupCaptures,
  ]);
  fatalError ??= error;
}

if (fatalError) {
  throw fatalError;
}

const [hydrateCapture] = hydrateCaptures;
if (!hydrateCapture) {
  throw new Error('Expected hydrate capture before writing fixture.');
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
      hydrate: hydrateCaptures,
      setup: setupCaptures,
      delete: deleteCaptures,
      recreate: recreateCaptures,
      postRecreateEntries: postRecreateEntryCaptures,
      finalReads: finalReadCaptures,
      cleanup: cleanupCaptures,
      upstreamCalls: [hydrateUpstreamCall(hydrateCapture)],
    },
    null,
    2,
  )}\n`,
);

console.log(`Wrote metaobject definition recreate lifecycle conformance fixture to ${outputPath}`);
