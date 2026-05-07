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

type CleanupResult = {
  segmentId: string;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'customer-segment-members-query-create-segment-id-paths.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const segmentCreateMutation = `#graphql
  mutation SegmentCreateForMemberQuerySegmentIdPaths($name: String!, $query: String!) {
    segmentCreate(name: $name, query: $query) {
      segment {
        id
        name
        query
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const segmentDeleteMutation = `#graphql
  mutation SegmentDeleteForMemberQuerySegmentIdPaths($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

const memberQueryCreateMutation = `#graphql
  mutation CustomerSegmentMembersQueryCreateSegmentIdPaths($input: CustomerSegmentMembersQueryInput!) {
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

function assertSuccess(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
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

const marker = `member-query-segment-id-paths-${Date.now()}`;
const cases: CapturedCase[] = [];
const createdSegmentIds: string[] = [];
const cleanup: CleanupResult[] = [];

try {
  const companiesSegment = await captureCase(cases, 'segmentCreateCompaniesNull', segmentCreateMutation, {
    name: `Member query companies ${marker}`,
    query: 'companies IS NULL',
  });
  const companiesSegmentId = readSegmentId(companiesSegment);
  createdSegmentIds.push(companiesSegmentId);

  await captureCase(cases, 'segmentIdCreateCompaniesNull', memberQueryCreateMutation, {
    input: {
      segmentId: companiesSegmentId,
    },
  });

  const countrySegment = await captureCase(cases, 'segmentCreateCustomerCountriesCa', segmentCreateMutation, {
    name: `Member query customer country ${marker}`,
    query: "number_of_orders >= 1 AND customer_countries CONTAINS 'CA'",
  });
  const countrySegmentId = readSegmentId(countrySegment);
  createdSegmentIds.push(countrySegmentId);

  await captureCase(cases, 'segmentIdCreateCustomerCountriesCa', memberQueryCreateMutation, {
    input: {
      segmentId: countrySegmentId,
    },
  });

  await captureCase(cases, 'unknownSegmentIdRejected', memberQueryCreateMutation, {
    input: {
      segmentId: 'gid://shopify/Segment/999999999999999999',
    },
  });
} finally {
  for (const segmentId of createdSegmentIds.reverse()) {
    const response = await runGraphqlRequest(segmentDeleteMutation, { id: segmentId });
    assertSuccess(response, `segmentDelete cleanup ${segmentId}`);
    cleanup.push({ segmentId, response });
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
      cleanup,
      notes: [
        'Live evidence that customerSegmentMembersQueryCreate(input: { segmentId }) accepts stored segment queries using broad segment grammar without revalidating the resolved query on this mutation surface.',
        "The configured live Admin API rejects the `country = 'CA'` alias at segmentCreate time, so this fixture records the accepted customer country spelling instead.",
        'Live evidence that an unknown valid Segment GID returns CustomerSegmentMembersQueryUserError field:null code:INVALID message:"Invalid segment ID."',
        'Disposable segments are created to capture segmentId-backed branches, then deleted during cleanup.',
        'CustomerSegmentMembersQuery jobs are Shopify async query state and do not have a cleanup mutation.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
