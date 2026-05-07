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
const outputPath = path.join(outputDir, 'functions-validation-metafields-input-validation.json');
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

type FunctionNode = {
  id: string;
  title: string | null;
  handle: string | null;
  apiType: string | null;
  description: string | null;
  appKey: string | null;
  app: {
    __typename: string | null;
    id: string | null;
    title: string | null;
    handle: string | null;
    apiKey: string | null;
  } | null;
};

async function capture(query: string, variables: Record<string, unknown>): Promise<Capture> {
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
  return readArray(readRecord(value)?.['userErrors']);
}

function assertEmptyUserErrors(value: unknown, context: string): void {
  const userErrors = readUserErrors(value);
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertRejectedPayload(value: unknown, context: string): void {
  const record = readRecord(value);
  const userErrors = readUserErrors(value);
  if (record?.['validation'] !== null || userErrors.length === 0) {
    throw new Error(`${context} did not return validation null plus userErrors: ${JSON.stringify(value, null, 2)}`);
  }
}

function validationIdsFromBatch(value: unknown): string[] {
  const data = readRecord(value);
  if (!data) {
    return [];
  }
  return Object.values(data)
    .map((payload) => readRecord(readRecord(payload)?.['validation'])?.['id'])
    .filter((id): id is string => typeof id === 'string');
}

const functionLookupDocument = `#graphql
  query ValidationMetafieldsInputValidationFunctionLookup {
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

const validationCreateDocument = `mutation ValidationMetafieldsSetup($validation: ValidationCreateInput!) {
  validationCreate(validation: $validation) {
    validation {
      id
      title
      metafields(first: 5) {
        nodes {
          namespace
          key
          type
          value
        }
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const invalidCreateBatchDocument = `mutation ValidationMetafieldsInvalidCreate(
  $mixedMissingKey: ValidationCreateInput!
  $missingType: ValidationCreateInput!
  $blankType: ValidationCreateInput!
  $missingValue: ValidationCreateInput!
  $blankValue: ValidationCreateInput!
  $invalidType: ValidationCreateInput!
  $reservedShopify: ValidationCreateInput!
  $invalidValue: ValidationCreateInput!
) {
  mixedMissingKey: validationCreate(validation: $mixedMissingKey) {
    validation {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
  missingType: validationCreate(validation: $missingType) {
    validation {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
  blankType: validationCreate(validation: $blankType) {
    validation {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
  missingValue: validationCreate(validation: $missingValue) {
    validation {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
  blankValue: validationCreate(validation: $blankValue) {
    validation {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
  invalidType: validationCreate(validation: $invalidType) {
    validation {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
  reservedShopify: validationCreate(validation: $reservedShopify) {
    validation {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
  invalidValue: validationCreate(validation: $invalidValue) {
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

const validationUpdateDocument = `mutation ValidationMetafieldsInvalidUpdate(
  $id: ID!
  $validation: ValidationUpdateInput!
) {
  validationUpdate(id: $id, validation: $validation) {
    validation {
      id
      metafields(first: 5) {
        nodes {
          namespace
          key
          type
          value
        }
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}
`;

const validationReadDocument = `query ValidationMetafieldsPostRejectedUpdateRead($id: ID!) {
  validation(id: $id) {
    title
    metafields(first: 5) {
      nodes {
        namespace
        key
        type
        value
      }
    }
  }
}
`;

const validationDeleteDocument = `mutation ValidationMetafieldsCleanup($id: ID!) {
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
const functionNodes = readArray(readPath(functionLookup.response.payload, ['data', 'shopifyFunctions', 'nodes']));
const validationFunction = functionNodes
  .map((node) => readRecord(node) as FunctionNode | null)
  .find((node) => node?.handle === validationFunctionHandle);
if (!validationFunction) {
  throw new Error(`Missing released validation Function handle ${validationFunctionHandle}`);
}

const runId = Date.now().toString(36);
const baseValidation = {
  functionHandle: validationFunctionHandle,
  title: `validation metafields input ${runId}`,
};
let setupValidationId: string | null = null;
let cleanup: Capture | null = null;
const unexpectedCreateIds: string[] = [];

try {
  const createSetup = await capture(validationCreateDocument, {
    validation: {
      ...baseValidation,
      metafields: [{ namespace: 'custom', key: 'mode', type: 'single_line_text_field', value: 'strict' }],
    },
  });
  assertNoTopLevelErrors(createSetup, 'validationCreate setup');
  const createPayload = readPath(createSetup.response.payload, ['data', 'validationCreate']);
  assertEmptyUserErrors(createPayload, 'validationCreate setup');
  const createdId = readRecord(readRecord(createPayload)?.['validation'])?.['id'];
  if (typeof createdId !== 'string') {
    throw new Error(`validationCreate setup did not return an id: ${JSON.stringify(createSetup.response, null, 2)}`);
  }
  setupValidationId = createdId;

  const invalidUpdate = await capture(validationUpdateDocument, {
    id: setupValidationId,
    validation: {
      metafields: [{ namespace: 'custom', type: 'single_line_text_field', value: 'loose' }],
    },
  });
  assertNoTopLevelErrors(invalidUpdate, 'validationUpdate invalid metafield');
  assertRejectedPayload(
    readPath(invalidUpdate.response.payload, ['data', 'validationUpdate']),
    'validationUpdate invalid metafield',
  );

  const postRejectedUpdateRead = await capture(validationReadDocument, { id: setupValidationId });
  assertNoTopLevelErrors(postRejectedUpdateRead, 'validation post-rejected-update read');

  const invalidCreateBatch = await capture(invalidCreateBatchDocument, {
    mixedMissingKey: {
      ...baseValidation,
      metafields: [
        { namespace: 'custom', key: 'mode', type: 'single_line_text_field', value: 'strict' },
        { namespace: 'custom', type: 'single_line_text_field', value: 'v' },
      ],
    },
    missingType: {
      ...baseValidation,
      metafields: [{ namespace: 'custom', key: 'mode', value: 'v' }],
    },
    blankType: {
      ...baseValidation,
      metafields: [{ namespace: 'custom', key: 'mode', type: '', value: 'v' }],
    },
    missingValue: {
      ...baseValidation,
      metafields: [{ namespace: 'custom', key: 'mode', type: 'single_line_text_field' }],
    },
    blankValue: {
      ...baseValidation,
      metafields: [{ namespace: 'custom', key: 'mode', type: 'single_line_text_field', value: '' }],
    },
    invalidType: {
      ...baseValidation,
      metafields: [{ namespace: 'custom', key: 'mode', type: 'bogus_type', value: 'v' }],
    },
    reservedShopify: {
      ...baseValidation,
      metafields: [{ namespace: 'shopify', key: 'mode', type: 'single_line_text_field', value: 'v' }],
    },
    invalidValue: {
      ...baseValidation,
      metafields: [{ namespace: 'custom', key: 'count', type: 'number_integer', value: 'not a number' }],
    },
  });
  const invalidCreateData = readPath(invalidCreateBatch.response.payload, ['data']);
  unexpectedCreateIds.push(...validationIdsFromBatch(invalidCreateData));
  assertNoTopLevelErrors(invalidCreateBatch, 'validationCreate invalid metafields');
  for (const [alias, payload] of Object.entries(readRecord(invalidCreateData) ?? {})) {
    assertRejectedPayload(payload, `validationCreate ${alias}`);
  }

  cleanup = await capture(validationDeleteDocument, { id: setupValidationId });
  assertNoTopLevelErrors(cleanup, 'validationDelete cleanup');
  assertEmptyUserErrors(readPath(cleanup.response.payload, ['data', 'validationDelete']), 'validationDelete cleanup');
  setupValidationId = null;

  const fixture = {
    scenarioId: 'functions-validation-metafields-input-validation',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify-and-cassette-backed-local-runtime',
    storeDomain,
    apiVersion,
    summary:
      'validationCreate and validationUpdate metafield input rejection branches with per-index userErrors and atomic no-write behavior.',
    conformanceApp: {
      validationFunctionHandle,
      validationFunction,
    },
    functionLookup,
    createSetup,
    invalidUpdate,
    postRejectedUpdateRead,
    invalidCreateBatch,
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
        'The script creates one disposable validation with a valid metafield, rejects an invalid validationUpdate metafield payload, verifies the original metafield is still readable, records validationCreate rejection branches, and deletes the disposable validation.',
      localRuntimeEvidence:
        'The upstream cassette contains only the ShopifyFunction lookup needed for local validationCreate staging; supported mutations stay local-only under the proxy.',
    },
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  const cleanupIds = [...unexpectedCreateIds, ...(setupValidationId ? [setupValidationId] : [])];
  for (const id of cleanupIds) {
    const cleanupAfterFailure = await capture(validationDeleteDocument, { id });
    if (
      cleanupAfterFailure.response.status < 200 ||
      cleanupAfterFailure.response.status >= 300 ||
      readRecord(cleanupAfterFailure.response.payload)?.['errors']
    ) {
      console.error(`Cleanup failed for ${id}: ${JSON.stringify(cleanupAfterFailure, null, 2)}`);
    }
  }
}
