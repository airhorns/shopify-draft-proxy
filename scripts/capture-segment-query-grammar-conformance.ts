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
const outputPath = path.join(outputDir, 'segment-query-grammar-not-contains.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertSuccess(result: ConformanceGraphqlResult, context: string): void {
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
  mutation SegmentCreateNotContains($name: String!, $query: String!) {
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
  query SegmentQueryGrammarRead($id: ID!) {
    segment(id: $id) {
      ${segmentFields}
    }
  }
`;

const memberQueryCreateMutation = `#graphql
  mutation CustomerSegmentMembersQueryCreate($input: CustomerSegmentMembersQueryInput!) {
    customerSegmentMembersQueryCreate(input: $input) {
      customerSegmentMembersQuery {
        id
        currentCount
        done
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteMutation = `#graphql
  mutation SegmentDelete($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

const marker = `har395-not-contains-${Date.now()}`;
const segmentQuery = `customer_tags NOT CONTAINS '${marker}'`;
const createVariables = {
  name: `HAR-395 not contains ${marker}`,
  query: segmentQuery,
};
const memberQueryVariables = {
  input: {
    query: segmentQuery,
  },
};
const cases: CapturedCase[] = [];
let createdSegmentId: string | null = null;

try {
  const create = await runGraphqlRequest(createMutation, createVariables);
  assertSuccess(create, 'segmentCreate NOT CONTAINS');
  cases.push({
    name: 'segmentCreateNotContains',
    request: {
      query: createMutation,
      variables: createVariables,
    },
    response: create,
  });

  createdSegmentId = readCreatedSegmentId(create);
  const readVariables = { id: createdSegmentId };
  const downstreamRead = await runGraphqlRequest(readQuery, readVariables);
  assertSuccess(downstreamRead, 'downstream segment read');
  cases.push({
    name: 'segmentReadCreated',
    request: {
      query: readQuery,
      variables: readVariables,
    },
    response: downstreamRead,
  });

  const memberQuery = await runGraphqlRequest(memberQueryCreateMutation, memberQueryVariables);
  assertSuccess(memberQuery, 'customerSegmentMembersQueryCreate NOT CONTAINS');
  cases.push({
    name: 'customerSegmentMembersQueryCreateNotContains',
    request: {
      query: memberQueryCreateMutation,
      variables: memberQueryVariables,
    },
    response: memberQuery,
  });
} finally {
  if (createdSegmentId) {
    const deleteVariables = { id: createdSegmentId };
    const cleanup = await runGraphqlRequest(deleteMutation, deleteVariables);
    assertSuccess(cleanup, 'segmentDelete cleanup');
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
        "Live Shopify evidence that customer segment grammar accepts `customer_tags NOT CONTAINS 'tag'` for segmentCreate and customerSegmentMembersQueryCreate.",
        'The segment created for this capture is deleted in the cleanup case. The member-query job is left as Shopify async query state and is not a store resource with a delete mutation.',
      ],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
