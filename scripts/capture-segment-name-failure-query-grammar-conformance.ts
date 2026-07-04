/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
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

type ExpectedUserError = {
  __typename: 'UserError';
  field: string[] | null;
  message: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segment-create-name-failure-short-circuits-query-grammar.json');
const createMutation = await readFile(
  'config/parity-requests/segments/segments-user-errors-shape-segment-create.graphql',
  'utf8',
);
const updateMutation = await readFile(
  'config/parity-requests/segments/segment-update-name-query-validation-order.graphql',
  'utf8',
);
const deleteMutation = await readFile(
  'config/parity-requests/segments/segments-user-errors-shape-segment-delete.graphql',
  'utf8',
);
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

function payloadFor(result: ConformanceGraphqlResult, root: string): Record<string, unknown> {
  const data = result.payload.data as Record<string, unknown> | undefined;
  const payload = data?.[root];
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) {
    throw new Error(`${root} did not return an object payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return payload as Record<string, unknown>;
}

function readCreatedSegmentId(result: ConformanceGraphqlResult): string {
  const segment = payloadFor(result, 'segmentCreate')['segment'] as Record<string, unknown> | null;
  const id = segment?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`segmentCreate did not return a segment id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

function assertUserErrors(
  result: ConformanceGraphqlResult,
  root: 'segmentCreate' | 'segmentUpdate',
  expected: ExpectedUserError[],
): void {
  const payload = payloadFor(result, root);
  if (payload['segment'] !== null) {
    throw new Error(`${root} expected segment:null: ${JSON.stringify(payload, null, 2)}`);
  }
  const actual = payload['userErrors'];
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${root} userErrors mismatch: ${JSON.stringify(actual, null, 2)}`);
  }
}

async function captureCase(
  cases: CapturedCase[],
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlRequest(query, variables);
  assertGraphqlOk(response, name);
  cases.push({
    name,
    request: { query, variables },
    response,
  });
  return response;
}

const invalidQuery = 'not a valid segment query ???';
const cases: CapturedCase[] = [];
let createdSegmentId: string | null = null;

try {
  const blankNameInvalidQuery = await captureCase(cases, 'segmentCreateBlankNameInvalidQuery', createMutation, {
    name: '',
    query: invalidQuery,
  });
  assertUserErrors(blankNameInvalidQuery, 'segmentCreate', [
    { __typename: 'UserError', field: ['name'], message: "Name can't be blank" },
  ]);

  const longNameInvalidQuery = await captureCase(cases, 'segmentCreateLongNameInvalidQuery', createMutation, {
    name: 'a'.repeat(256),
    query: invalidQuery,
  });
  assertUserErrors(longNameInvalidQuery, 'segmentCreate', [
    { __typename: 'UserError', field: ['name'], message: 'Name is too long (maximum is 255 characters)' },
  ]);

  const blankNameAndQuery = await captureCase(cases, 'segmentCreateBlankNameAndQuery', createMutation, {
    name: '',
    query: '',
  });
  assertUserErrors(blankNameAndQuery, 'segmentCreate', [
    { __typename: 'UserError', field: ['name'], message: "Name can't be blank" },
    { __typename: 'UserError', field: ['query'], message: "Query can't be blank" },
  ]);

  const setup = await captureCase(cases, 'segmentUpdateSetup', createMutation, {
    name: `Segment validation order ${Date.now()}`,
    query: 'number_of_orders >= 1',
  });
  createdSegmentId = readCreatedSegmentId(setup);

  const updateBlankNameInvalidQuery = await captureCase(cases, 'segmentUpdateBlankNameInvalidQuery', updateMutation, {
    id: createdSegmentId,
    name: '',
    query: invalidQuery,
  });
  assertUserErrors(updateBlankNameInvalidQuery, 'segmentUpdate', [
    { __typename: 'UserError', field: ['name'], message: "Name can't be blank" },
  ]);
} finally {
  if (createdSegmentId) {
    const cleanup = await runGraphqlRequest(deleteMutation, { id: createdSegmentId });
    assertGraphqlOk(cleanup, 'segmentDelete cleanup');
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
        'Live Shopify evidence that segmentCreate and segmentUpdate perform Change-level name/query presence and length validation before CDP query grammar validation.',
        'When name fails presence or length validation, Shopify returns only the name userError for grammatically invalid query text.',
        'Query presence and length remain Change-level errors and can be emitted alongside name validation errors.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
