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
const notContainsOutputPath = path.join(outputDir, 'segment-query-grammar-not-contains.json');
const createUpdateGrammarOutputPath = path.join(outputDir, 'segment-create-update-query-grammar.json');
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

function readMutationPayload(result: ConformanceGraphqlResult, root: string): Record<string, unknown> {
  const data = result.payload.data as Record<string, unknown> | undefined;
  const payload = data?.[root] as Record<string, unknown> | undefined;
  if (!payload) {
    throw new Error(`${root} missing from response: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return payload;
}

function readCreatedSegmentId(result: ConformanceGraphqlResult, root = 'segmentCreate'): string {
  const payload = readMutationPayload(result, root);
  const segment = payload['segment'] as Record<string, unknown> | null | undefined;
  const id = segment?.['id'];
  if (typeof id !== 'string') {
    throw new Error(`${root} did not return a segment id: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return id;
}

function assertNoUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  const payload = readMutationPayload(result, root);
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertUserErrors(result: ConformanceGraphqlResult, root: string, context: string): void {
  const payload = readMutationPayload(result, root);
  const userErrors = payload['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`${context} did not return userErrors: ${JSON.stringify(result.payload, null, 2)}`);
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
  mutation SegmentCreateQueryGrammar($name: String!, $query: String!) {
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

const updateMutation = `#graphql
  mutation SegmentUpdateQueryGrammar($id: ID!, $query: String) {
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
  mutation SegmentDeleteQueryGrammarCleanup($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

async function cleanupSegments(ids: string[], cases: CapturedCase[], label: string): Promise<void> {
  for (const [index, id] of ids.entries()) {
    const deleteVariables = { id };
    const cleanup = await runGraphqlRequest(deleteMutation, deleteVariables);
    assertGraphqlOk(cleanup, `${label} cleanup ${index + 1}`);
    cases.push({
      name: `${label}Cleanup${index + 1}`,
      request: {
        query: deleteMutation,
        variables: deleteVariables,
      },
      response: cleanup,
    });
  }
}

async function writeCaptureFile(outputPath: string, body: Record<string, unknown>): Promise<void> {
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(body, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
}

async function captureNotContainsScenario(marker: string): Promise<void> {
  const segmentQuery = `customer_tags NOT CONTAINS '${marker}'`;
  const createVariables = {
    name: `Query grammar not contains ${marker}`,
    query: segmentQuery,
  };
  const memberQueryVariables = {
    input: {
      query: segmentQuery,
    },
  };
  const cases: CapturedCase[] = [];
  const createdSegmentIds: string[] = [];

  try {
    const create = await captureCase(cases, 'segmentCreateNotContains', createMutation, createVariables, (result) =>
      assertNoUserErrors(result, 'segmentCreate', 'segmentCreate NOT CONTAINS'),
    );
    const createdSegmentId = readCreatedSegmentId(create);
    createdSegmentIds.push(createdSegmentId);

    const readVariables = { id: createdSegmentId };
    await captureCase(cases, 'segmentReadCreated', readQuery, readVariables, () => undefined);

    await captureCase(
      cases,
      'customerSegmentMembersQueryCreateNotContains',
      memberQueryCreateMutation,
      memberQueryVariables,
      (result) =>
        assertNoUserErrors(
          result,
          'customerSegmentMembersQueryCreate',
          'customerSegmentMembersQueryCreate NOT CONTAINS',
        ),
    );
  } finally {
    await cleanupSegments(createdSegmentIds, cases, 'segmentDeleteNotContains');
  }

  await writeCaptureFile(notContainsOutputPath, {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    cases,
    notes: [
      "Live Shopify evidence that customer segment grammar accepts `customer_tags NOT CONTAINS 'tag'` for segmentCreate and customerSegmentMembersQueryCreate.",
      'The segment created for this capture is deleted in the cleanup case. The member-query job is left as Shopify async query state and is not a store resource with a delete mutation.',
    ],
    upstreamCalls: [],
  });
}

async function captureCreateUpdateGrammarScenario(marker: string): Promise<void> {
  const cases: CapturedCase[] = [];
  const createdSegmentIds: string[] = [];

  const acceptedCreateCases = [
    {
      name: 'segmentCreateCustomerCountriesAccept',
      segmentName: 'Customer countries',
      query: "customer_countries CONTAINS 'CA'",
    },
    { name: 'segmentCreateAmountSpentAccept', segmentName: 'Amount spent', query: 'amount_spent > 100' },
    { name: 'segmentCreateCompaniesNullAccept', segmentName: 'Companies null', query: 'companies IS NULL' },
    {
      name: 'segmentCreateAndAccept',
      segmentName: 'Multi clause and',
      query: "number_of_orders >= 1 AND customer_countries CONTAINS 'CA'",
    },
    {
      name: 'segmentCreateParenthesizedOrAccept',
      segmentName: 'Parenthesized or',
      query: '(number_of_orders >= 1) OR (number_of_orders = 0)',
    },
    { name: 'segmentCreateRelativeDateAccept', segmentName: 'Relative date', query: 'last_order_date >= -30d' },
  ];

  try {
    for (const entry of acceptedCreateCases) {
      const response = await captureCase(
        cases,
        entry.name,
        createMutation,
        {
          name: `Query grammar ${entry.segmentName} ${marker}`,
          query: entry.query,
        },
        (result) => assertNoUserErrors(result, 'segmentCreate', entry.name),
      );
      createdSegmentIds.push(readCreatedSegmentId(response));
    }

    const setupCreate = await captureCase(
      cases,
      'segmentCreateUpdateSetup',
      createMutation,
      {
        name: `Query grammar update setup ${marker}`,
        query: 'number_of_orders >= 1',
      },
      (result) => assertNoUserErrors(result, 'segmentCreate', 'segmentCreate update setup'),
    );
    const updateSegmentId = readCreatedSegmentId(setupCreate);
    createdSegmentIds.push(updateSegmentId);

    await captureCase(
      cases,
      'segmentUpdateCustomerCountriesAccept',
      updateMutation,
      {
        id: updateSegmentId,
        query: "customer_countries CONTAINS 'CA'",
      },
      (result) => assertNoUserErrors(result, 'segmentUpdate', 'segmentUpdate customer country query'),
    );

    await captureCase(
      cases,
      'segmentCreateMalformedRejected',
      createMutation,
      {
        name: `Query grammar malformed ${marker}`,
        query: 'not a valid segment query ???',
      },
      (result) => assertUserErrors(result, 'segmentCreate', 'segmentCreate malformed query'),
    );
  } finally {
    await cleanupSegments(createdSegmentIds, cases, 'segmentDeleteCreateUpdateGrammar');
  }

  await writeCaptureFile(createUpdateGrammarOutputPath, {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    cases,
    notes: [
      'Live Shopify evidence that broader segment query grammar is accepted by segmentCreate and segmentUpdate.',
      'Accepted branches cover customer country, amount spent, IS NULL, AND, parenthesized OR, and relative date predicates. The malformed branch captures Shopify query userErrors.',
      'The public Admin API versions tested during capture reject the `country` and `total_spent` aliases, while the proxy still accepts them optimistically for callers that follow those aliases.',
      'Disposable segments created for this capture are deleted in cleanup cases.',
    ],
    upstreamCalls: [],
  });
}

const marker = `segment-query-grammar-${Date.now()}`;
await captureNotContainsScenario(marker);
await captureCreateUpdateGrammarScenario(marker);
