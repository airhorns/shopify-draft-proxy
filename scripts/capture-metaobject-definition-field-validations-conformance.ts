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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'definition-create-field-validations.json');
const requestPath = 'config/parity-requests/metaobjects/definition-create-field-validations.graphql';
const createDefinitionMutation = await readFile(requestPath, 'utf8');
const runId = Date.now().toString();

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

function readUserErrors(payload: unknown): unknown[] {
  const value = readPath(payload, ['data', 'metaobjectDefinitionCreate', 'userErrors']);
  return Array.isArray(value) ? value : [];
}

function readDefinitionId(payload: unknown): string | null {
  const value = readPath(payload, ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id']);
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertHasUserErrors(capture: Capture): void {
  if (readUserErrors(capture.response).length === 0) {
    throw new Error(`${capture.name} did not return userErrors: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function assertNoUserErrors(capture: Capture): void {
  const userErrors = readUserErrors(capture.response);
  if (userErrors.length > 0) {
    throw new Error(`${capture.name} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
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

function field(key: string): Record<string, unknown> {
  return {
    key,
    name: key,
    type: 'single_line_text_field',
  };
}

function definition(
  type: string,
  name: string,
  displayNameKey: string,
  fieldDefinitions: Array<Record<string, unknown>>,
): Record<string, unknown> {
  return {
    type,
    name,
    displayNameKey,
    fieldDefinitions,
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

const cleanup: Capture[] = [];
let hyphenDefinitionId: string | null = null;

const reservedHandle = await captureGraphql('reserved-field-key', createDefinitionMutation, {
  definition: definition(`field_validation_reserved_${runId}`, 'Reserved Field Key', 'handle', [field('handle')]),
});
assertHasUserErrors(reservedHandle);

const duplicateKey = await captureGraphql('duplicate-field-key', createDefinitionMutation, {
  definition: definition(`field_validation_duplicate_${runId}`, 'Duplicate Field Key', 'title', [
    field('title'),
    field('title'),
  ]),
});
assertHasUserErrors(duplicateKey);

const missingDisplayNameKey = await captureGraphql('missing-display-name-key', createDefinitionMutation, {
  definition: definition(`field_validation_display_${runId}`, 'Missing Display Name Key', 'missing', [field('title')]),
});
assertHasUserErrors(missingDisplayNameKey);

const hyphenKey = await captureGraphql('hyphen-field-key', createDefinitionMutation, {
  definition: definition(`field_validation_hyphen_${runId}`, 'Hyphen Field Key', 'field-key', [field('field-key')]),
});
assertNoUserErrors(hyphenKey);
hyphenDefinitionId = readDefinitionId(hyphenKey.response);

const tooManyFields = await captureGraphql('too-many-field-definitions', createDefinitionMutation, {
  definition: definition(
    `field_validation_many_${runId}`,
    'Too Many Field Definitions',
    'field_1',
    Array.from({ length: 41 }, (_, index) => field(`field_${index + 1}`)),
  ),
});
assertHasUserErrors(tooManyFields);

if (hyphenDefinitionId !== null) {
  cleanup.push(
    await captureGraphql('cleanup-metaobject-definition-delete', deleteDefinitionMutation, { id: hyphenDefinitionId }),
  );
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      summary:
        'MetaobjectDefinitionCreate field validation capture for reserved field keys, duplicate field input, displayNameKey resolution, hyphen key acceptance, and max field count.',
      seed: {
        runId,
        hyphenDefinitionId,
      },
      reservedHandle,
      duplicateKey,
      missingDisplayNameKey,
      hyphenKey,
      tooManyFields,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
