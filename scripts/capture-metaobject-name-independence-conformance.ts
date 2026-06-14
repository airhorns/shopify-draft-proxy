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

type Seed = {
  runId: string;
  type: string;
  handle: string;
  title: string;
  definitionId?: string;
  metaobjectId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-name-independence-create.json');
const runId = Date.now().toString();
const seed: Seed = {
  runId,
  type: `name_independence_${runId}`,
  handle: `normal-operation-${runId}`,
  title: `Normal Operation ${runId}`,
};

const definitionCreateDocument = await readFile(
  'config/parity-requests/metaobjects/metaobject-name-independence-definition-create.graphql',
  'utf8',
);
const createDocument = await readFile(
  'config/parity-requests/metaobjects/metaobject-name-independence-create.graphql',
  'utf8',
);
const readDocument = await readFile(
  'config/parity-requests/metaobjects/metaobject-name-independence-read.graphql',
  'utf8',
);

const deleteMetaobjectMutation = `#graphql
  mutation MetaobjectNameIndependenceCleanupDelete($id: ID!) {
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

const deleteDefinitionMutation = `#graphql
  mutation MetaobjectNameIndependenceDefinitionCleanupDelete($id: ID!) {
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
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function readStringPath(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }
  return value;
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

async function cleanup(cleanupCaptures: Capture[]): Promise<void> {
  if (seed.metaobjectId) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-metaobject-delete', deleteMetaobjectMutation, { id: seed.metaobjectId }),
    );
  }
  if (seed.definitionId) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-metaobject-definition-delete', deleteDefinitionMutation, { id: seed.definitionId }),
    );
  }
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobject-name-independence-create-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm conformance:capture -- --run metaobject-name-independence-create',
        blocker: { stage, message },
        seed,
        partialCaptures: captures,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
}

const captures: Capture[] = [];
const cleanupCaptures: Capture[] = [];

try {
  const definitionCreate = await captureGraphql('definition-create', definitionCreateDocument, {
    definition: {
      type: seed.type,
      name: `Name Independence ${runId}`,
      displayNameKey: 'title',
      capabilities: { publishable: { enabled: true } },
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
  });
  assertNoUserErrors(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'definition-create',
  );
  seed.definitionId = readStringPath(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition-create',
  );
  captures.push(definitionCreate);

  const entryCreate = await captureGraphql('entry-create-normal-operation-name', createDocument, {
    metaobject: {
      type: seed.type,
      handle: seed.handle,
      fields: [
        { key: 'title', value: seed.title },
        { key: 'body', value: `Captured through mutation CreateMetaobject ${runId}` },
      ],
    },
  });
  assertNoUserErrors(
    entryCreate.response,
    ['data', 'createdNormally', 'userErrors'],
    'entry-create-normal-operation-name',
  );
  seed.metaobjectId = readStringPath(
    entryCreate.response,
    ['data', 'createdNormally', 'metaobject', 'id'],
    'entry-create-normal-operation-name',
  );
  captures.push(entryCreate);

  const readAfterCreate = await captureGraphql('read-after-create', readDocument, {
    id: seed.metaobjectId,
    handle: { type: seed.type, handle: seed.handle },
    type: seed.type,
  });
  captures.push(readAfterCreate);

  await cleanup(cleanupCaptures);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        adminOrigin,
        apiVersion,
        source: 'scripts/capture-metaobject-name-independence-conformance.ts',
        seed,
        definitionCreate: captures[0],
        entryCreate: captures[1],
        readAfterCreate: captures[2],
        cleanup: cleanupCaptures,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await cleanup(cleanupCaptures);
  } catch (cleanupError) {
    console.error(`Cleanup failed: ${cleanupError instanceof Error ? cleanupError.message : String(cleanupError)}`);
  }
  await writeBlocker('capture', error, [...captures, ...cleanupCaptures]);
  throw error;
}
