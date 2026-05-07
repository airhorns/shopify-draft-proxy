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
const outputPath = path.join(outputDir, 'customer-segment-members-query-create-direct-query-grammar.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const memberQueryCreateMutation = `#graphql
  mutation CustomerSegmentMembersQueryCreateDirectQueryGrammar($input: CustomerSegmentMembersQueryInput!) {
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

function assertGraphqlOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPayload(result: ConformanceGraphqlResult): Record<string, unknown> {
  const data = result.payload.data as Record<string, unknown> | undefined;
  const payload = data?.['customerSegmentMembersQueryCreate'] as Record<string, unknown> | undefined;
  if (!payload) {
    throw new Error(`customerSegmentMembersQueryCreate missing: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return payload;
}

function assertNoUserErrors(result: ConformanceGraphqlResult, context: string): void {
  const payload = readPayload(result);
  const query = payload['customerSegmentMembersQuery'];
  const userErrors = payload['userErrors'];
  if (!query || !Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${context} returned unexpected payload: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function assertUserErrors(result: ConformanceGraphqlResult, context: string): void {
  const payload = readPayload(result);
  const query = payload['customerSegmentMembersQuery'];
  const userErrors = payload['userErrors'];
  if (query !== null || !Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`${context} did not return userErrors: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

async function captureCase(
  cases: CapturedCase[],
  name: string,
  variables: Record<string, unknown>,
  assert: (result: ConformanceGraphqlResult) => void,
): Promise<void> {
  const response = await runGraphqlRequest(memberQueryCreateMutation, variables);
  assertGraphqlOk(response, name);
  assert(response);
  cases.push({
    name,
    request: { query: memberQueryCreateMutation, variables },
    response,
  });
}

const cases: CapturedCase[] = [];

await captureCase(
  cases,
  'directQueryCustomerCountriesAccept',
  {
    input: {
      query: "customer_countries CONTAINS 'CA'",
    },
  },
  (result) => assertNoUserErrors(result, 'customer_countries direct query'),
);

await captureCase(
  cases,
  'directQueryCompaniesNullAccept',
  {
    input: {
      query: 'companies IS NULL',
    },
  },
  (result) => assertNoUserErrors(result, 'companies null direct query'),
);

await captureCase(
  cases,
  'directQueryParenthesizedOrAccept',
  {
    input: {
      query: '(number_of_orders >= 1) OR (number_of_orders = 0)',
    },
  },
  (result) => assertNoUserErrors(result, 'parenthesized OR direct query'),
);

await captureCase(
  cases,
  'directQueryMalformedRejected',
  {
    input: {
      query: 'not a valid segment query ???',
    },
  },
  (result) => assertUserErrors(result, 'malformed direct query'),
);

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
        'Live evidence that customerSegmentMembersQueryCreate(input: { query }) accepts broad segment grammar directly through the Customer Data Platform path.',
        'Accepted branches cover customer countries, company null checks, boolean composition, and parentheses.',
        'The malformed branch captures the CDP-shaped CustomerSegmentMembersQueryUserError response for one representative invalid query.',
        'CustomerSegmentMembersQuery jobs are Shopify async query state and do not have a cleanup mutation.',
      ],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
