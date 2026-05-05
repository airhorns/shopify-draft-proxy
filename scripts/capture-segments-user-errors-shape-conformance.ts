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
const outputPath = path.join(outputDir, 'segments-user-errors-shape.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const segmentCreateMutation = `#graphql
  mutation SegmentCreateUserErrorsShape($name: String!, $query: String!) {
    segmentCreate(name: $name, query: $query) {
      segment {
        id
      }
      userErrors {
        __typename
        field
        message
      }
    }
  }
`;

const segmentUpdateMutation = `#graphql
  mutation SegmentUpdateUserErrorsShape($id: ID!) {
    segmentUpdate(id: $id) {
      segment {
        id
      }
      userErrors {
        __typename
        field
        message
      }
    }
  }
`;

const segmentDeleteMutation = `#graphql
  mutation SegmentDeleteUserErrorsShape($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        __typename
        field
        message
      }
    }
  }
`;

const memberQueryCreateMutation = `#graphql
  mutation CustomerSegmentMembersQueryCreateUserErrorsShape($input: CustomerSegmentMembersQueryInput!) {
    customerSegmentMembersQueryCreate(input: $input) {
      customerSegmentMembersQuery {
        id
      }
      userErrors {
        __typename
        field
        code
        message
      }
    }
  }
`;

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

const marker = `har712-user-errors-${Date.now()}`;
const cases: CapturedCase[] = [];
let createdSegmentId: string | null = null;

try {
  await captureCase(cases, 'segmentCreateBlankNameAndQuery', segmentCreateMutation, {
    name: '',
    query: '',
  });

  const setupCreate = await captureCase(cases, 'segmentUpdateSetup', segmentCreateMutation, {
    name: `HAR-712 user errors ${marker}`,
    query: 'number_of_orders >= 1',
  });
  createdSegmentId = readCreatedSegmentId(setupCreate);

  await captureCase(cases, 'segmentUpdateEmptyInput', segmentUpdateMutation, {
    id: createdSegmentId,
  });

  await captureCase(cases, 'segmentDeleteBogusId', segmentDeleteMutation, {
    id: 'gid://shopify/Segment/999999999999',
  });

  await captureCase(cases, 'memberQueryBothSelectorsRejected', memberQueryCreateMutation, {
    input: {
      segmentId: 'gid://shopify/Segment/1',
      query: 'number_of_orders > 0',
    },
  });

  await captureCase(cases, 'memberQueryNeitherSelectorRejected', memberQueryCreateMutation, {
    input: {},
  });
} finally {
  if (createdSegmentId) {
    const cleanup = await runGraphqlRequest(segmentDeleteMutation, { id: createdSegmentId });
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
        'HAR-712 live evidence for segment and customerSegmentMembersQueryCreate userErrors shape.',
        'Shopify UserError for segmentCreate/segmentUpdate/segmentDelete exposes __typename, field, and message; selecting code on UserError is rejected by the live schema, so default code:null behavior is covered by runtime tests.',
        'segmentUpdateEmptyInput uses one disposable live segment for the id-only validation branch, then deletes it during cleanup.',
        'CustomerSegmentMembersQueryUserError exposes code and returns INVALID for selector validation branches.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
