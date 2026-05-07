/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-activate-deactivate-noop-idempotence.json');
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
const startsAt = new Date(Date.now() - 120_000).toISOString();
const activeCode = `NOOPACT${runId}`;
const expiredCode = `NOOPEXP${runId}`;
const createdDiscountIds: string[] = [];

const discountFields = `#graphql
  __typename
  startsAt
  endsAt
  status
  updatedAt
`;

const codeSelection = `#graphql
  codeDiscountNode {
    id
    codeDiscount {
      __typename
      ... on DiscountCodeBasic {
        ${discountFields}
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

const automaticSelection = `#graphql
  automaticDiscountNode {
    id
    automaticDiscount {
      __typename
      ... on DiscountAutomaticBasic {
        ${discountFields}
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

const codeCreateDocument = `#graphql
  mutation DiscountCodeNoopSetupCreate($input: DiscountCodeBasicInput!) {
    discountCodeBasicCreate(basicCodeDiscount: $input) {
      ${codeSelection}
    }
  }
`;

const automaticCreateDocument = `#graphql
  mutation DiscountAutomaticNoopSetupCreate($input: DiscountAutomaticBasicInput!) {
    discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
      ${automaticSelection}
    }
  }
`;

const codeActivateDocument = `#graphql
  mutation DiscountCodeActivateNoopIdempotence($id: ID!) {
    discountCodeActivate(id: $id) {
      ${codeSelection}
    }
  }
`;

const codeDeactivateDocument = `#graphql
  mutation DiscountCodeDeactivateNoopIdempotence($id: ID!) {
    discountCodeDeactivate(id: $id) {
      ${codeSelection}
    }
  }
`;

const automaticActivateDocument = `#graphql
  mutation DiscountAutomaticActivateNoopIdempotence($id: ID!) {
    discountAutomaticActivate(id: $id) {
      ${automaticSelection}
    }
  }
`;

const automaticDeactivateDocument = `#graphql
  mutation DiscountAutomaticDeactivateNoopIdempotence($id: ID!) {
    discountAutomaticDeactivate(id: $id) {
      ${automaticSelection}
    }
  }
`;

const codeDeleteDocument = `#graphql
  mutation DiscountCodeNoopCleanupDelete($id: ID!) {
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
  mutation DiscountAutomaticNoopCleanupDelete($id: ID!) {
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

const discountHydrateDocument = `#graphql
  query DiscountHydrate($id: ID!) {
    codeNode: codeDiscountNode(id: $id) {
      id
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          status
          startsAt
          endsAt
          updatedAt
          codes(first: 250) {
            nodes {
              id
              code
            }
          }
        }
        ... on DiscountCodeApp {
          title
          status
          startsAt
          endsAt
          updatedAt
        }
        ... on DiscountCodeBxgy {
          title
          status
          startsAt
          endsAt
          updatedAt
        }
        ... on DiscountCodeFreeShipping {
          title
          status
          startsAt
          endsAt
          updatedAt
        }
      }
    }
    automaticNode: automaticDiscountNode(id: $id) {
      id
      automaticDiscount {
        __typename
        ... on DiscountAutomaticBasic {
          title
          status
          startsAt
          endsAt
          updatedAt
        }
        ... on DiscountAutomaticApp {
          title
          status
          startsAt
          endsAt
          updatedAt
        }
        ... on DiscountAutomaticBxgy {
          title
          status
          startsAt
          endsAt
          updatedAt
        }
        ... on DiscountAutomaticFreeShipping {
          title
          status
          startsAt
          endsAt
          updatedAt
        }
      }
    }
  }
`;

function assertSuccess(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
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

function readRecord(result: ConformanceGraphqlResult, pathSegments: string[], context: string): JsonRecord {
  const value = readPath(result.payload, pathSegments);
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(
      `${context} did not return object ${pathSegments.join('.')}: ${JSON.stringify(result.payload, null, 2)}`,
    );
  }
  return value as JsonRecord;
}

function stableDiscountFields(result: ConformanceGraphqlResult, pathSegments: string[], context: string): JsonRecord {
  const discount = readRecord(result, pathSegments, context);
  return {
    startsAt: discount['startsAt'] ?? null,
    endsAt: discount['endsAt'] ?? null,
    status: discount['status'] ?? null,
    updatedAt: discount['updatedAt'] ?? null,
  };
}

function assertStableNoop(
  before: ConformanceGraphqlResult,
  after: ConformanceGraphqlResult,
  beforePathSegments: string[],
  afterPathSegments: string[],
  context: string,
): void {
  const beforeFields = stableDiscountFields(before, beforePathSegments, `${context} before`);
  const afterFields = stableDiscountFields(after, afterPathSegments, `${context} after`);
  if (JSON.stringify(beforeFields) !== JSON.stringify(afterFields)) {
    throw new Error(
      `${context} changed stable fields on no-op transition: ${JSON.stringify({ beforeFields, afterFields }, null, 2)}`,
    );
  }
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function cleanupDiscounts(): Promise<void> {
  for (const id of createdDiscountIds.slice().reverse()) {
    try {
      if (id.includes('/DiscountCodeNode/')) {
        await runGraphqlRaw(codeDeleteDocument, { id });
      } else {
        await runGraphqlRaw(automaticDeleteDocument, { id });
      }
    } catch (error) {
      console.error(`cleanup failed for ${id}: ${(error as Error).message}`);
    }
  }
}

function codeInput(title: string, code: string): JsonRecord {
  return {
    title,
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
  };
}

function automaticInput(title: string): JsonRecord {
  return {
    title,
    startsAt,
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
  };
}

try {
  const activeCodeCreate = await runGraphqlRaw(codeCreateDocument, {
    input: codeInput(`No-op active code ${runId}`, activeCode),
  });
  assertSuccess(activeCodeCreate, 'active code create');
  const activeCodeId = readRequiredString(
    activeCodeCreate,
    ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id'],
    'active code create',
  );
  createdDiscountIds.push(activeCodeId);

  const expiredCodeCreate = await runGraphqlRaw(codeCreateDocument, {
    input: codeInput(`No-op expired code ${runId}`, expiredCode),
  });
  assertSuccess(expiredCodeCreate, 'expired code create');
  const expiredCodeId = readRequiredString(
    expiredCodeCreate,
    ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id'],
    'expired code create',
  );
  createdDiscountIds.push(expiredCodeId);

  const activeAutomaticCreate = await runGraphqlRaw(automaticCreateDocument, {
    input: automaticInput(`No-op active automatic ${runId}`),
  });
  assertSuccess(activeAutomaticCreate, 'active automatic create');
  const activeAutomaticId = readRequiredString(
    activeAutomaticCreate,
    ['data', 'discountAutomaticBasicCreate', 'automaticDiscountNode', 'id'],
    'active automatic create',
  );
  createdDiscountIds.push(activeAutomaticId);

  const expiredAutomaticCreate = await runGraphqlRaw(automaticCreateDocument, {
    input: automaticInput(`No-op expired automatic ${runId}`),
  });
  assertSuccess(expiredAutomaticCreate, 'expired automatic create');
  const expiredAutomaticId = readRequiredString(
    expiredAutomaticCreate,
    ['data', 'discountAutomaticBasicCreate', 'automaticDiscountNode', 'id'],
    'expired automatic create',
  );
  createdDiscountIds.push(expiredAutomaticId);

  const firstCodeDeactivate = await runGraphqlRaw(codeDeactivateDocument, { id: expiredCodeId });
  assertSuccess(firstCodeDeactivate, 'first code deactivate');
  const firstAutomaticDeactivate = await runGraphqlRaw(automaticDeactivateDocument, { id: expiredAutomaticId });
  assertSuccess(firstAutomaticDeactivate, 'first automatic deactivate');

  await sleep(1_500);

  const codeActivateNoop = await runGraphqlRaw(codeActivateDocument, { id: activeCodeId });
  assertSuccess(codeActivateNoop, 'code activate no-op');
  const codeDeactivateNoop = await runGraphqlRaw(codeDeactivateDocument, { id: expiredCodeId });
  assertSuccess(codeDeactivateNoop, 'code deactivate no-op');
  const automaticActivateNoop = await runGraphqlRaw(automaticActivateDocument, { id: activeAutomaticId });
  assertSuccess(automaticActivateNoop, 'automatic activate no-op');
  const automaticDeactivateNoop = await runGraphqlRaw(automaticDeactivateDocument, { id: expiredAutomaticId });
  assertSuccess(automaticDeactivateNoop, 'automatic deactivate no-op');

  assertStableNoop(
    activeCodeCreate,
    codeActivateNoop,
    ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'codeDiscount'],
    ['data', 'discountCodeActivate', 'codeDiscountNode', 'codeDiscount'],
    'active code activate',
  );
  assertStableNoop(
    firstCodeDeactivate,
    codeDeactivateNoop,
    ['data', 'discountCodeDeactivate', 'codeDiscountNode', 'codeDiscount'],
    ['data', 'discountCodeDeactivate', 'codeDiscountNode', 'codeDiscount'],
    'expired code deactivate',
  );
  assertStableNoop(
    activeAutomaticCreate,
    automaticActivateNoop,
    ['data', 'discountAutomaticBasicCreate', 'automaticDiscountNode', 'automaticDiscount'],
    ['data', 'discountAutomaticActivate', 'automaticDiscountNode', 'automaticDiscount'],
    'active automatic activate',
  );
  assertStableNoop(
    firstAutomaticDeactivate,
    automaticDeactivateNoop,
    ['data', 'discountAutomaticDeactivate', 'automaticDiscountNode', 'automaticDiscount'],
    ['data', 'discountAutomaticDeactivate', 'automaticDiscountNode', 'automaticDiscount'],
    'expired automatic deactivate',
  );

  const hydrateActiveCode = await runGraphqlRaw(discountHydrateDocument, { id: activeCodeId });
  const hydrateExpiredCode = await runGraphqlRaw(discountHydrateDocument, { id: expiredCodeId });
  const hydrateActiveAutomatic = await runGraphqlRaw(discountHydrateDocument, { id: activeAutomaticId });
  const hydrateExpiredAutomatic = await runGraphqlRaw(discountHydrateDocument, { id: expiredAutomaticId });

  const output = {
    storeDomain,
    apiVersion,
    variables: {
      activeCodeId,
      expiredCodeId,
      activeAutomaticId,
      expiredAutomaticId,
      activeCode,
      expiredCode,
    },
    requests: {
      codeActivateNoop: { query: codeActivateDocument, variables: { id: activeCodeId } },
      codeDeactivateNoop: { query: codeDeactivateDocument, variables: { id: expiredCodeId } },
      automaticActivateNoop: { query: automaticActivateDocument, variables: { id: activeAutomaticId } },
      automaticDeactivateNoop: { query: automaticDeactivateDocument, variables: { id: expiredAutomaticId } },
    },
    scopeProbe,
    setup: {
      activeCodeCreate,
      expiredCodeCreate,
      activeAutomaticCreate,
      expiredAutomaticCreate,
      firstCodeDeactivate,
      firstAutomaticDeactivate,
    },
    codeActivateNoop,
    codeDeactivateNoop,
    automaticActivateNoop,
    automaticDeactivateNoop,
    upstreamCalls: [
      {
        operationName: 'DiscountHydrate',
        variables: { id: activeCodeId },
        query: discountHydrateDocument,
        response: { status: hydrateActiveCode.status, body: hydrateActiveCode.payload },
      },
      {
        operationName: 'DiscountHydrate',
        variables: { id: expiredCodeId },
        query: discountHydrateDocument,
        response: { status: hydrateExpiredCode.status, body: hydrateExpiredCode.payload },
      },
      {
        operationName: 'DiscountHydrate',
        variables: { id: activeAutomaticId },
        query: discountHydrateDocument,
        response: { status: hydrateActiveAutomatic.status, body: hydrateActiveAutomatic.payload },
      },
      {
        operationName: 'DiscountHydrate',
        variables: { id: expiredAutomaticId },
        query: discountHydrateDocument,
        response: { status: hydrateExpiredAutomatic.status, body: hydrateExpiredAutomatic.payload },
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
        activeCodeId,
        expiredCodeId,
        activeAutomaticId,
        expiredAutomaticId,
      },
      null,
      2,
    ),
  );
} finally {
  await cleanupDiscounts();
}
