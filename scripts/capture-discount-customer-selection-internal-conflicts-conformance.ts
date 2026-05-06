/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (current === null || typeof current !== 'object') return undefined;
    current = (current as JsonRecord)[segment];
  }
  return current;
}

function readRequiredString(result: ConformanceGraphqlResult, pathSegments: string[], context: string): string {
  const value = readPath(result.payload, pathSegments);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${context} did not return ${pathSegments.join('.')}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return value;
}

function assertSuccess(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertBadRequest(result: ConformanceGraphqlResult, context: string, message: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed with HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const errors = result.payload.errors;
  if (!Array.isArray(errors) || errors.length !== 1) {
    throw new Error(`${context} expected one top-level error: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const error = errors[0] as JsonRecord;
  if (error['message'] !== message) {
    throw new Error(`${context} expected message ${message}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const extensions = error['extensions'] as JsonRecord | undefined;
  if (extensions?.['code'] !== 'BAD_REQUEST') {
    throw new Error(`${context} expected BAD_REQUEST extension: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const data = result.payload.data as JsonRecord | undefined;
  if (data?.['discountCodeBasicCreate'] !== null) {
    throw new Error(
      `${context} expected data.discountCodeBasicCreate null: ${JSON.stringify(result.payload, null, 2)}`,
    );
  }
}

function assertInvalidVariable(result: ConformanceGraphqlResult, context: string, messagePrefix: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed with HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const errors = result.payload.errors;
  if (!Array.isArray(errors) || errors.length !== 1) {
    throw new Error(`${context} expected one top-level error: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const error = errors[0] as JsonRecord;
  if (typeof error['message'] !== 'string' || !error['message'].startsWith(messagePrefix)) {
    throw new Error(`${context} expected message prefix ${messagePrefix}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const extensions = error['extensions'] as JsonRecord | undefined;
  if (extensions?.['code'] !== 'INVALID_VARIABLE') {
    throw new Error(`${context} expected INVALID_VARIABLE extension: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-customer-selection-internal-conflicts.json');
const createDocument = await readFile(
  'config/parity-requests/discounts/discount-customer-selection-internal-conflicts-create.graphql',
  'utf8',
);
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const runId = Date.now();
const marker = `har933-customer-selection-${runId}`;
const startsAt = '2026-04-25T00:00:00Z';
const cleanup: JsonRecord = {};

const customerCreateDocument = `#graphql
  mutation DiscountCustomerSelectionConflictCustomerCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        displayName
        email
        tags
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerDeleteDocument = `#graphql
  mutation DiscountCustomerSelectionConflictCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

const segmentCreateDocument = `#graphql
  mutation DiscountCustomerSelectionConflictSegmentCreate($name: String!, $query: String!) {
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

const segmentDeleteDocument = `#graphql
  mutation DiscountCustomerSelectionConflictSegmentDelete($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

const discountDeleteDocument = `#graphql
  mutation DiscountCustomerSelectionConflictDiscountDelete($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function baseInput(code: string): JsonRecord {
  return {
    title: `Customer selection ${code}`,
    code,
    startsAt,
    customerGets: {
      value: { percentage: 0.1 },
      items: { all: true },
    },
  };
}

function inputWithCustomerSelection(code: string, customerSelection: JsonRecord): JsonRecord {
  return {
    ...baseInput(code),
    customerSelection,
  };
}

let customerId: string | null = null;
let segmentId: string | null = null;
let happyDiscountId: string | null = null;

try {
  const customerCreate = await runGraphqlRaw(customerCreateDocument, {
    input: {
      firstName: 'Discount',
      lastName: 'Customer Selection',
      email: `har933-customer-selection-${runId}@example.com`,
      tags: [marker],
    },
  });
  assertSuccess(customerCreate, 'customerCreate setup');
  customerId = readRequiredString(customerCreate, ['data', 'customerCreate', 'customer', 'id'], 'customerCreate setup');

  const segmentCreate = await runGraphqlRaw(segmentCreateDocument, {
    name: `HAR-933 customer selection ${runId}`,
    query: `customer_tags CONTAINS '${marker}'`,
  });
  assertSuccess(segmentCreate, 'segmentCreate setup');
  segmentId = readRequiredString(segmentCreate, ['data', 'segmentCreate', 'segment', 'id'], 'segmentCreate setup');

  const allWithCustomersVariables = {
    input: inputWithCustomerSelection(`HAR933CUST${runId}`, {
      all: true,
      customers: { add: [customerId] },
    }),
  };
  const allWithSavedSearchesVariables = {
    input: inputWithCustomerSelection(`HAR933SAVE${runId}`, {
      all: true,
      customerSavedSearches: { add: ['gid://shopify/SavedSearch/1'] },
    }),
  };
  const allWithSegmentsVariables = {
    input: inputWithCustomerSelection(`HAR933SEG${runId}`, {
      all: true,
      customerSegments: { add: [segmentId] },
    }),
  };
  const happyCustomersVariables = {
    input: inputWithCustomerSelection(`HAR933OK${runId}`, {
      customers: { add: [customerId] },
    }),
  };

  const allWithCustomers = await runGraphqlRaw(createDocument, allWithCustomersVariables);
  assertBadRequest(
    allWithCustomers,
    'all/customers conflict',
    'A discount cannot have customerSelection set to all, when customers or customerSavedSearches is specified.',
  );

  const allWithSavedSearches = await runGraphqlRaw(createDocument, allWithSavedSearchesVariables);
  assertInvalidVariable(
    allWithSavedSearches,
    'all/customerSavedSearches conflict',
    'Variable $input of type DiscountCodeBasicInput! was provided invalid value for customerSelection.customerSavedSearches',
  );

  const allWithSegments = await runGraphqlRaw(createDocument, allWithSegmentsVariables);
  assertBadRequest(
    allWithSegments,
    'all/customerSegments conflict',
    'A discount cannot have customerSelection set to all, when customerSegments is specified.',
  );

  const happyCustomers = await runGraphqlRaw(createDocument, happyCustomersVariables);
  assertSuccess(happyCustomers, 'happy customers.add create');
  happyDiscountId = readRequiredString(
    happyCustomers,
    ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id'],
    'happy customers.add create',
  );

  if (happyDiscountId) {
    cleanup['discountDelete'] = await runGraphqlRaw(discountDeleteDocument, { id: happyDiscountId });
    assertSuccess(cleanup['discountDelete'] as ConformanceGraphqlResult, 'discount cleanup');
  }
  if (segmentId) {
    cleanup['segmentDelete'] = await runGraphqlRaw(segmentDeleteDocument, { id: segmentId });
    assertSuccess(cleanup['segmentDelete'] as ConformanceGraphqlResult, 'segment cleanup');
  }
  if (customerId) {
    cleanup['customerDelete'] = await runGraphqlRaw(customerDeleteDocument, { input: { id: customerId } });
    assertSuccess(cleanup['customerDelete'] as ConformanceGraphqlResult, 'customer cleanup');
  }

  const fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    accessScopes: scopeProbe,
    setup: {
      customerCreate: { query: customerCreateDocument, response: customerCreate },
      segmentCreate: { query: segmentCreateDocument, response: segmentCreate },
      customerId,
      segmentId,
    },
    cases: {
      allWithCustomers: {
        query: createDocument,
        variables: allWithCustomersVariables,
        status: allWithCustomers.status,
        response: allWithCustomers.payload,
      },
      allWithSavedSearches: {
        query: createDocument,
        variables: allWithSavedSearchesVariables,
        status: allWithSavedSearches.status,
        response: allWithSavedSearches.payload,
      },
      allWithSegments: {
        query: createDocument,
        variables: allWithSegmentsVariables,
        status: allWithSegments.status,
        response: allWithSegments.payload,
      },
      happyCustomers: {
        query: createDocument,
        variables: happyCustomersVariables,
        status: happyCustomers.status,
        response: happyCustomers.payload,
      },
    },
    cleanup,
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        customerId,
        segmentId,
        happyDiscountId,
      },
      null,
      2,
    ),
  );
} finally {
  if (happyDiscountId && !cleanup['discountDelete']) {
    cleanup['discountDeleteAfterFailure'] = await runGraphqlRaw(discountDeleteDocument, { id: happyDiscountId });
  }
  if (segmentId && !cleanup['segmentDelete']) {
    cleanup['segmentDeleteAfterFailure'] = await runGraphqlRaw(segmentDeleteDocument, { id: segmentId });
  }
  if (customerId && !cleanup['customerDelete']) {
    cleanup['customerDeleteAfterFailure'] = await runGraphqlRaw(customerDeleteDocument, { input: { id: customerId } });
  }
}
