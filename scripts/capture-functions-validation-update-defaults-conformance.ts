/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const validationFunctionHandle = 'conformance-validation';
const missingValidationId = 'gid://shopify/Validation/999999999999';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'functions');
const outputPath = path.join(outputDir, 'functions-validation-update-defaults.json');
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

async function capture(query: string, variables: Record<string, unknown>): Promise<Capture> {
  return {
    query,
    variables,
    response: await runGraphqlRequest(query, variables),
  };
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

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
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

function assertValidationDefaults(value: unknown, context: string): void {
  const validation = readRecord(value)?.['validation'];
  const record = readRecord(validation);
  if (!record || record['enabled'] !== false || record['blockOnFailure'] !== false) {
    throw new Error(`${context} did not reset enabled/blockOnFailure: ${JSON.stringify(value, null, 2)}`);
  }
}

function assertUnknownIdError(value: unknown): void {
  const record = readRecord(value);
  const userErrors = readUserErrors(value);
  const first = readRecord(userErrors[0]);
  const field = Array.isArray(first?.['field']) ? first['field'] : [];
  if (
    record?.['validation'] !== null ||
    userErrors.length !== 1 ||
    first?.['code'] !== 'NOT_FOUND' ||
    first?.['message'] !== 'Extension not found.' ||
    field.length !== 1 ||
    field[0] !== 'id'
  ) {
    throw new Error(`validationUpdate unknown-id shape mismatch: ${JSON.stringify(value, null, 2)}`);
  }
}

const functionLookupDocument = `#graphql
  query ValidationUpdateDefaultsFunctionLookup {
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

const validationCreateDocument = `mutation ValidationUpdateDefaultsSetup($validation: ValidationCreateInput!) {
  validationCreate(validation: $validation) {
    validation {
      id
      title
      enabled
      blockOnFailure
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const validationUpdateDocument = `mutation ValidationUpdateOmittedDefaults($id: ID!, $validation: ValidationUpdateInput!) {
  validationUpdate(id: $id, validation: $validation) {
    validation {
      id
      title
      enabled
      blockOnFailure
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const validationReadDocument = `query ValidationUpdateDefaultsRead($id: ID!) {
  validation(id: $id) {
    id
    title
    enabled
    blockOnFailure
  }
}
`;

const validationUnknownIdDocument = `mutation ValidationUpdateUnknownId($id: ID!, $validation: ValidationUpdateInput!) {
  validationUpdate(id: $id, validation: $validation) {
    validation {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const validationDeleteDocument = `mutation ValidationUpdateDefaultsCleanup($id: ID!) {
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

const functionLookup = await capture(functionLookupDocument, {});
assertNoTopLevelErrors(functionLookup, 'shopifyFunctions lookup');
const functionNodes = readPath(functionLookup.response.payload, ['data', 'shopifyFunctions', 'nodes']);
const validationFunction = Array.isArray(functionNodes)
  ? functionNodes.map(readRecord).find((node) => node?.['handle'] === validationFunctionHandle)
  : undefined;
if (!validationFunction) {
  throw new Error(`Missing released validation Function handle ${validationFunctionHandle}`);
}

let createdValidationId: string | null = null;
let cleanup: Capture | null = null;

try {
  const createActive = await capture(validationCreateDocument, {
    validation: {
      functionHandle: validationFunctionHandle,
      title: 'HAR-778 validation update defaults',
      enable: true,
      blockOnFailure: true,
    },
  });
  assertNoTopLevelErrors(createActive, 'validationCreate setup');
  const createPayload = readPath(createActive.response.payload, ['data', 'validationCreate']);
  assertEmptyUserErrors(createPayload, 'validationCreate setup');
  const createdValidation = readRecord(readRecord(createPayload)?.['validation']);
  const createdId = typeof createdValidation?.['id'] === 'string' ? createdValidation['id'] : null;
  if (!createdId) {
    throw new Error(`validationCreate did not return an id: ${JSON.stringify(createActive.response, null, 2)}`);
  }
  createdValidationId = createdId;

  const updateTitleOnly = await capture(validationUpdateDocument, {
    id: createdId,
    validation: {
      title: 'HAR-778 validation update renamed',
    },
  });
  assertNoTopLevelErrors(updateTitleOnly, 'validationUpdate title-only');
  const updatePayload = readPath(updateTitleOnly.response.payload, ['data', 'validationUpdate']);
  assertEmptyUserErrors(updatePayload, 'validationUpdate title-only');
  assertValidationDefaults(updatePayload, 'validationUpdate title-only');

  const postUpdateRead = await capture(validationReadDocument, { id: createdId });
  assertNoTopLevelErrors(postUpdateRead, 'validation post-update read');
  const readPayload = {
    validation: readPath(postUpdateRead.response.payload, ['data', 'validation']),
  };
  assertValidationDefaults(readPayload, 'validation post-update read');

  const unknownId = await capture(validationUnknownIdDocument, {
    id: missingValidationId,
    validation: {},
  });
  assertNoTopLevelErrors(unknownId, 'validationUpdate unknown-id');
  const unknownPayload = readPath(unknownId.response.payload, ['data', 'validationUpdate']);
  assertUnknownIdError(unknownPayload);

  cleanup = await capture(validationDeleteDocument, { id: createdId });
  assertNoTopLevelErrors(cleanup, 'validationDelete cleanup');
  const cleanupPayload = readPath(cleanup.response.payload, ['data', 'validationDelete']);
  assertEmptyUserErrors(cleanupPayload, 'validationDelete cleanup');
  createdValidationId = null;

  const fixture = {
    scenarioId: 'functions-validation-update-defaults',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify',
    storeDomain,
    apiVersion,
    summary:
      'HAR-778 live validationUpdate evidence for omitted enable input/defaults, enabled output, blockOnFailure defaults, and unknown-id userError shape.',
    conformanceApp: {
      validationFunctionHandle,
      validationFunction,
    },
    functionLookup,
    createActive,
    updateTitleOnly,
    postUpdateRead,
    unknownId,
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
                nodes: [validationFunction],
              },
            },
          },
        },
      },
    ],
    notes: {
      lifecycle:
        'The script creates one disposable active validation, updates it with title only, verifies enabled and blockOnFailure reset to false in the mutation payload and read-after-write query, then deletes it.',
      unknownId:
        'The unknown-id branch records Shopify userErrors with code NOT_FOUND, field ["id"], and message "Extension not found.".',
    },
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (createdValidationId) {
    const cleanupAfterFailure = await capture(validationDeleteDocument, { id: createdValidationId });
    if (
      cleanupAfterFailure.response.status < 200 ||
      cleanupAfterFailure.response.status >= 300 ||
      readRecord(cleanupAfterFailure.response.payload)?.['errors']
    ) {
      console.error(`Cleanup failed for ${createdValidationId}: ${JSON.stringify(cleanupAfterFailure, null, 2)}`);
    }
  }
}
