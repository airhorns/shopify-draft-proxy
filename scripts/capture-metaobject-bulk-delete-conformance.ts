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
  lastHandle: string;
  definitionId?: string;
  firstId?: string;
  lastId?: string;
  entryIds: string[];
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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-bulk-delete-type-lifecycle.json');
const runId = Date.now().toString();
const seed: SeedState = {
  type: `codex_bulk_delete_multi_page_${runId}`,
  firstHandle: `codex-bulk-delete-first-${runId}`,
  lastHandle: `codex-bulk-delete-last-${runId}`,
  entryIds: [],
};

const bulkDeleteMutation = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-type-delete.graphql',
  'utf8',
);
const bulkReadQuery = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-type-read.graphql',
  'utf8',
);
const jobReadQuery = await readFile(
  'config/parity-requests/metaobjects/metaobject-bulk-delete-job-read.graphql',
  'utf8',
);
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

function recordedCall(operationName: string, capture: Capture): RecordedCall {
  return {
    operationName,
    variables: capture.request.variables,
    query: capture.request.query,
    response: {
      status: capture.status,
      body: capture.response,
    },
  };
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
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
  for (const entryId of seed.entryIds) {
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
const jobReads: Capture[] = [];
const downstreamReads: Capture[] = [];
const cleanupCaptures: Capture[] = [];
const upstreamCalls: RecordedCall[] = [];
let fatalError: unknown = null;
let bulkDeleteSucceeded = false;

try {
  const definitionCreate = await runSuccessMutation(
    'setup-definition-create',
    definitionCreateMutation,
    {
      definition: {
        type: seed.type,
        name: `Codex Bulk Delete Multi Page ${runId}`,
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

  for (let index = 0; index < 251; index += 1) {
    const isLast = index === 250;
    const handle = isLast ? seed.lastHandle : index === 0 ? seed.firstHandle : `codex-bulk-delete-${index}-${runId}`;
    const entryCreate = await runSuccessMutation(
      `setup-entry-create-${index}`,
      entryCreateMutation,
      {
        metaobject: {
          type: seed.type,
          handle,
          fields: [
            { key: 'title', value: `Bulk delete entry ${index}` },
            { key: 'body', value: `Entry ${index} selected by type bulk delete.` },
          ],
        },
      },
      ['data', 'metaobjectCreate', 'userErrors'],
    );
    setupCaptures.push(entryCreate);
    const entryId = extractId(
      entryCreate.response,
      ['data', 'metaobjectCreate', 'metaobject', 'id'],
      `entry ${index} create`,
    );
    seed.entryIds.push(entryId);
    if (index === 0) seed.firstId = entryId;
    if (isLast) seed.lastId = entryId;
  }

  const readVariables = {
    type: seed.type,
    firstId: seed.firstId,
    lastId: seed.lastId,
    lastHandleQuery: `handle:${seed.lastHandle}`,
  };

  const hydrateRead = await captureGraphql('seeded-bounded-type-hydrate', bulkDeleteHydrateByTypeQuery, {
    type: seed.type,
  });
  seededReads.push(hydrateRead);
  const beforeDeleteRead = await captureGraphql('seeded-before-bulk-delete-read', bulkReadQuery, readVariables);
  seededReads.push(beforeDeleteRead);
  upstreamCalls.push(recordedCall('MetaobjectBulkDeleteHydrateByType', hydrateRead));
  upstreamCalls.push(recordedCall('MetaobjectBulkDeleteByTypeRead', beforeDeleteRead));
  const hydratedNodes = readPath(hydrateRead.response, ['data', 'catalog', 'nodes']);
  if (!Array.isArray(hydratedNodes) || hydratedNodes.length !== 250) {
    throw new Error(`bounded hydrate did not return exactly 250 rows: ${JSON.stringify(hydratedNodes, null, 2)}`);
  }
  if (readPath(beforeDeleteRead.response, ['data', 'last', 'id']) !== seed.lastId) {
    throw new Error(
      `the row beyond the bounded hydrate was not readable: ${JSON.stringify(beforeDeleteRead.response, null, 2)}`,
    );
  }

  const bulkDelete = await runSuccessMutation('bulk-delete-by-type', bulkDeleteMutation, { type: seed.type }, [
    'data',
    'metaobjectBulkDelete',
    'userErrors',
  ]);
  bulkDeleteCaptures.push(bulkDelete);
  bulkDeleteSucceeded = true;
  const jobId = extractId(bulkDelete.response, ['data', 'metaobjectBulkDelete', 'job', 'id'], 'bulk delete job');
  for (let attempt = 0; attempt < 60; attempt += 1) {
    const jobRead = await captureGraphql(`bulk-delete-job-read-${attempt}`, jobReadQuery, { id: jobId });
    jobReads.push(jobRead);
    if (readPath(jobRead.response, ['data', 'job', 'done']) === true) break;
    await delay(250);
  }
  if (readPath(jobReads.at(-1)?.response, ['data', 'job', 'done']) !== true) {
    throw new Error(`bulk delete job did not settle: ${JSON.stringify(jobReads.at(-1), null, 2)}`);
  }
  let settledDownstreamRead: Capture | null = null;
  for (let attempt = 0; attempt < 120; attempt += 1) {
    settledDownstreamRead = await captureGraphql('downstream-after-bulk-delete-read', bulkReadQuery, readVariables);
    const last = readPath(settledDownstreamRead.response, ['data', 'last']);
    const catalog = readPath(settledDownstreamRead.response, ['data', 'catalog', 'nodes']);
    if (last === null && Array.isArray(catalog) && catalog.length === 0) break;
    await delay(1_000);
  }
  if (readPath(settledDownstreamRead?.response, ['data', 'last']) !== null) {
    throw new Error('the row beyond the first page did not disappear after the bulk-delete job settled');
  }
  if (settledDownstreamRead) downstreamReads.push(settledDownstreamRead);
} catch (error) {
  fatalError = error;
  await writeBlocker('capture', error, [...setupCaptures, ...seededReads, ...bulkDeleteCaptures, ...downstreamReads]);
}

try {
  if (bulkDeleteSucceeded) seed.entryIds = [];
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
      jobReads,
      downstreamReads,
      cleanup: cleanupCaptures,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote metaobject bulk-delete conformance fixture to ${outputPath}`);
