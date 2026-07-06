/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { readDiscountHydrateDocument } from './discount-hydrate-query.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

function assertSuccess(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertHttpSuccess(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (current === null || typeof current !== 'object') {
      return undefined;
    }
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

function assertNoUserErrors(result: ConformanceGraphqlResult, pathSegments: string[], context: string): void {
  const value = readPath(result.payload, pathSegments);
  if (!Array.isArray(value) || value.length !== 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(value, null, 2)}`);
  }
}

function assertConfigPreserved(result: ConformanceGraphqlResult, pathSegments: string[], context: string): void {
  const discount = readPath(result.payload, pathSegments);
  if (discount === null || typeof discount !== 'object' || Array.isArray(discount)) {
    throw new Error(`${context} did not return a discount object: ${JSON.stringify(result.payload, null, 2)}`);
  }
  const record = discount as JsonRecord;
  const customerGets = record['customerGets'] as JsonRecord | undefined;
  const value = customerGets?.['value'] as JsonRecord | undefined;
  const amount = (value?.['amount'] as JsonRecord | undefined)?.['amount'];
  const minimumRequirement = record['minimumRequirement'] as JsonRecord | undefined;
  const subtotal = (minimumRequirement?.['greaterThanOrEqualToSubtotal'] as JsonRecord | undefined)?.['amount'];
  if (
    value?.['__typename'] !== 'DiscountAmount' ||
    amount !== '5.0' ||
    value?.['appliesOnEachItem'] !== false ||
    (customerGets?.['items'] as JsonRecord | undefined)?.['__typename'] !== 'AllDiscountItems' ||
    minimumRequirement?.['__typename'] !== 'DiscountMinimumSubtotal' ||
    subtotal !== '50.0' ||
    record['usageLimit'] !== 100 ||
    record['appliesOncePerCustomer'] !== true
  ) {
    throw new Error(`${context} did not preserve fixed-amount config: ${JSON.stringify(record, null, 2)}`);
  }
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-upstream-fixed-amount-activate.json');
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

const activateDocument = await readFile(
  'config/parity-requests/discounts/discount-upstream-fixed-amount-activate.graphql',
  'utf8',
);
const readDocument = await readFile(
  'config/parity-requests/discounts/discount-upstream-fixed-amount-read.graphql',
  'utf8',
);
const discountHydrateDocument = await readDiscountHydrateDocument();

const codeBasicSelection = `#graphql
  codeDiscountNode {
    id
    metafields(first: 5) {
      nodes {
        id
        namespace
        key
        type
        value
        createdAt
        updatedAt
      }
    }
    codeDiscount {
      __typename
      ... on DiscountCodeBasic {
        title
        status
        summary
        startsAt
        endsAt
        createdAt
        updatedAt
        asyncUsageCount
        usageLimit
        appliesOncePerCustomer
        recurringCycleLimit
        discountClasses
        combinesWith {
          productDiscounts
          orderDiscounts
          shippingDiscounts
        }
        context {
          __typename
          ... on DiscountBuyerSelectionAll {
            all
          }
        }
        customerGets {
          value {
            __typename
            ... on DiscountPercentage {
              percentage
            }
            ... on DiscountAmount {
              amount {
                amount
                currencyCode
              }
              appliesOnEachItem
            }
          }
          items {
            __typename
            ... on AllDiscountItems {
              allItems
            }
          }
          appliesOnOneTimePurchase
          appliesOnSubscription
        }
        minimumRequirement {
          __typename
          ... on DiscountMinimumSubtotal {
            greaterThanOrEqualToSubtotal {
              amount
              currencyCode
            }
          }
          ... on DiscountMinimumQuantity {
            greaterThanOrEqualToQuantity
          }
        }
        codes(first: 2) {
          nodes {
            id
            code
            asyncUsageCount
          }
        }
      }
    }
  }
  userErrors {
    field
    message
    code
    extraInfo
  }
`;

const createDocument = `#graphql
  mutation DiscountUpstreamFixedAmountSetupCreate($input: DiscountCodeBasicInput!) {
    discountCodeBasicCreate(basicCodeDiscount: $input) {
      ${codeBasicSelection}
    }
  }
`;

const deleteDocument = `#graphql
  mutation DiscountUpstreamFixedAmountCleanupDelete($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

const runId = Date.now();
const code = `UPFIX${runId}`;
const startsAt = new Date(Date.now() - 120_000).toISOString();
const createVariables = {
  input: {
    title: `Upstream fixed amount ${runId}`,
    code,
    startsAt,
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    customerGets: {
      value: {
        discountAmount: {
          amount: '5.00',
          appliesOnEachItem: false,
        },
      },
      items: {
        all: true,
      },
    },
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '50.00',
      },
    },
    usageLimit: 100,
    appliesOncePerCustomer: true,
  },
};

let discountId: string | null = null;
let cleanup: ConformanceGraphqlResult | null = null;
let deleted = false;

try {
  const create = await runGraphqlRaw(createDocument, createVariables);
  assertSuccess(create, 'upstream fixed-amount create');
  assertNoUserErrors(create, ['data', 'discountCodeBasicCreate', 'userErrors'], 'upstream fixed-amount create');
  assertConfigPreserved(
    create,
    ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'codeDiscount'],
    'upstream fixed-amount create',
  );
  discountId = readRequiredString(
    create,
    ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id'],
    'upstream fixed-amount create',
  );

  const hydrateBeforeActivate = await runGraphqlRaw(discountHydrateDocument, { id: discountId });
  assertHttpSuccess(hydrateBeforeActivate, 'upstream fixed-amount hydrate before activate');
  assertConfigPreserved(
    hydrateBeforeActivate,
    ['data', 'codeNode', 'codeDiscount'],
    'upstream fixed-amount hydrate before activate',
  );

  const activate = await runGraphqlRaw(activateDocument, { id: discountId });
  assertSuccess(activate, 'upstream fixed-amount activate');
  assertNoUserErrors(activate, ['data', 'discountCodeActivate', 'userErrors'], 'upstream fixed-amount activate');
  assertConfigPreserved(
    activate,
    ['data', 'discountCodeActivate', 'codeDiscountNode', 'codeDiscount'],
    'upstream fixed-amount activate',
  );

  const readAfterActivate = await runGraphqlRaw(readDocument, { id: discountId });
  assertSuccess(readAfterActivate, 'upstream fixed-amount read after activate');
  assertConfigPreserved(
    readAfterActivate,
    ['data', 'codeDiscountNode', 'codeDiscount'],
    'upstream fixed-amount read after activate',
  );

  cleanup = await runGraphqlRaw(deleteDocument, { id: discountId });
  assertSuccess(cleanup, 'upstream fixed-amount cleanup');
  assertNoUserErrors(cleanup, ['data', 'discountCodeDelete', 'userErrors'], 'upstream fixed-amount cleanup');
  deleted = true;

  const output = {
    scenarioId: 'discount-upstream-fixed-amount-activate',
    storeDomain,
    apiVersion,
    runId,
    variables: {
      id: discountId,
      code,
    },
    requests: {
      setupCreate: { query: createDocument, variables: createVariables },
      activate: { query: activateDocument, variables: { id: discountId } },
      readAfterActivate: { query: readDocument, variables: { id: discountId } },
      cleanup: { query: deleteDocument, variables: { id: discountId } },
    },
    scopeProbe,
    setup: {
      create: { response: create },
      hydrateBeforeActivate: { response: hydrateBeforeActivate },
    },
    activate: { response: activate },
    readAfterActivate: { response: readAfterActivate },
    cleanup: { response: cleanup },
    upstreamCalls: [
      {
        operationName: 'DiscountHydrate',
        variables: { id: discountId },
        query: discountHydrateDocument,
        response: { status: hydrateBeforeActivate.status, body: hydrateBeforeActivate.payload },
      },
    ],
  };

  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        discountId,
        code,
      },
      null,
      2,
    ),
  );
} finally {
  if (!deleted && discountId !== null) {
    try {
      cleanup = await runGraphqlRaw(deleteDocument, { id: discountId });
      if (cleanup.status >= 200 && cleanup.status < 300 && !cleanup.payload.errors) {
        deleted = true;
      }
    } catch (error) {
      console.error(`cleanup failed for ${discountId}: ${(error as Error).message}`);
    }
  }
}
