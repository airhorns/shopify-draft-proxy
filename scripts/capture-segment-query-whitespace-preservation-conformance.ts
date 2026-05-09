/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedCase = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segment-query-whitespace-preservation.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertGraphqlOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let cursor = value;
  for (const segment of pathSegments) {
    if (!cursor || typeof cursor !== 'object') return undefined;
    cursor = (cursor as Record<string, unknown>)[segment];
  }
  return cursor;
}

function readRequiredString(result: ConformanceGraphqlResult, pathSegments: string[], context: string): string {
  const value = readPath(result.payload, pathSegments);
  if (typeof value !== 'string') {
    throw new Error(`${context} did not return ${pathSegments.join('.')}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return value;
}

function readUserErrors(result: ConformanceGraphqlResult, root: string): unknown[] {
  const value = readPath(result.payload, ['data', root, 'userErrors']);
  if (!Array.isArray(value)) {
    throw new Error(`${root}.userErrors missing from response: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return value;
}

function assertNoUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  const userErrors = readUserErrors(result, root);
  if (userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertQueryEquals(
  result: ConformanceGraphqlResult,
  pathSegments: string[],
  expected: string,
  context: string,
): void {
  const actual = readRequiredString(result, pathSegments, context);
  if (actual !== expected) {
    throw new Error(`${context} expected query ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

async function captureCase(
  cases: CapturedCase[],
  name: string,
  query: string,
  variables: Record<string, unknown>,
  assert: (result: ConformanceGraphqlResult) => void,
): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlRequest(query, variables);
  assertGraphqlOk(response, name);
  assert(response);
  cases.push({
    name,
    request: { query, variables },
    response,
  });
  return response;
}

const segmentFields = `
  id
  name
  query
  creationDate
  lastEditDate
`;

const createMutation = `#graphql
  mutation SegmentQueryWhitespaceCreate($name: String!, $query: String!) {
    segmentCreate(name: $name, query: $query) {
      segment {
        ${segmentFields}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const readQuery = `#graphql
  query SegmentQueryWhitespaceRead($id: ID!) {
    segment(id: $id) {
      ${segmentFields}
    }
  }
`;

const updateMutation = `#graphql
  mutation SegmentQueryWhitespaceUpdate($id: ID!, $query: String) {
    segmentUpdate(id: $id, query: $query) {
      segment {
        ${segmentFields}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation SegmentQueryWhitespaceCleanup($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

const marker = `query-whitespace-${Date.now()}`;
const createQuery = '   number_of_orders = 0   ';
const updateQuery = '   number_of_orders > 0   ';
const cases: CapturedCase[] = [];
let createdSegmentId: string | null = null;

try {
  const create = await captureCase(
    cases,
    'segmentCreatePreservesQueryWhitespace',
    createMutation,
    {
      name: `Query whitespace ${marker}`,
      query: createQuery,
    },
    (result) => {
      assertNoUserErrors(result, 'segmentCreate', 'segmentCreate query whitespace');
      assertQueryEquals(
        result,
        ['data', 'segmentCreate', 'segment', 'query'],
        createQuery,
        'segmentCreate query whitespace',
      );
    },
  );
  createdSegmentId = readRequiredString(create, ['data', 'segmentCreate', 'segment', 'id'], 'segmentCreate id');

  await captureCase(
    cases,
    'segmentReadPreservesCreatedQueryWhitespace',
    readQuery,
    { id: createdSegmentId },
    (result) => assertQueryEquals(result, ['data', 'segment', 'query'], createQuery, 'segment read query whitespace'),
  );

  await captureCase(
    cases,
    'segmentUpdatePreservesQueryWhitespace',
    updateMutation,
    {
      id: createdSegmentId,
      query: updateQuery,
    },
    (result) => {
      assertNoUserErrors(result, 'segmentUpdate', 'segmentUpdate query whitespace');
      assertQueryEquals(
        result,
        ['data', 'segmentUpdate', 'segment', 'query'],
        updateQuery,
        'segmentUpdate query whitespace',
      );
    },
  );
} finally {
  if (createdSegmentId) {
    const cleanup = await runGraphqlRequest(deleteMutation, { id: createdSegmentId });
    assertGraphqlOk(cleanup, 'segmentDelete query whitespace cleanup');
    cases.push({
      name: 'segmentDeleteQueryWhitespaceCleanup',
      request: {
        query: deleteMutation,
        variables: { id: createdSegmentId },
      },
      response: cleanup,
    });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      cases,
      notes: [
        'Live Shopify evidence that segmentCreate and segmentUpdate preserve leading and trailing query whitespace when storing and returning Segment.query.',
        'The script creates one disposable live segment, reads it back, updates its query with a different padded string, and deletes it during cleanup.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
