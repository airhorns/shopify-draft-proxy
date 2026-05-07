/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const validationFunctionHandle = 'conformance-validation';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'validation-create-title-fallback-parity.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type Capture = {
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

type JsonRecord = Record<string, unknown>;

async function capture(query: string, variables: Record<string, unknown> = {}): Promise<Capture> {
  return {
    query,
    variables,
    response: await runGraphqlRequest(query, variables),
  };
}

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current: unknown = value;
  for (const segment of pathSegments) {
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

function assertNoTopLevelErrors(captureResult: Capture, context: string): void {
  if (
    captureResult.response.status < 200 ||
    captureResult.response.status >= 300 ||
    readRecord(captureResult.response.payload)?.['errors']
  ) {
    throw new Error(`${context} failed: ${JSON.stringify(captureResult.response, null, 2)}`);
  }
}

function readUserErrors(value: unknown): unknown[] {
  const userErrors = readRecord(value)?.['userErrors'];
  return Array.isArray(userErrors) ? userErrors : [];
}

function assertEmptyUserErrors(value: unknown, context: string): void {
  const userErrors = readUserErrors(value);
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function readValidationCreatePayload(captureResult: Capture, alias: string): JsonRecord {
  const payload = readRecord(readPath(captureResult.response.payload, ['data', alias]));
  if (!payload) {
    throw new Error(`Missing ${alias} payload: ${JSON.stringify(captureResult.response, null, 2)}`);
  }
  return payload;
}

function readValidationRecord(payload: JsonRecord, context: string): JsonRecord {
  const validation = readRecord(payload['validation']);
  if (!validation) {
    throw new Error(`${context} did not return a validation: ${JSON.stringify(payload, null, 2)}`);
  }
  return validation;
}

function readString(value: unknown, context: string): string {
  if (typeof value !== 'string') {
    throw new Error(`${context} was not a string: ${JSON.stringify(value)}`);
  }
  return value;
}

function assertTitle(value: unknown, expected: string, context: string): void {
  if (value !== expected) {
    throw new Error(`${context} title mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(value)}`);
  }
}

const functionLookupDocument = `#graphql
  query ValidationCreateTitleFallbackFunctionLookup {
    shopifyFunctions(first: 20) {
      nodes {
        id
        title
        handle
        apiType
        description
        appKey
        app {
          __typename
          id
          title
          handle
          apiKey
        }
      }
    }
  }
`;

const validationInventoryDocument = `#graphql
  query ValidationCreateTitleFallbackInventory {
    validations(first: 50) {
      nodes {
        id
        title
        shopifyFunction {
          id
          title
          handle
          apiType
        }
      }
    }
  }
`;

const validationCreateDocument = `#graphql
  mutation ValidationCreateTitleFallback($functionHandle: String!) {
    omitted: validationCreate(validation: { functionHandle: $functionHandle }) {
      validation {
        id
        title
      }
      userErrors {
        field
        message
        code
      }
    }
    explicitNull: validationCreate(validation: { functionHandle: $functionHandle, title: null }) {
      validation {
        id
        title
      }
      userErrors {
        field
        message
        code
      }
    }
    emptyString: validationCreate(validation: { functionHandle: $functionHandle, title: "" }) {
      validation {
        id
        title
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const validationReadDocument = `#graphql
  query ValidationCreateTitleFallbackValidationRead($id: ID!) {
    validation(id: $id) {
      id
      title
    }
  }
`;

const validationsReadDocument = `#graphql
  query ValidationCreateTitleFallbackValidationsRead {
    validations(first: 3) {
      nodes {
        title
      }
    }
  }
`;

const validationDeleteDocument = `#graphql
  mutation ValidationCreateTitleFallbackCleanup($id: ID!) {
    validationDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function validationNodes(captureResult: Capture): JsonRecord[] {
  const nodes = readPath(captureResult.response.payload, ['data', 'validations', 'nodes']);
  return readArray(nodes).flatMap((node) => {
    const record = readRecord(node);
    return record ? [record] : [];
  });
}

function validationId(record: JsonRecord): string {
  return readString(record['id'], 'validation id');
}

async function deleteValidations(ids: string[], label: string): Promise<Capture[]> {
  const deletes: Capture[] = [];
  for (const id of ids) {
    const result = await capture(validationDeleteDocument, { id });
    assertNoTopLevelErrors(result, `${label} validationDelete ${id}`);
    const payload = readPath(result.response.payload, ['data', 'validationDelete']);
    assertEmptyUserErrors(payload, `${label} validationDelete ${id}`);
    deletes.push(result);
  }
  return deletes;
}

async function cleanupExistingValidations(): Promise<{ inventory: Capture; deletes: Capture[] }> {
  const inventory = await capture(validationInventoryDocument);
  assertNoTopLevelErrors(inventory, 'pre-capture validations inventory');
  const ids = validationNodes(inventory).map(validationId);
  return {
    inventory,
    deletes: await deleteValidations(ids, 'pre-capture cleanup'),
  };
}

const functionLookup = await capture(functionLookupDocument);
assertNoTopLevelErrors(functionLookup, 'shopifyFunctions lookup');
const functionNodes = readPath(functionLookup.response.payload, ['data', 'shopifyFunctions', 'nodes']);
const validationFunction = readArray(functionNodes)
  .map(readRecord)
  .find((node): node is JsonRecord => node?.['handle'] === validationFunctionHandle);
if (!validationFunction) {
  throw new Error(`Missing released validation Function handle ${validationFunctionHandle}`);
}
readString(validationFunction['title'], 'validation Function title');

const preCleanup = await cleanupExistingValidations();
let createdValidationIds: string[] = [];

try {
  const createTitleFallback = await capture(validationCreateDocument, {
    functionHandle: validationFunctionHandle,
  });
  assertNoTopLevelErrors(createTitleFallback, 'validationCreate title fallback');

  const omittedPayload = readValidationCreatePayload(createTitleFallback, 'omitted');
  const explicitNullPayload = readValidationCreatePayload(createTitleFallback, 'explicitNull');
  const emptyStringPayload = readValidationCreatePayload(createTitleFallback, 'emptyString');
  assertEmptyUserErrors(omittedPayload, 'omitted validationCreate');
  assertEmptyUserErrors(explicitNullPayload, 'explicitNull validationCreate');
  assertEmptyUserErrors(emptyStringPayload, 'emptyString validationCreate');

  const omitted = readValidationRecord(omittedPayload, 'omitted validationCreate');
  const explicitNull = readValidationRecord(explicitNullPayload, 'explicitNull validationCreate');
  const emptyString = readValidationRecord(emptyStringPayload, 'emptyString validationCreate');
  createdValidationIds = [validationId(omitted), validationId(explicitNull), validationId(emptyString)];
  const observedFallbackTitle = readString(omitted['title'], 'omitted validationCreate title');
  assertTitle(explicitNull['title'], observedFallbackTitle, 'explicitNull validationCreate');
  assertTitle(emptyString['title'], '', 'emptyString validationCreate');

  const postCreateValidationRead = await capture(validationReadDocument, {
    id: createdValidationIds[0],
  });
  assertNoTopLevelErrors(postCreateValidationRead, 'post-create validation(id:) read');
  assertTitle(
    readPath(postCreateValidationRead.response.payload, ['data', 'validation', 'title']),
    observedFallbackTitle,
    'post-create validation(id:) read',
  );

  const postCreateValidationsRead = await capture(validationsReadDocument);
  assertNoTopLevelErrors(postCreateValidationsRead, 'post-create validations read');
  const connectionTitles = validationNodes(postCreateValidationsRead).map((node) => node['title']);
  const expectedTitles = [observedFallbackTitle, observedFallbackTitle, ''];
  if (JSON.stringify(connectionTitles) !== JSON.stringify(expectedTitles)) {
    throw new Error(
      `post-create validations(first: 3) titles mismatch: expected ${JSON.stringify(expectedTitles)}, got ${JSON.stringify(connectionTitles)}`,
    );
  }

  const cleanup = await deleteValidations(createdValidationIds, 'post-capture cleanup');
  createdValidationIds = [];

  const fixture = {
    scenarioId: 'functions-validation-create-title-fallback',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify',
    storeDomain,
    apiVersion,
    summary:
      'Live validationCreate evidence for title fallback to the resolved ShopifyFunction title when title input is omitted or explicitly null.',
    conformanceApp: {
      validationFunctionHandle,
      validationFunction,
      observedFallbackTitle,
    },
    functionLookup,
    preCleanup,
    createTitleFallback,
    postCreateValidationRead,
    postCreateValidationsRead,
    cleanup,
    upstreamCalls: [
      {
        operationName: 'FunctionHydrateByHandle',
        variables: {
          handle: validationFunctionHandle,
          apiType: 'VALIDATION',
        },
        query: 'cassette-backed VALIDATION ShopifyFunction lookup captured from the live conformance app',
        response: {
          status: 200,
          body: {
            data: {
              shopifyFunctions: {
                nodes: [
                  {
                    ...validationFunction,
                    title: observedFallbackTitle,
                  },
                ],
              },
            },
          },
        },
      },
    ],
    notes: {
      lifecycle:
        'The script deletes disposable validations before capture, creates omitted-title/null-title/empty-string-title validations, verifies mutation payload titles and downstream reads, then deletes the created validations.',
      titleFallback:
        'Shopify persisted the same Function-derived fallback for omitted and explicit null title input. The current conformance extension stores raw name t:name while shopifyFunctions.title returns the localized name, so the cassette title uses the mutation-observed fallback for local replay. An explicit empty string stayed empty.',
    },
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (createdValidationIds.length > 0) {
    try {
      await deleteValidations(createdValidationIds, 'failure cleanup');
    } catch (error) {
      console.error(`Cleanup failed after capture error: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}
