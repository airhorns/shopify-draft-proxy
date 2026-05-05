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
const outputPath = path.join(outputDir, 'metaobject-definition-delete-cascade.json');
const runId = Date.now().toString();
const seed: SeedState = {
  type: `codex_har_675_delete_cascade_${runId}`,
  firstHandle: `codex-har-675-cascade-first-${runId}`,
  secondHandle: `codex-har-675-cascade-second-${runId}`,
};

const definitionCreateMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-definition-lifecycle-create.graphql',
  'utf8',
);
const entryCreateMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-definition-delete-cascade-entry-create.graphql',
  'utf8',
);
const definitionDeleteMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-definition-lifecycle-delete.graphql',
  'utf8',
);
const downstreamReadQuery = await readFile(
  'config/parity-requests/metaobjects/metaobject-definition-delete-cascade-read.graphql',
  'utf8',
);

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

const entryDeleteMutation = `#graphql
  mutation MetaobjectDefinitionDeleteCascadeEntryCleanup($id: ID!) {
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
  const blockerPath = path.join(outputDir, `metaobject-definition-delete-cascade-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command:
          'SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm conformance:capture -- --run metaobject-definition-delete-cascade',
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

function readVariables(): Record<string, unknown> {
  if (!seed.definitionId || !seed.firstId || !seed.secondId) {
    throw new Error(`Cannot build read variables before setup ids exist: ${JSON.stringify(seed, null, 2)}`);
  }

  return {
    definitionId: seed.definitionId,
    type: seed.type,
    firstId: seed.firstId,
    secondId: seed.secondId,
    firstHandle: {
      type: seed.type,
      handle: seed.firstHandle,
    },
    secondHandle: {
      type: seed.type,
      handle: seed.secondHandle,
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
  for (const entryId of [seed.firstId, seed.secondId]) {
    if (entryId) {
      cleanup.push(await captureGraphql('cleanup-metaobject-delete', entryDeleteMutation, { id: entryId }));
    }
  }

  if (seed.definitionId) {
    cleanup.push(
      await captureGraphql('cleanup-metaobject-definition-delete', definitionDeleteMutation, { id: seed.definitionId }),
    );
  }
}

const setupCaptures: Capture[] = [];
const hydrateCaptures: Capture[] = [];
const cascadeDeleteCaptures: Capture[] = [];
const downstreamReads: Capture[] = [];
const eventualReads: Capture[] = [];
const cleanupCaptures: Capture[] = [];
let fatalError: unknown = null;

try {
  hydrateCaptures.push(
    await captureGraphql('hydrate-definition-by-type-before-create', hydrateDefinitionByTypeQuery, { type: seed.type }),
  );

  const definitionCreate = await runSuccessMutation(
    'setup-definition-create',
    definitionCreateMutation,
    {
      definition: {
        type: seed.type,
        name: `Codex HAR-675 Cascade ${runId}`,
        description: 'Temporary HAR-675 conformance definition for definition-delete cascade capture.',
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            description: 'Display title.',
            type: 'single_line_text_field',
            required: true,
          },
          {
            key: 'body',
            name: 'Body',
            description: 'Body text.',
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
          { key: 'title', value: 'Cascade delete first' },
          { key: 'body', value: 'First entry selected by definition delete cascade.' },
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
          { key: 'title', value: 'Cascade delete second' },
          { key: 'body', value: 'Second entry selected by definition delete cascade.' },
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

  cascadeDeleteCaptures.push(
    await runSuccessMutation('definition-delete-cascade', definitionDeleteMutation, { id: seed.definitionId }, [
      'data',
      'metaobjectDefinitionDelete',
      'userErrors',
    ]),
  );
  downstreamReads.push(
    await captureGraphql('downstream-after-definition-delete-read', downstreamReadQuery, readVariables()),
  );
  await sleep(15_000);
  eventualReads.push(
    await captureGraphql('eventual-after-definition-delete-read', downstreamReadQuery, readVariables()),
  );
} catch (error) {
  fatalError = error;
  await writeBlocker('capture', error, [
    ...setupCaptures,
    ...hydrateCaptures,
    ...cascadeDeleteCaptures,
    ...downstreamReads,
    ...eventualReads,
  ]);
}

try {
  await captureCleanup(cleanupCaptures);
} catch (error) {
  await writeBlocker('cleanup', error, [
    ...setupCaptures,
    ...hydrateCaptures,
    ...cascadeDeleteCaptures,
    ...downstreamReads,
    ...eventualReads,
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
      setup: setupCaptures,
      hydrate: hydrateCaptures,
      cascadeDelete: cascadeDeleteCaptures,
      downstreamReads,
      eventualReads,
      cleanup: cleanupCaptures,
      upstreamCalls: [hydrateUpstreamCall(hydrateCapture)],
    },
    null,
    2,
  )}\n`,
);

console.log(`Wrote metaobject definition delete cascade conformance fixture to ${outputPath}`);
