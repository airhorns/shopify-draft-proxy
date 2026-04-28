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
  preHandle: string;
  deletedBeforeHandle: string;
  postHandle: string;
  deletedAfterHandle: string;
  definitionId?: string;
  preEntryId?: string;
  deletedBeforeEntryId?: string;
  postEntryId?: string;
  deletedAfterEntryId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const outputPath = path.join(outputDir, 'metaobject-schema-change-lifecycle.json');
const runId = Date.now().toString();
const seed: SeedState = {
  type: `codex_har_245_${runId}`,
  preHandle: `codex-har-245-pre-${runId}`,
  deletedBeforeHandle: `codex-har-245-delete-before-${runId}`,
  postHandle: `codex-har-245-post-${runId}`,
  deletedAfterHandle: `codex-har-245-delete-after-${runId}`,
};

const requestPaths = {
  definitionCreate: 'config/parity-requests/metaobject-schema-change-definition-create.graphql',
  definitionUpdate: 'config/parity-requests/metaobject-schema-change-definition-update.graphql',
  entryCreate: 'config/parity-requests/metaobject-schema-change-entry-create.graphql',
  entryUpdate: 'config/parity-requests/metaobject-schema-change-entry-update.graphql',
  entryDelete: 'config/parity-requests/metaobject-schema-change-entry-delete.graphql',
  read: 'config/parity-requests/metaobject-schema-change-read.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

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

function assertHasUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length === 0) {
    throw new Error(`${label} did not return userErrors: ${JSON.stringify(payload, null, 2)}`);
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

async function runExpectedErrorMutation(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  userErrorPath: string[],
): Promise<Capture> {
  const capture = await captureGraphql(name, query, variables);
  assertHasUserErrors(capture.response, userErrorPath, name);
  return capture;
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobject-schema-change-lifecycle-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm conformance:capture-metaobject-schema-change',
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
  for (const entryId of [seed.preEntryId, seed.postEntryId, seed.deletedAfterEntryId, seed.deletedBeforeEntryId]) {
    if (!entryId) {
      continue;
    }

    cleanup.push(await captureGraphql('cleanup-metaobject-delete', queries.entryDelete, { id: entryId }));
  }

  if (seed.definitionId) {
    cleanup.push(
      await captureGraphql(
        'cleanup-metaobject-definition-delete',
        'mutation DeleteMetaobjectDefinition($id: ID!) { metaobjectDefinitionDelete(id: $id) { deletedId userErrors { field message code elementKey elementIndex } } }',
        {
          id: seed.definitionId,
        },
      ),
    );
  }
}

const setupCaptures: Capture[] = [];
const preSchemaReads: Capture[] = [];
const schemaChangeCaptures: Capture[] = [];
const postSchemaValidationCaptures: Capture[] = [];
const postSchemaMutationCaptures: Capture[] = [];
const finalReadCaptures: Capture[] = [];
const cleanupCaptures: Capture[] = [];

try {
  const definitionCreate = await runSuccessMutation(
    'setup-definition-create',
    queries.definitionCreate,
    {
      definition: {
        type: seed.type,
        name: `Codex HAR-245 ${runId}`,
        description: 'Temporary HAR-245 conformance definition for schema-change lifecycle capture.',
        capabilities: {
          publishable: {
            enabled: true,
          },
          translatable: {
            enabled: false,
          },
        },
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            description: 'Display title before schema changes.',
            type: 'single_line_text_field',
            required: true,
          },
          {
            key: 'body',
            name: 'Body',
            description: 'Body text before schema changes.',
            type: 'multi_line_text_field',
            required: false,
          },
          {
            key: 'legacy',
            name: 'Legacy',
            description: 'Field removed during schema-change capture.',
            type: 'single_line_text_field',
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
    'metaobjectDefinitionCreate',
  );
  setupCaptures.push(definitionCreate);

  const preEntryCreate = await runSuccessMutation(
    'setup-pre-schema-entry-create',
    queries.entryCreate,
    {
      metaobject: {
        type: seed.type,
        handle: seed.preHandle,
        capabilities: {
          publishable: {
            status: 'ACTIVE',
          },
        },
        fields: [
          { key: 'title', value: `HAR-245 pre title ${runId}` },
          { key: 'body', value: `HAR-245 pre body ${runId}` },
          { key: 'legacy', value: `HAR-245 legacy ${runId}` },
        ],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.preEntryId = extractId(
    preEntryCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'metaobjectCreate',
  );
  setupCaptures.push(preEntryCreate);

  const deletedBeforeCreate = await runSuccessMutation(
    'setup-delete-before-schema-entry-create',
    queries.entryCreate,
    {
      metaobject: {
        type: seed.type,
        handle: seed.deletedBeforeHandle,
        fields: [
          { key: 'title', value: `HAR-245 delete-before title ${runId}` },
          { key: 'body', value: `HAR-245 delete-before body ${runId}` },
        ],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.deletedBeforeEntryId = extractId(
    deletedBeforeCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'delete-before metaobjectCreate',
  );
  setupCaptures.push(deletedBeforeCreate);

  setupCaptures.push(
    await runSuccessMutation(
      'setup-delete-before-schema-entry-delete',
      queries.entryDelete,
      { id: seed.deletedBeforeEntryId },
      ['data', 'metaobjectDelete', 'userErrors'],
    ),
  );

  preSchemaReads.push(
    await captureGraphql('pre-schema-read', queries.read, {
      id: seed.preEntryId,
      handle: { type: seed.type, handle: seed.preHandle },
      deletedId: seed.deletedBeforeEntryId,
      type: seed.type,
    }),
  );

  schemaChangeCaptures.push(
    await runSuccessMutation(
      'definition-update-add-remove-reorder-display-validation-capability',
      queries.definitionUpdate,
      {
        id: seed.definitionId,
        definition: {
          name: `Codex HAR-245 Updated ${runId}`,
          description: 'Updated by HAR-245 schema-change lifecycle capture.',
          displayNameKey: 'summary',
          resetFieldOrder: true,
          capabilities: {
            publishable: { enabled: false },
            translatable: { enabled: true },
            renderable: { enabled: true },
          },
          fieldDefinitions: [
            {
              create: {
                key: 'summary',
                name: 'Summary',
                description: 'Required display field added during schema change.',
                type: 'single_line_text_field',
                required: true,
                validations: [{ name: 'max', value: '80' }],
              },
            },
            {
              update: {
                key: 'title',
                name: 'Short title',
                description: 'Title retained after schema change.',
                required: false,
              },
            },
            {
              update: {
                key: 'body',
                name: 'Body summary',
                description:
                  'Body validation changed during schema change; Shopify 2026-04 does not expose type on update input.',
                required: false,
                validations: [{ name: 'max', value: '120' }],
              },
            },
            { delete: { key: 'legacy' } },
          ],
        },
      },
      ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    ),
  );

  finalReadCaptures.push(
    await captureGraphql('post-definition-update-read-existing-entry', queries.read, {
      id: seed.preEntryId,
      handle: { type: seed.type, handle: seed.preHandle },
      deletedId: seed.deletedBeforeEntryId,
      type: seed.type,
    }),
  );

  postSchemaValidationCaptures.push(
    await runExpectedErrorMutation(
      'post-schema-create-missing-required-summary',
      queries.entryCreate,
      {
        metaobject: {
          type: seed.type,
          handle: `codex-har-245-missing-summary-${runId}`,
          fields: [{ key: 'title', value: `HAR-245 missing summary title ${runId}` }],
        },
      },
      ['data', 'metaobjectCreate', 'userErrors'],
    ),
  );

  postSchemaValidationCaptures.push(
    await runExpectedErrorMutation(
      'post-schema-update-removed-legacy-field',
      queries.entryUpdate,
      {
        id: seed.preEntryId,
        metaobject: {
          fields: [{ key: 'legacy', value: `HAR-245 invalid legacy ${runId}` }],
        },
      },
      ['data', 'metaobjectUpdate', 'userErrors'],
    ),
  );

  postSchemaMutationCaptures.push(
    await runSuccessMutation(
      'post-schema-update-existing-entry',
      queries.entryUpdate,
      {
        id: seed.preEntryId,
        metaobject: {
          fields: [
            { key: 'summary', value: `HAR-245 pre summary after schema ${runId}` },
            { key: 'body', value: `HAR-245 pre body after schema ${runId}` },
          ],
        },
      },
      ['data', 'metaobjectUpdate', 'userErrors'],
    ),
  );

  const postEntryCreate = await runSuccessMutation(
    'post-schema-entry-create',
    queries.entryCreate,
    {
      metaobject: {
        type: seed.type,
        handle: seed.postHandle,
        fields: [
          { key: 'summary', value: `HAR-245 post summary ${runId}` },
          { key: 'title', value: `HAR-245 post title ${runId}` },
          { key: 'body', value: `HAR-245 post body ${runId}` },
        ],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.postEntryId = extractId(
    postEntryCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'post metaobjectCreate',
  );
  postSchemaMutationCaptures.push(postEntryCreate);

  postSchemaMutationCaptures.push(
    await runSuccessMutation(
      'post-schema-entry-update',
      queries.entryUpdate,
      {
        id: seed.postEntryId,
        metaobject: {
          handle: `${seed.postHandle}-updated`,
          fields: [{ key: 'summary', value: `HAR-245 post summary updated ${runId}` }],
        },
      },
      ['data', 'metaobjectUpdate', 'userErrors'],
    ),
  );

  const deletedAfterCreate = await runSuccessMutation(
    'post-schema-delete-entry-create',
    queries.entryCreate,
    {
      metaobject: {
        type: seed.type,
        handle: seed.deletedAfterHandle,
        fields: [{ key: 'summary', value: `HAR-245 delete-after summary ${runId}` }],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.deletedAfterEntryId = extractId(
    deletedAfterCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'delete-after metaobjectCreate',
  );
  postSchemaMutationCaptures.push(deletedAfterCreate);

  postSchemaMutationCaptures.push(
    await runSuccessMutation('post-schema-delete-entry-delete', queries.entryDelete, { id: seed.deletedAfterEntryId }, [
      'data',
      'metaobjectDelete',
      'userErrors',
    ]),
  );

  finalReadCaptures.push(
    await captureGraphql('final-read-existing-entry', queries.read, {
      id: seed.preEntryId,
      handle: { type: seed.type, handle: seed.preHandle },
      deletedId: seed.deletedAfterEntryId,
      type: seed.type,
    }),
  );
  finalReadCaptures.push(
    await captureGraphql('final-read-post-entry', queries.read, {
      id: seed.postEntryId,
      handle: { type: seed.type, handle: `${seed.postHandle}-updated` },
      deletedId: seed.deletedAfterEntryId,
      type: seed.type,
    }),
  );

  await captureCleanup(cleanupCaptures);

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    seed,
    safety: {
      setup:
        'Creates one temporary metaobject definition and several temporary rows on the disposable conformance shop, exercises add/update/delete before and after a schema edit, then deletes remaining rows and the definition before writing the successful fixture.',
      paritySpecs:
        'The matching parity spec replays the captured mutation/read sequence through local staging; volatile IDs, timestamps, and opaque cursors are path-scoped expected differences.',
    },
    setup: setupCaptures,
    preSchemaReads,
    schemaChanges: schemaChangeCaptures,
    postSchemaValidation: postSchemaValidationCaptures,
    postSchemaMutations: postSchemaMutationCaptures,
    finalReads: finalReadCaptures,
    cleanup: cleanupCaptures,
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await captureCleanup(cleanupCaptures);
  } catch (cleanupError) {
    cleanupCaptures.push({
      name: 'cleanup-failure',
      request: {
        query: '',
        variables: {},
      },
      status: 0,
      response: cleanupError instanceof Error ? cleanupError.message : String(cleanupError),
    });
  }

  await writeBlocker('metaobject schema-change lifecycle capture', error, [
    ...setupCaptures,
    ...preSchemaReads,
    ...schemaChangeCaptures,
    ...postSchemaValidationCaptures,
    ...postSchemaMutationCaptures,
    ...finalReadCaptures,
    ...cleanupCaptures,
  ]);
  throw error;
}
