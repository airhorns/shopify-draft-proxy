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
const outputPath = path.join(outputDir, 'segment-create-update-length-edge-cases.json');
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

function assertNoUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  assertGraphqlOk(result, context);
  const data = result.payload.data as Record<string, unknown> | undefined;
  const payload = data?.[root] as Record<string, unknown> | undefined;
  const userErrors = payload?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  assertGraphqlOk(result, context);
  const data = result.payload.data as Record<string, unknown> | undefined;
  const payload = data?.[root] as Record<string, unknown> | undefined;
  const userErrors = payload?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`${context} did not return userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function readCreatedSegmentId(result: ConformanceGraphqlResult): string {
  const data = result.payload.data as Record<string, unknown> | undefined;
  const create = data?.['segmentCreate'] as Record<string, unknown> | undefined;
  const segment = create?.['segment'] as Record<string, unknown> | undefined;
  const id = segment?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`segmentCreate did not return a segment id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

const segmentFields = `
  id
  name
  query
  creationDate
  lastEditDate
`;

const createMutation = `#graphql
  mutation SegmentCreateValidationLimits($name: String!, $query: String!) {
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

const updateNameMutation = `#graphql
  mutation SegmentUpdateNameValidationLimits($id: ID!, $name: String) {
    segmentUpdate(id: $id, name: $name) {
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

const updateQueryMutation = `#graphql
  mutation SegmentUpdateQueryValidationLimits($id: ID!, $query: String) {
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
  mutation SegmentDeleteLengthEdgeCasesCleanup($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

const marker = `length-edge-${Date.now()}`;
const createNormalizedName = `Create ${marker}`;
const updateNormalizedName = `Update ${marker}`;
const paddedCreateName = `${' '.repeat(250)}${createNormalizedName}${' '.repeat(10)}`;
const paddedUpdateName = `${' '.repeat(256)}${updateNormalizedName}`;
const rawOverLimitQuery = `${' '.repeat(4000)}number_of_orders = 0${' '.repeat(1500)}`;
const cases: CapturedCase[] = [];
let createdSegmentId: string | null = null;

try {
  const createPaddedNameVariables = {
    name: paddedCreateName,
    query: 'number_of_orders = 0',
  };
  const createPaddedName = await runGraphqlRequest(createMutation, createPaddedNameVariables);
  assertNoUserErrors(createPaddedName, 'segmentCreate', 'segmentCreate padded name edge case');
  createdSegmentId = readCreatedSegmentId(createPaddedName);
  cases.push({
    name: 'segmentCreatePaddedNameAccepted',
    request: { query: createMutation, variables: createPaddedNameVariables },
    response: createPaddedName,
  });

  const createRawQueryLimitVariables = {
    name: `Query ${marker}`,
    query: rawOverLimitQuery,
  };
  const createRawQueryLimit = await runGraphqlRequest(createMutation, createRawQueryLimitVariables);
  assertUserErrors(createRawQueryLimit, 'segmentCreate', 'segmentCreate raw query length edge case');
  cases.push({
    name: 'segmentCreateRawQueryLengthRejected',
    request: { query: createMutation, variables: createRawQueryLimitVariables },
    response: createRawQueryLimit,
  });

  const updatePaddedNameVariables = {
    id: createdSegmentId,
    name: paddedUpdateName,
  };
  const updatePaddedName = await runGraphqlRequest(updateNameMutation, updatePaddedNameVariables);
  assertNoUserErrors(updatePaddedName, 'segmentUpdate', 'segmentUpdate padded name edge case');
  cases.push({
    name: 'segmentUpdatePaddedNameAccepted',
    request: { query: updateNameMutation, variables: updatePaddedNameVariables },
    response: updatePaddedName,
  });

  const updateRawQueryLimitVariables = {
    id: createdSegmentId,
    query: rawOverLimitQuery,
  };
  const updateRawQueryLimit = await runGraphqlRequest(updateQueryMutation, updateRawQueryLimitVariables);
  assertUserErrors(updateRawQueryLimit, 'segmentUpdate', 'segmentUpdate raw query length edge case');
  cases.push({
    name: 'segmentUpdateRawQueryLengthRejected',
    request: { query: updateQueryMutation, variables: updateRawQueryLimitVariables },
    response: updateRawQueryLimit,
  });
} finally {
  if (createdSegmentId) {
    const deleteVariables = { id: createdSegmentId };
    const cleanup = await runGraphqlRequest(deleteMutation, deleteVariables);
    assertGraphqlOk(cleanup, 'segmentDelete length edge cleanup');
    cases.push({
      name: 'segmentDeleteCleanup',
      request: {
        query: deleteMutation,
        variables: deleteVariables,
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
        'Live Shopify evidence covers segmentCreate/segmentUpdate length normalization edge cases: name length is checked after stripping while query length is checked against raw input.',
        'The script creates one disposable live segment for the accepted create/update branches and deletes it during cleanup.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
