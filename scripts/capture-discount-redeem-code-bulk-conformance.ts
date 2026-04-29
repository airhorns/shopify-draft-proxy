/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
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
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const runId = Date.now();
const seedCode = `HAR438BASE${runId}`;
const addedCode = `HAR438ADD${runId}`;
const secondAddedCode = `HAR438PLUS${runId}`;
const lowerAddedCode = addedCode.toLowerCase();
const startsAt = new Date(Date.now() - 60_000).toISOString();

const createDocument = `#graphql
  mutation DiscountRedeemCodeBulkSeedCreate($input: DiscountCodeBasicInput!) {
    discountCodeBasicCreate(basicCodeDiscount: $input) {
      codeDiscountNode {
        id
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

const deleteDiscountDocument = `#graphql
  mutation DiscountRedeemCodeBulkSeedDelete($id: ID!) {
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

const addDocument = `#graphql
  mutation DiscountRedeemCodeBulkLiveAdd($discountId: ID!, $codes: [DiscountRedeemCodeInput!]!) {
    discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) {
      bulkCreation {
        done
        codesCount
        importedCount
        failedCount
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

const deleteRedeemCodesDocument = `#graphql
  mutation DiscountRedeemCodeBulkLiveDelete($discountId: ID!, $ids: [ID!]!) {
    discountCodeRedeemCodeBulkDelete(discountId: $discountId, ids: $ids) {
      job {
        done
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

const readDocument = `#graphql
  query DiscountRedeemCodeBulkLiveRead(
    $id: ID!
    $exactAddedCode: String!
    $lowerAddedCode: String!
    $removedCode: String!
  ) {
    codeDiscountNode(id: $id) {
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
          codes(first: 10) {
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
          codesCount {
            count
            precision
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
          }
        }
      }
    }
    exactAdded: codeDiscountNodeByCode(code: $exactAddedCode) {
      id
    }
    lowerAdded: codeDiscountNodeByCode(code: $lowerAddedCode) {
      id
    }
    removed: codeDiscountNodeByCode(code: $removedCode) {
      id
    }
  }
`;

const createVariables = {
  input: {
    title: `HAR-438 redeem code bulk ${runId}`,
    code: seedCode,
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
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  },
};

const readVariables = {
  id: '',
  exactAddedCode: addedCode,
  lowerAddedCode,
  removedCode: seedCode,
};

function readCreatedDiscountId(response: unknown): string | null {
  const id = (
    response as { payload?: { data?: { discountCodeBasicCreate?: { codeDiscountNode?: { id?: unknown } } } } }
  ).payload?.data?.discountCodeBasicCreate?.codeDiscountNode?.id;
  return typeof id === 'string' ? id : null;
}

function readSeedRedeemCodeId(response: unknown): string | null {
  const nodes = (
    response as {
      payload?: {
        data?: {
          codeDiscountNode?: {
            codeDiscount?: { codes?: { nodes?: Array<{ id?: unknown; code?: unknown }> } };
          };
        };
      };
    }
  ).payload?.data?.codeDiscountNode?.codeDiscount?.codes?.nodes;
  const seedNode = nodes?.find((node) => node.code === seedCode);
  return typeof seedNode?.id === 'string' ? seedNode.id : null;
}

function hasLookup(response: unknown, alias: 'lowerAdded' | 'removed'): boolean {
  const node = (response as { payload?: { data?: Record<'lowerAdded' | 'removed', { id?: unknown } | null> } }).payload
    ?.data?.[alias];
  return typeof node?.id === 'string';
}

async function waitForLookup(variables: typeof readVariables, alias: 'lowerAdded' | 'removed', expected: boolean) {
  let lastResponse: unknown = null;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    lastResponse = await runGraphqlRaw(readDocument, variables);
    if (hasLookup(lastResponse, alias) === expected) {
      return lastResponse;
    }
    await sleep(500);
  }
  return lastResponse;
}

let discountId: string | null = null;
let cleanup: unknown = null;

try {
  const create = await runGraphqlRaw(createDocument, createVariables);
  discountId = readCreatedDiscountId(create);
  if (!discountId) {
    throw new Error(`discountCodeBasicCreate did not return a code discount id: ${JSON.stringify(create)}`);
  }

  const concreteReadVariables = { ...readVariables, id: discountId };
  const initialRead = await waitForLookup(concreteReadVariables, 'removed', true);
  const seedRedeemCodeId = readSeedRedeemCodeId(initialRead);
  if (!seedRedeemCodeId) {
    throw new Error(`Initial read did not return a seed redeem-code id: ${JSON.stringify(initialRead)}`);
  }

  const addVariables = {
    discountId,
    codes: [{ code: addedCode }, { code: secondAddedCode }],
  };
  const add = await runGraphqlRaw(addDocument, addVariables);
  const readAfterAdd = await waitForLookup(concreteReadVariables, 'lowerAdded', true);

  const deleteVariables = {
    discountId,
    ids: [seedRedeemCodeId],
  };
  const deleteCodes = await runGraphqlRaw(deleteRedeemCodesDocument, deleteVariables);
  const readAfterDelete = await waitForLookup(concreteReadVariables, 'removed', false);

  cleanup = await runGraphqlRaw(deleteDiscountDocument, { id: discountId });

  const output = {
    variables: {
      discountId,
      seedCode,
      seedRedeemCodeId,
      addedCode,
      secondAddedCode,
      lowerAddedCode,
    },
    requests: {
      create: { query: createDocument, variables: createVariables },
      add: { query: addDocument, variables: addVariables },
      read: { query: readDocument },
      delete: { query: deleteRedeemCodesDocument, variables: deleteVariables },
      cleanup: { query: deleteDiscountDocument, variables: { id: discountId } },
    },
    seedDiscounts: [
      (
        initialRead as {
          payload?: { data?: { codeDiscountNode?: unknown } };
        }
      ).payload?.data?.codeDiscountNode,
    ].filter(Boolean),
    scopeProbe,
    create,
    initialRead,
    add,
    readAfterAdd,
    deleteCodes,
    readAfterDelete,
    cleanup,
  };

  const outputPath = path.join(outputDir, 'discount-redeem-code-bulk.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        discountId,
        seedCode,
        addedCode,
        seedRedeemCodeId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  if (discountId && !cleanup) {
    try {
      cleanup = await runGraphqlRaw(deleteDiscountDocument, { id: discountId });
      console.error(`Cleaned up ${discountId} after capture failure: ${JSON.stringify(cleanup)}`);
    } catch (cleanupError) {
      console.error(`Failed to clean up ${discountId}: ${String(cleanupError)}`);
    }
  }
  throw error;
}
