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
const seedCode = `HAR784BASE${runId}`;
const duplicateCode = `HAR784DUP${runId}`;
const validCode = `HAR784OK${runId}`;
const newlineCode = `HAR784NL${runId}\nBAD`;
const carriageReturnCode = `HAR784CR${runId}\rBAD`;
const longCode = 'X'.repeat(256);
const startsAt = new Date(Date.now() - 60_000).toISOString();
const unknownDiscountId = 'gid://shopify/DiscountCodeNode/0';

const createDocument = `#graphql
  mutation DiscountRedeemCodeBulkValidationCreate($input: DiscountCodeBasicInput!) {
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
  mutation DiscountRedeemCodeBulkValidationDelete($id: ID!) {
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
  mutation DiscountRedeemCodeBulkValidationAdd($discountId: ID!, $codes: [DiscountRedeemCodeInput!]!) {
    discountRedeemCodeBulkAdd(discountId: $discountId, codes: $codes) {
      bulkCreation {
        id
        done
        codesCount
        importedCount
        failedCount
        codes(first: 10) {
          nodes {
            code
            errors {
              field
              message
              code
              extraInfo
            }
            discountRedeemCode {
              id
              code
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

const creationReadDocument = `#graphql
  query DiscountRedeemCodeBulkValidationCreationRead($id: ID!) {
    discountRedeemCodeBulkCreation(id: $id) {
      done
      codesCount
      importedCount
      failedCount
      codes(first: 10) {
        nodes {
          code
          errors {
            field
            message
            code
            extraInfo
          }
          discountRedeemCode {
            code
          }
        }
      }
    }
  }
`;

const readDocument = `#graphql
  query DiscountRedeemCodeBulkValidationRead($discountId: ID!, $duplicateCode: String!, $validCode: String!) {
    codeDiscountNode(id: $discountId) {
      codeDiscount {
        ... on DiscountCodeBasic {
          codes(first: 10) {
            nodes {
              code
            }
          }
          codesCount {
            count
            precision
          }
        }
      }
    }
    duplicate: codeDiscountNodeByCode(code: $duplicateCode) {
      id
    }
    valid: codeDiscountNodeByCode(code: $validCode) {
      id
    }
  }
`;

const createVariables = {
  input: {
    title: `HAR-784 redeem code validation ${runId}`,
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

function readCreatedDiscountId(response: unknown): string | null {
  const id = (
    response as { payload?: { data?: { discountCodeBasicCreate?: { codeDiscountNode?: { id?: unknown } } } } }
  ).payload?.data?.discountCodeBasicCreate?.codeDiscountNode?.id;
  return typeof id === 'string' ? id : null;
}

function readBulkCreationId(response: unknown): string | null {
  const id = (
    response as {
      payload?: { data?: { discountRedeemCodeBulkAdd?: { bulkCreation?: { id?: unknown } | null } } };
    }
  ).payload?.data?.discountRedeemCodeBulkAdd?.bulkCreation?.id;
  return typeof id === 'string' ? id : null;
}

async function waitForBulkCreationDone(id: string) {
  let lastResponse: unknown = null;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    lastResponse = await runGraphqlRaw(creationReadDocument, { id });
    const done = (lastResponse as { payload?: { data?: { discountRedeemCodeBulkCreation?: { done?: unknown } } } })
      .payload?.data?.discountRedeemCodeBulkCreation?.done;
    if (done === true) {
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

  const unknownVariables = {
    discountId: unknownDiscountId,
    codes: [{ code: 'ABC' }],
  };
  const tooManyVariables = {
    discountId,
    codes: Array.from({ length: 251 }, (_, index) => ({ code: `HAR784MAX${runId}-${index}` })),
  };
  const emptyCodesVariables = {
    discountId,
    codes: [],
  };
  const invalidCodesVariables = {
    discountId,
    codes: [
      { code: '' },
      { code: newlineCode },
      { code: carriageReturnCode },
      { code: longCode },
      { code: duplicateCode },
      { code: duplicateCode },
      { code: validCode },
    ],
  };

  const unknown = await runGraphqlRaw(addDocument, unknownVariables);
  const tooMany = await runGraphqlRaw(addDocument, tooManyVariables);
  const emptyCodes = await runGraphqlRaw(addDocument, emptyCodesVariables);
  const invalidCodesAdd = await runGraphqlRaw(addDocument, invalidCodesVariables);
  const invalidBulkCreationId = readBulkCreationId(invalidCodesAdd);
  if (!invalidBulkCreationId) {
    throw new Error(`invalid code add did not return bulk creation id: ${JSON.stringify(invalidCodesAdd)}`);
  }
  const invalidCodesFinalRead = await waitForBulkCreationDone(invalidBulkCreationId);
  const invalidCodesReadAfter = await runGraphqlRaw(readDocument, {
    discountId,
    duplicateCode,
    validCode,
  });

  cleanup = await runGraphqlRaw(deleteDiscountDocument, { id: discountId });

  const output = {
    variables: {
      discountId,
      seedCode,
      unknownDiscountId,
      duplicateCode,
      validCode,
      newlineCode,
      carriageReturnCode,
      longCode,
      invalidBulkCreationId,
    },
    requests: {
      create: { query: createDocument, variables: createVariables },
      unknown: { query: addDocument, variables: unknownVariables },
      tooMany: { query: addDocument, variables: tooManyVariables },
      emptyCodes: { query: addDocument, variables: emptyCodesVariables },
      invalidCodes: { query: addDocument, variables: invalidCodesVariables },
      invalidCodesFinalRead: { query: creationReadDocument, variables: { id: invalidBulkCreationId } },
      invalidCodesReadAfter: { query: readDocument, variables: { discountId, duplicateCode, validCode } },
      cleanup: { query: deleteDiscountDocument, variables: { id: discountId } },
    },
    scopeProbe,
    create,
    unknown,
    tooMany,
    emptyCodes,
    invalidCodes: {
      add: invalidCodesAdd,
      finalRead: invalidCodesFinalRead,
      readAfter: invalidCodesReadAfter,
    },
    cleanup,
    upstreamCalls: [
      {
        operationName: 'DiscountHydrate',
        variables: { id: unknownDiscountId },
        query: 'sha:DiscountHydrate',
        response: {
          status: 200,
          body: {
            data: {
              codeNode: null,
              automaticNode: null,
            },
          },
        },
      },
    ],
  };

  const outputPath = path.join(outputDir, 'discount-redeem-code-bulk-add-validation.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(JSON.stringify({ ok: true, apiVersion, outputPath, discountId, invalidBulkCreationId }, null, 2));
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
