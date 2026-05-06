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
  setupType: string;
  setupDefinitionId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'definition-name-type-description-length.json');
const runId = Date.now().toString();
const seed: Seed = {
  runId,
  setupType: `definition_length_${runId}`,
};

const requestPaths = {
  create: 'config/parity-requests/metaobjects/definition-name-type-description-length-create.graphql',
  update: 'config/parity-requests/metaobjects/definition-name-type-description-length-update.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const deleteDefinitionMutation = `#graphql
  mutation DeleteMetaobjectDefinition($id: ID!) {
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

function assertHasUserErrorCode(payload: unknown, pathParts: string[], code: string, label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (!userErrors.some((error) => readPath(error, ['code']) === code)) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(userErrors, null, 2)}`);
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

function definitionInput(
  type: string,
  name: string,
  description = 'Definition validation capture.',
): Record<string, unknown> {
  return {
    type,
    name,
    description,
    displayNameKey: 'title',
    fieldDefinitions: [
      {
        key: 'title',
        name: 'Title',
        description: 'Title field for definition validation capture.',
        type: 'single_line_text_field',
        required: true,
      },
    ],
  };
}

async function cleanupDefinitions(cleanup: Capture[]): Promise<void> {
  if (!seed.setupDefinitionId) {
    return;
  }

  cleanup.push(
    await captureGraphql('cleanup-metaobject-definition-delete', deleteDefinitionMutation, {
      id: seed.setupDefinitionId,
    }),
  );
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `definition-name-type-description-length-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm conformance:capture -- --run metaobject-definition-name-type-description-length',
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

const captures: Capture[] = [];
const cleanup: Capture[] = [];

try {
  const setup = await captureGraphql('setup-valid-definition', queries.create, {
    definition: definitionInput(seed.setupType, `Definition Length ${runId}`),
  });
  assertNoUserErrors(setup.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], 'setup-valid-definition');
  seed.setupDefinitionId = readStringPath(
    setup.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'setup-valid-definition',
  );
  captures.push(setup);

  const blankNameCreate = await captureGraphql('blank-name-create', queries.create, {
    definition: definitionInput(`definition_blank_${runId}`, ''),
  });
  assertHasUserErrorCode(
    blankNameCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'BLANK',
    'blank-name-create',
  );
  captures.push(blankNameCreate);

  const longNameCreate = await captureGraphql('long-name-create', queries.create, {
    definition: definitionInput(`definition_long_name_${runId}`, 'n'.repeat(256)),
  });
  assertHasUserErrorCode(
    longNameCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'TOO_LONG',
    'long-name-create',
  );
  captures.push(longNameCreate);

  const longDescriptionCreate = await captureGraphql('long-description-create', queries.create, {
    definition: definitionInput(`definition_long_description_${runId}`, 'Long Description', 'd'.repeat(256)),
  });
  assertHasUserErrorCode(
    longDescriptionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'TOO_LONG',
    'long-description-create',
  );
  captures.push(longDescriptionCreate);

  const shortTypeCreate = await captureGraphql('short-type-create', queries.create, {
    definition: definitionInput('ab', 'Short Type'),
  });
  assertHasUserErrorCode(
    shortTypeCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'TOO_SHORT',
    'short-type-create',
  );
  captures.push(shortTypeCreate);

  const blankNameUpdate = await captureGraphql('blank-name-update', queries.update, {
    id: seed.setupDefinitionId,
    definition: { name: '' },
  });
  assertHasUserErrorCode(
    blankNameUpdate.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'BLANK',
    'blank-name-update',
  );
  captures.push(blankNameUpdate);

  const longNameUpdate = await captureGraphql('long-name-update', queries.update, {
    id: seed.setupDefinitionId,
    definition: { name: 'n'.repeat(256) },
  });
  assertHasUserErrorCode(
    longNameUpdate.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'TOO_LONG',
    'long-name-update',
  );
  captures.push(longNameUpdate);

  const longDescriptionUpdate = await captureGraphql('long-description-update', queries.update, {
    id: seed.setupDefinitionId,
    definition: { description: 'd'.repeat(256) },
  });
  assertHasUserErrorCode(
    longDescriptionUpdate.response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'TOO_LONG',
    'long-description-update',
  );
  captures.push(longDescriptionUpdate);

  await cleanupDefinitions(cleanup);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'Metaobject definition create/update validation capture for name presence, name/description maximum length, and type minimum length.',
        seed,
        setup,
        blankNameCreate,
        longNameCreate,
        longDescriptionCreate,
        shortTypeCreate,
        blankNameUpdate,
        longNameUpdate,
        longDescriptionUpdate,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await cleanupDefinitions(cleanup);
  } finally {
    await writeBlocker('capture', error, captures);
  }
  throw error;
}
