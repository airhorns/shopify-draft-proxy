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

type EntrySeed = {
  handle: string;
  title: string;
  subtitle: string;
  id?: string;
};

type CaptureSeed = {
  type: string;
  definitionId?: string;
  entries: EntrySeed[];
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobjects-filter-sort.json');
const requestDir = path.join('config', 'parity-requests', 'metaobjects');
const definitionCreateDocument = await readFile(
  path.join(requestDir, 'metaobjects-filter-sort-definition-create.graphql'),
  'utf8',
);
const entryCreateDocument = await readFile(
  path.join(requestDir, 'metaobjects-filter-sort-entry-create.graphql'),
  'utf8',
);
const readDocument = await readFile(path.join(requestDir, 'metaobjects-filter-sort-read.graphql'), 'utf8');

const runId = Date.now().toString();
const seed: CaptureSeed = {
  type: `codex_metaobject_filter_sort_${runId}`,
  entries: [
    { handle: 'charlie-entry', title: 'Charlie', subtitle: 'Lake story' },
    { handle: 'alpha-entry', title: 'Alpha', subtitle: 'River note' },
    { handle: 'bravo-entry', title: 'Bravo', subtitle: 'Hill note' },
  ],
};

const entryDeleteMutation = `#graphql
  mutation MetaobjectsFilterSortEntryDelete($id: ID!) {
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
  mutation MetaobjectsFilterSortDefinitionDelete($id: ID!) {
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
    if (object === null) return undefined;
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
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function catalogHandles(payload: unknown): string[] {
  const nodes = readPath(payload, ['data', 'catalog', 'nodes']);
  if (!Array.isArray(nodes)) return [];
  return nodes
    .map((node) => readObject(node)?.['handle'])
    .filter((handle): handle is string => typeof handle === 'string');
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: Record<string, unknown> = {}): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

async function runSetupMutation(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  userErrorPath: string[],
): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, name);
  assertNoUserErrors(result.payload, userErrorPath, name);
  return captureFromResult(name, query, variables, result);
}

async function waitForSearchIndex(): Promise<void> {
  const expectedHandles = seed.entries.map((entry) => entry.handle).sort();
  const readHandles = async (variables: Record<string, unknown>): Promise<string[]> => {
    const result = await runGraphqlRaw(readDocument, variables);
    assertGraphqlOk(result, 'wait-for-metaobject-search-index');
    return catalogHandles(result.payload).sort();
  };

  for (let attempt = 0; attempt < 60; attempt += 1) {
    const idHandles = await readHandles({
      type: seed.type,
      query: null,
      sortKey: 'id',
      reverse: false,
    });
    const displayNameHandles = await readHandles({
      type: seed.type,
      query: 'display_name:Alpha*',
      sortKey: null,
      reverse: false,
    });
    const fieldHandles = await readHandles({
      type: seed.type,
      query: 'fields.subtitle:Lake*',
      sortKey: null,
      reverse: false,
    });
    const displayNameSortHandles = await readHandles({
      type: seed.type,
      query: null,
      sortKey: 'display_name',
      reverse: true,
    });
    if (
      expectedHandles.every((handle) => idHandles.includes(handle)) &&
      displayNameHandles.includes('alpha-entry') &&
      fieldHandles.includes('charlie-entry') &&
      expectedHandles.every((handle) => displayNameSortHandles.includes(handle))
    ) {
      return;
    }
    await sleep(2000);
  }
  throw new Error(`Timed out waiting for metaobjects(type:) to include ${expectedHandles.join(', ')}`);
}

async function captureCleanup(cleanup: Capture[]): Promise<void> {
  for (const entry of [...seed.entries].reverse()) {
    if (!entry.id) continue;
    cleanup.push(await captureGraphql(`cleanup-${entry.handle}`, entryDeleteMutation, { id: entry.id }));
  }
  if (seed.definitionId) {
    cleanup.push(await captureGraphql('cleanup-definition', definitionDeleteMutation, { id: seed.definitionId }));
  }
}

async function writeBlocker(stage: string, error: unknown, partial: Record<string, unknown>): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobjects-filter-sort-blocker-${runId}.json`);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command:
          'SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm tsx scripts/capture-metaobjects-filter-sort-conformance.ts',
        blocker: {
          stage,
          message: error instanceof Error ? error.message : String(error),
        },
        seed,
        partial,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
}

const setup: { definitionCreate?: Capture; entryCreates: Capture[] } = { entryCreates: [] };
const reads: Record<string, Capture> = {};
const cleanup: Capture[] = [];

try {
  setup.definitionCreate = await runSetupMutation(
    'definition-create',
    definitionCreateDocument,
    {
      definition: {
        type: seed.type,
        name: `Metaobject filter sort ${runId}`,
        displayNameKey: 'title',
        fieldDefinitions: [
          {
            key: 'title',
            name: 'Title',
            type: 'single_line_text_field',
            required: true,
            capabilities: { adminFilterable: { enabled: true } },
          },
          {
            key: 'subtitle',
            name: 'Subtitle',
            type: 'single_line_text_field',
            required: false,
            capabilities: { adminFilterable: { enabled: true } },
          },
        ],
      },
    },
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
  );
  seed.definitionId = extractId(
    setup.definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'metaobjectDefinitionCreate',
  );

  for (const entry of seed.entries) {
    const capture = await runSetupMutation(
      `entry-create-${entry.handle}`,
      entryCreateDocument,
      {
        metaobject: {
          type: seed.type,
          handle: entry.handle,
          fields: [
            { key: 'title', value: entry.title },
            { key: 'subtitle', value: entry.subtitle },
          ],
        },
      },
      ['data', 'metaobjectCreate', 'userErrors'],
    );
    entry.id = extractId(capture.response, ['data', 'metaobjectCreate', 'metaobject', 'id'], 'metaobjectCreate');
    setup.entryCreates.push(capture);
  }

  await waitForSearchIndex();

  reads['displayName'] = await captureGraphql('filter-display-name', readDocument, {
    type: seed.type,
    query: 'display_name:Alpha*',
    sortKey: null,
    reverse: false,
  });
  reads['fieldValue'] = await captureGraphql('filter-field-value', readDocument, {
    type: seed.type,
    query: 'fields.subtitle:Lake*',
    sortKey: null,
    reverse: false,
  });
  reads['handle'] = await captureGraphql('filter-handle', readDocument, {
    type: seed.type,
    query: 'handle:bravo-entry',
    sortKey: null,
    reverse: false,
  });
  reads['idRangeNoMatch'] = await captureGraphql('filter-id-range-no-match', readDocument, {
    type: seed.type,
    query: 'id:<0',
    sortKey: null,
    reverse: false,
  });
  reads['updatedAtRangeNoMatch'] = await captureGraphql('filter-updated-at-range-no-match', readDocument, {
    type: seed.type,
    query: 'updated_at:<1900-01-01T00:00:00Z',
    sortKey: null,
    reverse: false,
  });
  reads['displayNameReverse'] = await captureGraphql('sort-display-name-reverse', readDocument, {
    type: seed.type,
    query: null,
    sortKey: 'display_name',
    reverse: true,
  });
  reads['idReverse'] = await captureGraphql('sort-id-reverse', readDocument, {
    type: seed.type,
    query: null,
    sortKey: 'id',
    reverse: true,
  });

  await captureCleanup(cleanup);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        seed,
        safety: {
          setup:
            'Creates one temporary metaobject definition and three temporary entries on the disposable conformance shop, waits for the metaobjects(type:) search index, records filter/sort reads, then deletes all created entries and the definition before writing the successful fixture.',
        },
        setup,
        reads,
        cleanup,
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await captureCleanup(cleanup);
  } catch (cleanupError) {
    cleanup.push({
      name: 'cleanup-failure',
      request: { query: '', variables: {} },
      status: 0,
      response: cleanupError instanceof Error ? cleanupError.message : String(cleanupError),
    });
  }
  await writeBlocker('metaobjects filter/sort capture', error, { setup, reads, cleanup });
  throw error;
}
