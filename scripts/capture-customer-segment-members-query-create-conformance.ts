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
const outputPath = path.join(outputDir, 'customer-segment-members-query-create-validation-and-shape.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const memberQueryCreateMutation = `#graphql
  mutation CustomerSegmentMembersQueryCreateValidationAndShape($input: CustomerSegmentMembersQueryInput!) {
    customerSegmentMembersQueryCreate(input: $input) {
      customerSegmentMembersQuery {
        id
        currentCount
        done
      }
      userErrors {
        field
        code
        message
      }
    }
  }
`;

const memberQueryLookupQuery = `#graphql
  query CustomerSegmentMembersQueryLookupValidationAndShape($id: ID!) {
    customerSegmentMembersQuery(id: $id) {
      id
      currentCount
      done
    }
  }
`;

const segmentCreateMutation = `#graphql
  mutation SegmentCreateForMemberQueryShape($name: String!, $query: String!) {
    segmentCreate(name: $name, query: $query) {
      segment {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const segmentDeleteMutation = `#graphql
  mutation SegmentDeleteForMemberQueryShape($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertSuccess(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readMemberQueryId(result: ConformanceGraphqlResult): string {
  const data = result.payload.data as Record<string, unknown> | undefined;
  const create = data?.['customerSegmentMembersQueryCreate'] as Record<string, unknown> | undefined;
  const query = create?.['customerSegmentMembersQuery'] as Record<string, unknown> | undefined;
  const id = query?.['id'];
  if (typeof id !== 'string') {
    throw new Error(
      `customerSegmentMembersQueryCreate did not return an id: ${JSON.stringify(result.payload, null, 2)}`,
    );
  }
  return id;
}

function readSegmentId(result: ConformanceGraphqlResult): string {
  const data = result.payload.data as Record<string, unknown> | undefined;
  const create = data?.['segmentCreate'] as Record<string, unknown> | undefined;
  const segment = create?.['segment'] as Record<string, unknown> | undefined;
  const id = segment?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`segmentCreate did not return an id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

async function captureCase(
  cases: CapturedCase[],
  name: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<ConformanceGraphqlResult> {
  const response = await runGraphqlRequest(query, variables);
  assertSuccess(response, name);
  cases.push({
    name,
    request: { query, variables },
    response,
  });
  return response;
}

const marker = `har714-member-query-${Date.now()}`;
const memberQuery = 'number_of_orders > 0';
const emptyMemberQuery = 'number_of_orders = 999999';
const cases: CapturedCase[] = [];
let createdSegmentId: string | null = null;

try {
  await captureCase(cases, 'bothQueryAndSegmentIdRejected', memberQueryCreateMutation, {
    input: {
      segmentId: 'gid://shopify/Segment/1',
      query: memberQuery,
    },
  });

  await captureCase(cases, 'neitherQueryNorSegmentIdRejected', memberQueryCreateMutation, {
    input: {},
  });

  const queryCreate = await captureCase(cases, 'queryCreateInitialized', memberQueryCreateMutation, {
    input: {
      query: memberQuery,
    },
  });

  const setupSegment = await runGraphqlRequest(segmentCreateMutation, {
    name: `HAR-714 member query ${marker}`,
    query: emptyMemberQuery,
  });
  assertSuccess(setupSegment, 'segmentCreate setup');
  createdSegmentId = readSegmentId(setupSegment);

  await captureCase(cases, 'segmentIdCreateInitialized', memberQueryCreateMutation, {
    input: {
      segmentId: createdSegmentId,
    },
  });

  await captureCase(cases, 'queryLookupCreatedInitialized', memberQueryLookupQuery, {
    id: readMemberQueryId(queryCreate),
  });
} finally {
  if (createdSegmentId) {
    const cleanup = await runGraphqlRequest(segmentDeleteMutation, { id: createdSegmentId });
    assertSuccess(cleanup, 'segmentDelete cleanup');
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
        'HAR-714 live evidence for customerSegmentMembersQueryCreate validation when both or neither input selectors are supplied.',
        '`CustomerSegmentMembersQuery.status` was not selectable on the configured 2025-01 or 2026-04 Admin schema during capture; runtime tests cover the local projected status field.',
        'A disposable segment is created only to capture the segmentId-backed success branch, then deleted during cleanup.',
        'CustomerSegmentMembersQuery jobs are Shopify async query state and do not have a cleanup mutation.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
