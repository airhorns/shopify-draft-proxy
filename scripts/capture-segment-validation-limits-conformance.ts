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
const outputPath = path.join(outputDir, 'segments-create-update-validation-limits.json');
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
  mutation SegmentDeleteValidationLimitsCleanup($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

const marker = `har713-validation-limits-${Date.now()}`;
const longName = 'N'.repeat(256);
const longQuery = `number_of_orders >= 1 ${'x'.repeat(5000)}`;
const cases: CapturedCase[] = [];
let createdSegmentId: string | null = null;

try {
  const createLongNameVariables = {
    name: longName,
    query: 'number_of_orders >= 1',
  };
  const createLongName = await runGraphqlRequest(createMutation, createLongNameVariables);
  assertGraphqlOk(createLongName, 'segmentCreate long name validation');
  cases.push({
    name: 'segmentCreateLongName',
    request: { query: createMutation, variables: createLongNameVariables },
    response: createLongName,
  });

  const createLongQueryVariables = {
    name: `HAR-713 long query ${marker}`,
    query: longQuery,
  };
  const createLongQuery = await runGraphqlRequest(createMutation, createLongQueryVariables);
  assertGraphqlOk(createLongQuery, 'segmentCreate long query validation');
  cases.push({
    name: 'segmentCreateLongQuery',
    request: { query: createMutation, variables: createLongQueryVariables },
    response: createLongQuery,
  });

  const setupVariables = {
    name: `HAR-713 setup ${marker}`,
    query: 'number_of_orders >= 1',
  };
  const setupCreate = await runGraphqlRequest(createMutation, setupVariables);
  assertGraphqlOk(setupCreate, 'segmentCreate update setup');
  createdSegmentId = readCreatedSegmentId(setupCreate);
  cases.push({
    name: 'segmentCreateUpdateSetup',
    request: { query: createMutation, variables: setupVariables },
    response: setupCreate,
  });

  const updateLongNameVariables = {
    id: createdSegmentId,
    name: longName,
  };
  const updateLongName = await runGraphqlRequest(updateNameMutation, updateLongNameVariables);
  assertGraphqlOk(updateLongName, 'segmentUpdate long name validation');
  cases.push({
    name: 'segmentUpdateLongName',
    request: { query: updateNameMutation, variables: updateLongNameVariables },
    response: updateLongName,
  });

  const updateLongQueryVariables = {
    id: createdSegmentId,
    query: longQuery,
  };
  const updateLongQuery = await runGraphqlRequest(updateQueryMutation, updateLongQueryVariables);
  assertGraphqlOk(updateLongQuery, 'segmentUpdate long query validation');
  cases.push({
    name: 'segmentUpdateLongQuery',
    request: { query: updateQueryMutation, variables: updateLongQueryVariables },
    response: updateLongQuery,
  });
} finally {
  if (createdSegmentId) {
    const deleteVariables = { id: createdSegmentId };
    const cleanup = await runGraphqlRequest(deleteMutation, deleteVariables);
    assertGraphqlOk(cleanup, 'segmentDelete validation cleanup');
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
        'Live Shopify evidence covers segmentCreate/segmentUpdate name and query length validation.',
        'The segment-limit branch is not parity-covered here because pre-seeding local staged segments is invalid parity evidence.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
