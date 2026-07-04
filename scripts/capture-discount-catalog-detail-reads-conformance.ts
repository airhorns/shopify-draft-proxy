/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRequest } = createAdminGraphqlClient(adminOptions);

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const catalogDocumentPath = 'config/parity-requests/discounts/discount-catalog-empty-read.graphql';
const codeDetailDocumentPath = 'config/parity-requests/discounts/discount-code-basic-detail-read.graphql';
const automaticDetailDocumentPath = 'config/parity-requests/discounts/discount-automatic-basic-detail-read.graphql';

const [catalogDocument, codeDetailDocument, automaticDetailDocument] = await Promise.all([
  readFile(catalogDocumentPath, 'utf8'),
  readFile(codeDetailDocumentPath, 'utf8'),
  readFile(automaticDetailDocumentPath, 'utf8'),
]);

const codeCreateDocument = `#graphql
  mutation DiscountCodeBasicDetailSetup($input: DiscountCodeBasicInput!) {
    discountCodeBasicCreate(basicCodeDiscount: $input) {
      codeDiscountNode {
        id
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
            discountClasses
            combinesWith {
              productDiscounts
              orderDiscounts
              shippingDiscounts
            }
            codes(first: 2) {
              nodes {
                id
                code
                asyncUsageCount
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
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
    }
  }
`;

const automaticCreateDocument = `#graphql
  mutation DiscountAutomaticBasicDetailSetup($input: DiscountAutomaticBasicInput!) {
    discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
      automaticDiscountNode {
        id
        automaticDiscount {
          __typename
          ... on DiscountAutomaticBasic {
            title
            status
            summary
            startsAt
            endsAt
            createdAt
            updatedAt
            asyncUsageCount
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
              ... on DiscountMinimumQuantity {
                greaterThanOrEqualToQuantity
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
    }
  }
`;

const codeDeleteDocument = `#graphql
  mutation DiscountCodeBasicDetailCleanup($id: ID!) {
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

const automaticDeleteDocument = `#graphql
  mutation DiscountAutomaticBasicDetailCleanup($id: ID!) {
    discountAutomaticDelete(id: $id) {
      deletedAutomaticDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

type RecordedCall = {
  operationName: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    body: ConformanceGraphqlResult['payload'];
  };
  query: string;
};

function assertGraphqlSuccess(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function userErrorsAt(result: ConformanceGraphqlResult, pathSegments: string[]): unknown[] {
  let current: unknown = result.payload;
  for (const segment of pathSegments) {
    current = (current as Record<string, unknown> | undefined)?.[segment];
  }
  return Array.isArray(current) ? current : [];
}

function assertNoUserErrors(result: ConformanceGraphqlResult, pathSegments: string[], label: string): void {
  const userErrors = userErrorsAt(result, pathSegments);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function recordedCall(
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): RecordedCall {
  return {
    operationName: operationName(query),
    variables,
    response: {
      status: result.status,
      body: result.payload,
    },
    query,
  };
}

function operationName(query: string): string {
  return /\b(?:query|mutation)\s+([A-Za-z0-9_]+)/u.exec(query)?.[1] ?? 'AnonymousOperation';
}

function readCodeDiscountId(result: ConformanceGraphqlResult): string {
  const id = (
    result.payload as {
      data?: { discountCodeBasicCreate?: { codeDiscountNode?: { id?: unknown } } };
    }
  ).data?.discountCodeBasicCreate?.codeDiscountNode?.id;

  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`discountCodeBasicCreate did not return an id: ${JSON.stringify(result, null, 2)}`);
  }

  return id;
}

function readAutomaticDiscountId(result: ConformanceGraphqlResult): string {
  const id = (
    result.payload as {
      data?: { discountAutomaticBasicCreate?: { automaticDiscountNode?: { id?: unknown } } };
    }
  ).data?.discountAutomaticBasicCreate?.automaticDiscountNode?.id;

  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`discountAutomaticBasicCreate did not return an id: ${JSON.stringify(result, null, 2)}`);
  }

  return id;
}

function catalogFixture(variables: Record<string, unknown>, result: ConformanceGraphqlResult) {
  return {
    variables,
    response: result.payload,
    upstreamCalls: [recordedCall(catalogDocument, variables, result)],
  };
}

const runId = Date.now();
const startsAt = '2026-04-25T00:00:00Z';
const catalogCode = `DRAFTCAT${runId}`;
let codeId: string | null = null;
let automaticId: string | null = null;

try {
  const codeCreateVariables = {
    input: {
      title: `Conformance catalog detail code ${runId}`,
      code: catalogCode,
      startsAt,
      endsAt: null,
      combinesWith: {
        productDiscounts: false,
        orderDiscounts: true,
        shippingDiscounts: false,
      },
      context: {
        all: 'ALL',
      },
      minimumRequirement: {
        subtotal: {
          greaterThanOrEqualToSubtotal: '1.00',
        },
      },
      customerGets: {
        value: {
          percentage: 0.1,
        },
        items: {
          all: true,
        },
      },
    },
  };
  const codeCreate = await runGraphqlRequest(codeCreateDocument, codeCreateVariables);
  assertGraphqlSuccess(codeCreate, 'discountCodeBasicCreate setup');
  assertNoUserErrors(codeCreate, ['data', 'discountCodeBasicCreate', 'userErrors'], 'discountCodeBasicCreate setup');
  codeId = readCodeDiscountId(codeCreate);

  const automaticCreateVariables = {
    input: {
      title: `Conformance detail automatic ${runId}`,
      startsAt,
      endsAt: null,
      combinesWith: {
        productDiscounts: false,
        orderDiscounts: true,
        shippingDiscounts: false,
      },
      context: {
        all: 'ALL',
      },
      minimumRequirement: {
        quantity: {
          greaterThanOrEqualToQuantity: '2',
        },
      },
      customerGets: {
        value: {
          percentage: 0.15,
        },
        items: {
          all: true,
        },
      },
    },
  };
  const automaticCreate = await runGraphqlRequest(automaticCreateDocument, automaticCreateVariables);
  assertGraphqlSuccess(automaticCreate, 'discountAutomaticBasicCreate setup');
  assertNoUserErrors(
    automaticCreate,
    ['data', 'discountAutomaticBasicCreate', 'userErrors'],
    'discountAutomaticBasicCreate setup',
  );
  automaticId = readAutomaticDiscountId(automaticCreate);

  const emptyVariables = {
    query: 'title:__draft_discount_empty_probe__',
    first: 2,
    sortKey: 'TITLE',
    reverse: false,
    countLimit: 10000,
  };
  const nonEmptyVariables = {
    query: `title:'Conformance catalog detail code ${runId}'`,
    first: 2,
    sortKey: 'TITLE',
    reverse: false,
    countLimit: 10000,
  };
  const statusFilterVariables = {
    query: `status:active title:'Conformance catalog detail code ${runId}'`,
    first: 2,
    sortKey: 'TITLE',
    reverse: false,
    countLimit: 10000,
  };
  const codeFilterVariables = {
    query: `code:${catalogCode}`,
    first: 2,
    sortKey: 'TITLE',
    reverse: false,
    countLimit: 10000,
  };
  const codeDetailVariables = { id: codeId, code: catalogCode };
  const automaticDetailVariables = { id: automaticId };

  const [emptyRead, nonEmptyRead, statusFilterRead, codeFilterRead, codeDetailRead, automaticDetailRead] =
    await Promise.all([
      runGraphqlRequest(catalogDocument, emptyVariables),
      runGraphqlRequest(catalogDocument, nonEmptyVariables),
      runGraphqlRequest(catalogDocument, statusFilterVariables),
      runGraphqlRequest(catalogDocument, codeFilterVariables),
      runGraphqlRequest(codeDetailDocument, codeDetailVariables),
      runGraphqlRequest(automaticDetailDocument, automaticDetailVariables),
    ]);

  for (const [label, result] of [
    ['empty catalog read', emptyRead],
    ['non-empty catalog read', nonEmptyRead],
    ['status-filter catalog read', statusFilterRead],
    ['code-filter catalog read', codeFilterRead],
    ['code detail read', codeDetailRead],
    ['automatic detail read', automaticDetailRead],
  ] as const) {
    assertGraphqlSuccess(result, label);
  }

  const fixtures = {
    'discount-catalog-empty-read.json': catalogFixture(emptyVariables, emptyRead),
    'discount-catalog-non-empty-read.json': catalogFixture(nonEmptyVariables, nonEmptyRead),
    'discount-catalog-status-filter-read.json': catalogFixture(statusFilterVariables, statusFilterRead),
    'discount-catalog-code-filter-empty-read.json': catalogFixture(codeFilterVariables, codeFilterRead),
    'discount-code-basic-detail-read.json': {
      variables: codeDetailVariables,
      create: codeCreate.payload,
      response: codeDetailRead.payload,
      upstreamCalls: [recordedCall(codeDetailDocument, codeDetailVariables, codeDetailRead)],
    },
    'discount-automatic-basic-detail-read.json': {
      variables: automaticDetailVariables,
      create: automaticCreate.payload,
      response: automaticDetailRead.payload,
      upstreamCalls: [recordedCall(automaticDetailDocument, automaticDetailVariables, automaticDetailRead)],
    },
  };

  await Promise.all(
    Object.entries(fixtures).map(([filename, fixture]) =>
      writeFile(path.join(outputDir, filename), `${JSON.stringify(fixture, null, 2)}\n`, 'utf8'),
    ),
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputDir,
        files: Object.keys(fixtures),
        codeId,
        automaticId,
      },
      null,
      2,
    ),
  );
} finally {
  const cleanupResults: Array<Promise<ConformanceGraphqlResult>> = [];
  if (codeId !== null) {
    cleanupResults.push(runGraphqlRequest(codeDeleteDocument, { id: codeId }));
  }
  if (automaticId !== null) {
    cleanupResults.push(runGraphqlRequest(automaticDeleteDocument, { id: automaticId }));
  }
  await Promise.allSettled(cleanupResults);
}
