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
const outputPath = path.join(outputDir, 'functions-validation-update-metafields-upsert.json');
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

type MetafieldExpectation = {
  namespace: string;
  key: string;
  type: string;
  value: string;
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

function assertNoTopLevelErrors(captureResult: Capture, context: string): void {
  if (
    captureResult.response.status < 200 ||
    captureResult.response.status >= 300 ||
    readRecord(captureResult.response.payload)?.['errors']
  ) {
    throw new Error(`${context} failed: ${JSON.stringify(captureResult.response, null, 2)}`);
  }
}

function assertEmptyUserErrors(value: unknown, context: string): void {
  const userErrors = readUserErrors(value);
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertMetafields(value: unknown, expected: MetafieldExpectation[], context: string): void {
  const validation = readRecord(value);
  const nodes = readRecord(validation?.['metafields'])?.['nodes'];
  if (!Array.isArray(nodes)) {
    throw new Error(`${context} did not return metafield nodes: ${JSON.stringify(value, null, 2)}`);
  }

  const actual = nodes.map((node) => {
    const row = readRecord(node);
    return {
      namespace: row?.['namespace'],
      key: row?.['key'],
      type: row?.['type'],
      value: row?.['value'],
      updatedAt: row?.['updatedAt'],
    };
  });

  if (
    actual.length !== expected.length ||
    expected.some((expectedRow, index) => {
      const actualRow = actual[index];
      return (
        actualRow?.namespace !== expectedRow.namespace ||
        actualRow.key !== expectedRow.key ||
        actualRow.type !== expectedRow.type ||
        actualRow.value !== expectedRow.value ||
        typeof actualRow.updatedAt !== 'string'
      );
    })
  ) {
    throw new Error(
      `${context} metafield rows mismatch:\nexpected ${JSON.stringify(expected, null, 2)}\nactual ${JSON.stringify(
        actual,
        null,
        2,
      )}`,
    );
  }
}

function assertValidationPayload(value: unknown, expected: MetafieldExpectation[], context: string): string {
  const payload = readRecord(value);
  assertEmptyUserErrors(payload, context);
  const validation = readRecord(payload?.['validation']);
  const id = typeof validation?.['id'] === 'string' ? validation['id'] : null;
  if (!id) {
    throw new Error(`${context} did not return a validation id: ${JSON.stringify(value, null, 2)}`);
  }
  assertMetafields(validation, expected, context);
  return id;
}

function assertValidationRead(value: unknown, expected: MetafieldExpectation[], context: string): void {
  const validation = readRecord(value);
  if (!validation || typeof validation['id'] !== 'string') {
    throw new Error(`${context} did not return validation read data: ${JSON.stringify(value, null, 2)}`);
  }
  assertMetafields(validation, expected, context);
}

const initialModeMetafield: MetafieldExpectation = {
  namespace: 'custom',
  key: 'mode',
  type: 'single_line_text_field',
  value: 'strict',
};
const initialColorMetafield: MetafieldExpectation = {
  namespace: 'custom',
  key: 'color',
  type: 'single_line_text_field',
  value: 'blue',
};
const partialModeMetafield: MetafieldExpectation = {
  namespace: 'custom',
  key: 'mode',
  type: 'single_line_text_field',
  value: 'relaxed',
};
const partialSizeMetafield: MetafieldExpectation = {
  namespace: 'custom',
  key: 'size',
  type: 'single_line_text_field',
  value: 'large',
};

const initialMetafields: MetafieldExpectation[] = [initialModeMetafield, initialColorMetafield];
const partialMetafields: MetafieldExpectation[] = [partialModeMetafield, initialColorMetafield, partialSizeMetafield];

const functionLookupDocument = `#graphql
  query ValidationUpdateMetafieldsUpsertFunctionLookup {
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

const validationCreateDocument = `mutation ValidationUpdateMetafieldsUpsertSetup($validation: ValidationCreateInput!) {
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
          updatedAt
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

const validationUpdateDocument = `mutation ValidationUpdateMetafieldsUpsert($id: ID!, $validation: ValidationUpdateInput!) {
  validationUpdate(id: $id, validation: $validation) {
    validation {
      id
      title
      metafields(first: 5) {
        nodes {
          namespace
          key
          type
          value
          updatedAt
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

const validationReadDocument = `query ValidationUpdateMetafieldsUpsertRead($id: ID!) {
  validation(id: $id) {
    id
    title
    metafields(first: 5) {
      nodes {
        namespace
        key
        type
        value
        updatedAt
      }
    }
  }
}
`;

const validationDeleteDocument = `mutation ValidationUpdateMetafieldsUpsertCleanup($id: ID!) {
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
  const createInitial = await capture(validationCreateDocument, {
    validation: {
      functionHandle: validationFunctionHandle,
      title: 'Validation metafields upsert original',
      metafields: initialMetafields,
    },
  });
  assertNoTopLevelErrors(createInitial, 'validationCreate setup');
  const createPayload = readPath(createInitial.response.payload, ['data', 'validationCreate']);
  createdValidationId = assertValidationPayload(createPayload, initialMetafields, 'validationCreate setup');

  const updateTitleOnly = await capture(validationUpdateDocument, {
    id: createdValidationId,
    validation: {
      title: 'Validation metafields upsert renamed',
    },
  });
  assertNoTopLevelErrors(updateTitleOnly, 'validationUpdate title-only');
  assertValidationPayload(
    readPath(updateTitleOnly.response.payload, ['data', 'validationUpdate']),
    initialMetafields,
    'validationUpdate title-only',
  );

  const readAfterTitleOnly = await capture(validationReadDocument, { id: createdValidationId });
  assertNoTopLevelErrors(readAfterTitleOnly, 'validation read after title-only');
  assertValidationRead(
    readPath(readAfterTitleOnly.response.payload, ['data', 'validation']),
    initialMetafields,
    'validation read after title-only',
  );

  const updateEmpty = await capture(validationUpdateDocument, {
    id: createdValidationId,
    validation: {
      metafields: [],
    },
  });
  assertNoTopLevelErrors(updateEmpty, 'validationUpdate empty metafields');
  assertValidationPayload(
    readPath(updateEmpty.response.payload, ['data', 'validationUpdate']),
    initialMetafields,
    'validationUpdate empty metafields',
  );

  const readAfterEmpty = await capture(validationReadDocument, { id: createdValidationId });
  assertNoTopLevelErrors(readAfterEmpty, 'validation read after empty metafields');
  assertValidationRead(
    readPath(readAfterEmpty.response.payload, ['data', 'validation']),
    initialMetafields,
    'validation read after empty metafields',
  );

  const updatePartial = await capture(validationUpdateDocument, {
    id: createdValidationId,
    validation: {
      metafields: [partialModeMetafield, partialSizeMetafield],
    },
  });
  assertNoTopLevelErrors(updatePartial, 'validationUpdate partial metafields');
  assertValidationPayload(
    readPath(updatePartial.response.payload, ['data', 'validationUpdate']),
    partialMetafields,
    'validationUpdate partial metafields',
  );

  const readAfterPartial = await capture(validationReadDocument, { id: createdValidationId });
  assertNoTopLevelErrors(readAfterPartial, 'validation read after partial metafields');
  assertValidationRead(
    readPath(readAfterPartial.response.payload, ['data', 'validation']),
    partialMetafields,
    'validation read after partial metafields',
  );

  cleanup = await capture(validationDeleteDocument, { id: createdValidationId });
  assertNoTopLevelErrors(cleanup, 'validationDelete cleanup');
  assertEmptyUserErrors(readPath(cleanup.response.payload, ['data', 'validationDelete']), 'validationDelete cleanup');

  const output = {
    scenarioId: 'functions-validation-update-metafields-upsert',
    capturedAt: new Date().toISOString(),
    source: 'live-shopify',
    storeDomain,
    apiVersion,
    summary:
      'Live validationUpdate evidence that omitted and empty metafields inputs preserve existing Validation metafields, while non-empty inputs upsert by namespace/key and retain unrelated rows.',
    conformanceApp: {
      validationFunctionHandle,
      validationFunction,
    },
    functionLookup,
    createInitial,
    updateTitleOnly,
    readAfterTitleOnly,
    updateEmpty,
    readAfterEmpty,
    updatePartial,
    readAfterPartial,
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
        'The script creates one disposable validation with two metafields, updates it with title-only input, explicit empty metafields input, and a partial non-empty metafields input, reads after each update, then deletes the disposable validation.',
      ordering:
        'The capture asserts stable creation order: the matching namespace/key row stays in its original position, unrelated existing rows remain, and newly inserted rows append after existing rows.',
    },
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  if (createdValidationId && !cleanup) {
    try {
      cleanup = await capture(validationDeleteDocument, { id: createdValidationId });
      console.error(`Cleanup attempted for ${createdValidationId}: ${JSON.stringify(cleanup.response, null, 2)}`);
    } catch (cleanupError) {
      console.error(`Cleanup failed for ${createdValidationId}: ${cleanupError}`);
    }
  }
  throw error;
}
