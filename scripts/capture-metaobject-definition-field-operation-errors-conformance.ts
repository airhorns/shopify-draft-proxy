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
  type: string;
  definitionId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobjectDefinitionUpdate-field-operation-errors.json');
const runId = Date.now().toString();
const seed: Seed = {
  type: `field_operation_errors_${runId}`,
};

const requestPaths = {
  create: 'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-field-operation-errors-create.graphql',
  update: 'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-field-operation-errors-update.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const deleteDefinitionMutation = `#graphql
  mutation MetaobjectDefinitionFieldOperationErrorsDelete($id: ID!) {
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

function assertUserErrors(payload: unknown, pathParts: string[], label: string, expectedCodes: string[]): void {
  const userErrors = readUserErrors(payload, pathParts);
  const actualCodes = userErrors.map((error) => readPath(error, ['code']));
  if (JSON.stringify(actualCodes) !== JSON.stringify(expectedCodes)) {
    throw new Error(
      `${label} returned unexpected userError codes: ${JSON.stringify(
        { expectedCodes, actualCodes, userErrors },
        null,
        2,
      )}`,
    );
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

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobjectDefinitionUpdate-field-operation-errors-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm conformance:capture -- --run metaobject-definition-field-operation-errors',
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

async function cleanupDefinition(cleanup: Capture[]): Promise<void> {
  if (!seed.definitionId) {
    return;
  }

  cleanup.push(await captureGraphql('cleanup-definition-delete', deleteDefinitionMutation, { id: seed.definitionId }));
}

const setupDefinition = {
  type: seed.type,
  name: `Field operation errors ${runId}`,
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
};

const setupCaptures: Capture[] = [];
const cases: Record<string, Capture> = {};
const cleanup: Capture[] = [];

try {
  const definitionCreate = await captureGraphql('setup-definition-create', queries.create, {
    definition: setupDefinition,
  });
  assertNoUserErrors(definitionCreate.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], 'setup create');
  seed.definitionId = extractId(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'setup create',
  );
  setupCaptures.push(definitionCreate);

  cases['createExisting'] = await captureGraphql('create-existing-field', queries.update, {
    id: seed.definitionId,
    definition: {
      fieldDefinitions: [
        {
          create: {
            key: 'title',
            name: 'Title again',
            type: 'single_line_text_field',
          },
        },
      ],
    },
  });
  assertUserErrors(
    cases['createExisting'].response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'create-existing-field',
    ['OBJECT_FIELD_TAKEN'],
  );

  cases['updateMissing'] = await captureGraphql('update-missing-field', queries.update, {
    id: seed.definitionId,
    definition: {
      fieldDefinitions: [
        {
          update: {
            key: 'missing_update',
            name: 'Missing update',
          },
        },
      ],
    },
  });
  assertUserErrors(
    cases['updateMissing'].response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'update-missing-field',
    ['UNDEFINED_OBJECT_FIELD'],
  );

  cases['deleteMissing'] = await captureGraphql('delete-missing-field', queries.update, {
    id: seed.definitionId,
    definition: {
      fieldDefinitions: [
        {
          delete: {
            key: 'missing_delete',
          },
        },
      ],
    },
  });
  assertUserErrors(
    cases['deleteMissing'].response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'delete-missing-field',
    ['UNDEFINED_OBJECT_FIELD'],
  );

  cases['multiConflict'] = await captureGraphql('multi-conflict-field-operations', queries.update, {
    id: seed.definitionId,
    definition: {
      fieldDefinitions: [
        {
          update: {
            key: 'missing_update',
            name: 'Missing update',
          },
        },
        {
          create: {
            key: 'title',
            name: 'Title again',
            type: 'single_line_text_field',
          },
        },
        {
          delete: {
            key: 'missing_delete',
          },
        },
      ],
    },
  });
  assertUserErrors(
    cases['multiConflict'].response,
    ['data', 'metaobjectDefinitionUpdate', 'userErrors'],
    'multi-conflict-field-operations',
    ['UNDEFINED_OBJECT_FIELD', 'OBJECT_FIELD_TAKEN', 'UNDEFINED_OBJECT_FIELD'],
  );

  await cleanupDefinition(cleanup);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'metaobjectDefinitionUpdate field-operation conflict userError codes, field paths, messages, and ordering.',
        seed,
        setup: setupCaptures,
        cases,
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
    await cleanupDefinition(cleanup);
  } finally {
    await writeBlocker('capture', error, [...setupCaptures, ...Object.values(cases), ...cleanup]);
  }
  throw error;
}
