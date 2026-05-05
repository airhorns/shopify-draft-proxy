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
  appTypeInput: string;
  appTypeResolved: string;
  successTypeInput: string;
  successTypeResolved: string;
  appDefinitionId?: string;
  successDefinitionId?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-definition-create-validation.json');
const runId = Date.now().toString();
const appRest = `har_673_app_${runId}`;
const successTypeRest = `har_673_case_${runId}`;
const appApiClientDbId = '347082227713';
const seed: Seed = {
  runId,
  appTypeInput: `$app:${appRest}`,
  appTypeResolved: `app--${appApiClientDbId}--${appRest}`,
  successTypeInput: `HAR_673_CASE_${runId}`,
  successTypeResolved: successTypeRest,
};

const requestPaths = {
  create: 'config/parity-requests/metaobjects/metaobject-definition-create-validation-create.graphql',
  update: 'config/parity-requests/metaobjects/metaobject-definition-create-validation-update.graphql',
  readByType: 'config/parity-requests/metaobjects/metaobject-definition-create-validation-read-by-type.graphql',
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

function assertHasUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length === 0) {
    throw new Error(`${label} did not return userErrors: ${JSON.stringify(payload, null, 2)}`);
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

function definitionInput(type: string, name: string, access?: Record<string, unknown>): Record<string, unknown> {
  return {
    type,
    name,
    access,
    displayNameKey: 'title',
    fieldDefinitions: [
      {
        key: 'title',
        name: 'Title',
        description: 'Title field for HAR-673 validation capture.',
        type: 'single_line_text_field',
        required: true,
      },
    ],
  };
}

async function cleanupDefinitions(cleanup: Capture[]): Promise<void> {
  for (const id of [seed.appDefinitionId, seed.successDefinitionId]) {
    if (!id) {
      continue;
    }

    cleanup.push(await captureGraphql('cleanup-metaobject-definition-delete', deleteDefinitionMutation, { id }));
  }
}

async function writeBlocker(stage: string, error: unknown, captures: Capture[]): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobject-definition-create-validation-blocker-${runId}.json`);
  const message = error instanceof Error ? error.message : String(error);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        command: 'corepack pnpm conformance:capture -- --id metaobject-definition-create-validation',
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
  const tooShort = await captureGraphql('too-short-type', queries.create, {
    definition: definitionInput('ab', `HAR 673 Too Short ${runId}`),
  });
  assertHasUserErrors(tooShort.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], 'too-short-type');
  captures.push(tooShort);

  const invalidFormat = await captureGraphql('invalid-format-type', queries.create, {
    definition: definitionInput('Has Spaces!', `HAR 673 Invalid ${runId}`),
  });
  assertHasUserErrors(
    invalidFormat.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'invalid-format-type',
  );
  captures.push(invalidFormat);

  const appPrefixed = await captureGraphql('app-prefixed-type', queries.create, {
    definition: definitionInput(seed.appTypeInput, `HAR 673 App ${runId}`, { admin: 'MERCHANT_READ_WRITE' }),
  });
  assertNoUserErrors(appPrefixed.response, ['data', 'metaobjectDefinitionCreate', 'userErrors'], 'app-prefixed-type');
  seed.appDefinitionId = readStringPath(
    appPrefixed.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'app-prefixed-type',
  );
  captures.push(appPrefixed);

  const appReadByType = await captureGraphql('app-read-by-type', queries.readByType, { type: seed.appTypeInput });
  captures.push(appReadByType);

  const successfulCreate = await captureGraphql('successful-create', queries.create, {
    definition: definitionInput(seed.successTypeInput, `HAR 673 Case ${runId}`),
  });
  assertNoUserErrors(
    successfulCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'successful-create',
  );
  seed.successDefinitionId = readStringPath(
    successfulCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'successful-create',
  );
  captures.push(successfulCreate);

  const successReadByType = await captureGraphql('success-read-by-type', queries.readByType, {
    type: seed.successTypeResolved,
  });
  captures.push(successReadByType);

  const duplicateCase = await captureGraphql('duplicate-case-create', queries.create, {
    definition: definitionInput(seed.successTypeResolved, `HAR 673 Duplicate ${runId}`),
  });
  assertHasUserErrors(
    duplicateCase.response,
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    'duplicate-case-create',
  );
  captures.push(duplicateCase);

  const validUpdate = await captureGraphql('valid-update', queries.update, {
    id: seed.appDefinitionId,
    definition: {
      name: `HAR 673 App Updated ${runId}`,
    },
  });
  assertNoUserErrors(validUpdate.response, ['data', 'metaobjectDefinitionUpdate', 'userErrors'], 'valid-update');
  captures.push(validUpdate);

  await cleanupDefinitions(cleanup);

  const hydratedDefinition = readPath(successReadByType.response, ['data', 'metaobjectDefinitionByType']);
  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'HAR-673 metaobjectDefinitionCreate/Update validation capture for type format/length, app namespace resolution, case-insensitive duplicates, and field key validation.',
        seed,
        tooShort,
        invalidFormat,
        appPrefixed,
        appReadByType,
        successfulCreate,
        successReadByType,
        duplicateCase,
        validUpdate,
        cleanup,
        upstreamCalls: [
          {
            operationName: 'MetaobjectDefinitionHydrateByType',
            variables: { type: seed.successTypeResolved },
            query: 'sha:hand-synthesized-from-capture',
            response: {
              status: 200,
              body: {
                data: {
                  metaobjectDefinitionByType: hydratedDefinition,
                },
              },
            },
          },
        ],
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
