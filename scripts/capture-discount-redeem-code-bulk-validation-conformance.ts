/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
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
const seedCode = `DRAFTBASE${runId}`;
const crossDiscountCode = `DRAFTCROSS${runId}`;
const existingFreshCode = `DRAFTFRESH${runId}`;
const duplicateCode = `DRAFTDUP${runId}`;
const validCode = `DRAFTOK${runId}`;
const newlineCode = `DRAFTNL${runId}\nBAD`;
const carriageReturnCode = `DRAFTCR${runId}\rBAD`;
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

const existingConflictReadDocument = `#graphql
  query DiscountRedeemCodeBulkValidationExistingRead(
    $discountId: ID!
    $sameDiscountCode: String!
    $crossDiscountCode: String!
    $freshCode: String!
  ) {
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
    sameDiscount: codeDiscountNodeByCode(code: $sameDiscountCode) {
      id
    }
    crossDiscount: codeDiscountNodeByCode(code: $crossDiscountCode) {
      id
    }
    fresh: codeDiscountNodeByCode(code: $freshCode) {
      id
    }
  }
`;

// The shop-wide uniqueness probe the proxy forwards while validating
// `discountRedeemCodeBulkAdd` codes. Sharing the exact `.graphql` document the
// runtime emits (`DISCOUNT_UNIQUENESS_QUERY`) keeps the recorded cassette entry
// byte-identical to the proxy's forwarded query so it matches during parity.
const uniquenessCheckDocument = await readFile(
  'config/parity-requests/discounts/discount-uniqueness-check.graphql',
  'utf8',
);

const createVariables = {
  input: {
    title: `Conformance redeem code validation ${runId}`,
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

const crossCreateVariables = {
  input: {
    ...createVariables.input,
    title: `Conformance redeem code cross discount ${runId}`,
    code: crossDiscountCode,
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
let crossDiscountId: string | null = null;
let cleanup: unknown = null;
let crossCleanup: unknown = null;

try {
  const create = await runGraphqlRaw(createDocument, createVariables);
  discountId = readCreatedDiscountId(create);
  if (!discountId) {
    throw new Error(`discountCodeBasicCreate did not return a code discount id: ${JSON.stringify(create)}`);
  }

  const crossCreate = await runGraphqlRaw(createDocument, crossCreateVariables);
  crossDiscountId = readCreatedDiscountId(crossCreate);
  if (!crossDiscountId) {
    throw new Error(`cross discountCodeBasicCreate did not return a code discount id: ${JSON.stringify(crossCreate)}`);
  }

  const unknownVariables = {
    discountId: unknownDiscountId,
    codes: [{ code: 'ABC' }],
  };
  const tooManyVariables = {
    discountId,
    codes: Array.from({ length: 251 }, (_, index) => ({ code: `DRAFTMAX${runId}-${index}` })),
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
  const existingConflictVariables = {
    discountId,
    codes: [{ code: seedCode }, { code: crossDiscountCode }, { code: existingFreshCode }],
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
  const crossCodeUniquenessCheck = await runGraphqlRaw(uniquenessCheckDocument, { code: crossDiscountCode });
  const freshCodeUniquenessCheck = await runGraphqlRaw(uniquenessCheckDocument, { code: existingFreshCode });
  const existingConflictAdd = await runGraphqlRaw(addDocument, existingConflictVariables);
  const existingConflictBulkCreationId = readBulkCreationId(existingConflictAdd);
  if (!existingConflictBulkCreationId) {
    throw new Error(`existing-conflict add did not return bulk creation id: ${JSON.stringify(existingConflictAdd)}`);
  }
  const existingConflictFinalRead = await waitForBulkCreationDone(existingConflictBulkCreationId);
  const existingConflictReadAfter = await runGraphqlRaw(existingConflictReadDocument, {
    discountId,
    sameDiscountCode: seedCode,
    crossDiscountCode,
    freshCode: existingFreshCode,
  });

  crossCleanup = await runGraphqlRaw(deleteDiscountDocument, { id: crossDiscountId });
  cleanup = await runGraphqlRaw(deleteDiscountDocument, { id: discountId });

  const output = {
    variables: {
      discountId,
      crossDiscountId,
      seedCode,
      crossDiscountCode,
      existingFreshCode,
      unknownDiscountId,
      duplicateCode,
      validCode,
      newlineCode,
      carriageReturnCode,
      longCode,
      invalidBulkCreationId,
      existingConflictBulkCreationId,
    },
    requests: {
      create: { query: createDocument, variables: createVariables },
      crossCreate: { query: createDocument, variables: crossCreateVariables },
      unknown: { query: addDocument, variables: unknownVariables },
      tooMany: { query: addDocument, variables: tooManyVariables },
      emptyCodes: { query: addDocument, variables: emptyCodesVariables },
      invalidCodes: { query: addDocument, variables: invalidCodesVariables },
      invalidCodesFinalRead: { query: creationReadDocument, variables: { id: invalidBulkCreationId } },
      invalidCodesReadAfter: { query: readDocument, variables: { discountId, duplicateCode, validCode } },
      crossCodeUniquenessCheck: { query: uniquenessCheckDocument, variables: { code: crossDiscountCode } },
      freshCodeUniquenessCheck: { query: uniquenessCheckDocument, variables: { code: existingFreshCode } },
      existingCodeConflicts: { query: addDocument, variables: existingConflictVariables },
      existingCodeConflictsFinalRead: {
        query: creationReadDocument,
        variables: { id: existingConflictBulkCreationId },
      },
      existingCodeConflictsReadAfter: {
        query: existingConflictReadDocument,
        variables: { discountId, sameDiscountCode: seedCode, crossDiscountCode, freshCode: existingFreshCode },
      },
      crossCleanup: { query: deleteDiscountDocument, variables: { id: crossDiscountId } },
      cleanup: { query: deleteDiscountDocument, variables: { id: discountId } },
    },
    scopeProbe,
    create,
    crossCreate,
    unknown,
    tooMany,
    emptyCodes,
    invalidCodes: {
      add: invalidCodesAdd,
      finalRead: invalidCodesFinalRead,
      readAfter: invalidCodesReadAfter,
    },
    existingCodeConflicts: {
      add: existingConflictAdd,
      finalRead: existingConflictFinalRead,
      readAfter: existingConflictReadAfter,
    },
    crossCleanup,
    cleanup,
    // The proxy resolves shop-wide code uniqueness during bulk-add validation by
    // forwarding a `codeDiscountNodeByCode` lookup per candidate code. The cross
    // discount's code resolves to its node (TAKEN); a fresh code resolves to null
    // (importable). Both responses are captured live above; the query text is the
    // shared document the runtime forwards so the cassette matches byte-for-byte.
    // The base code and the in-batch/format-invalid codes are decided locally and
    // never forwarded, so no other uniqueness entries are recorded.
    upstreamCalls: [
      {
        operationName: 'DiscountUniquenessCheck',
        variables: { code: crossDiscountCode },
        query: uniquenessCheckDocument,
        response: {
          status: 200,
          body: crossCodeUniquenessCheck.payload,
        },
      },
      {
        operationName: 'DiscountUniquenessCheck',
        variables: { code: existingFreshCode },
        query: uniquenessCheckDocument,
        response: {
          status: 200,
          body: freshCodeUniquenessCheck.payload,
        },
      },
    ],
  };

  const outputPath = path.join(outputDir, 'discount-redeem-code-bulk-add-validation.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        discountId,
        crossDiscountId,
        invalidBulkCreationId,
        existingConflictBulkCreationId,
      },
      null,
      2,
    ),
  );
} catch (error) {
  if (crossDiscountId && !crossCleanup) {
    try {
      crossCleanup = await runGraphqlRaw(deleteDiscountDocument, { id: crossDiscountId });
      console.error(`Cleaned up ${crossDiscountId} after capture failure: ${JSON.stringify(crossCleanup)}`);
    } catch (cleanupError) {
      console.error(`Failed to clean up ${crossDiscountId}: ${String(cleanupError)}`);
    }
  }
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
