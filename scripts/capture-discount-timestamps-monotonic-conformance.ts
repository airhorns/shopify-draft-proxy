/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

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

const timestampSelection = `#graphql
  codeDiscountNode {
    id
    codeDiscount {
      __typename
      ... on DiscountCodeBasic {
        title
        createdAt
        updatedAt
        codes(first: 1) {
          nodes {
            code
          }
        }
      }
    }
  }
  userErrors {
    field
    message
    code
  }
`;

const createDocument = `#graphql
  mutation DiscountTimestampsMonotonicCreate($input: DiscountCodeBasicInput!) {
    discountCodeBasicCreate(basicCodeDiscount: $input) {
      ${timestampSelection}
    }
  }
`;

const updateDocument = `#graphql
  mutation DiscountTimestampsMonotonicUpdate($id: ID!, $input: DiscountCodeBasicInput!) {
    discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
      ${timestampSelection}
    }
  }
`;

const readDocument = `#graphql
  query DiscountTimestampsMonotonicRead(
    $firstId: ID!
    $secondId: ID!
    $firstCode: String!
    $secondCode: String!
  ) {
    first: codeDiscountNode(id: $firstId) {
      id
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          createdAt
          updatedAt
        }
      }
    }
    second: codeDiscountNode(id: $secondId) {
      id
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          createdAt
          updatedAt
        }
      }
    }
    firstByCode: codeDiscountNodeByCode(code: $firstCode) {
      id
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          createdAt
          updatedAt
        }
      }
    }
    secondByCode: codeDiscountNodeByCode(code: $secondCode) {
      id
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          createdAt
          updatedAt
        }
      }
    }
  }
`;

const deleteDocument = `#graphql
  mutation DiscountTimestampsMonotonicDelete($id: ID!) {
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

const runId = Date.now();
const firstCode = `HAR603A${runId}`;
const secondCode = `HAR603B${runId}`;
const startsAt = new Date(Date.now() - 60_000).toISOString();

function basicInput(title: string, code: string, percentage: number) {
  return {
    title,
    code,
    startsAt,
    context: {
      all: 'ALL',
    },
    customerGets: {
      value: {
        percentage,
      },
      items: {
        all: true,
      },
    },
  };
}

function readCreatedDiscountId(response: unknown, root: 'discountCodeBasicCreate' | 'discountCodeBasicUpdate') {
  return (
    response as {
      payload?: {
        data?: {
          [K in typeof root]?: { codeDiscountNode?: { id?: unknown } };
        };
      };
    }
  ).payload?.data?.[root]?.codeDiscountNode?.id;
}

const firstCreateVariables = {
  input: basicInput(`HAR-603 first ${runId}`, firstCode, 0.1),
};
const secondCreateVariables = {
  input: basicInput(`HAR-603 second ${runId}`, secondCode, 0.1),
};

let firstId: string | null = null;
let secondId: string | null = null;
let firstCreate: unknown;
let secondCreate: unknown;
let updateFirst: unknown;
let readAfterUpdate: unknown;
let firstCleanup: unknown = null;
let secondCleanup: unknown = null;

try {
  firstCreate = await runGraphqlRaw(createDocument, firstCreateVariables);
  const createdFirstId = readCreatedDiscountId(firstCreate, 'discountCodeBasicCreate');
  if (typeof createdFirstId !== 'string') {
    throw new Error(`First discount create did not return an id: ${JSON.stringify(firstCreate)}`);
  }
  firstId = createdFirstId;

  await delay(1100);

  secondCreate = await runGraphqlRaw(createDocument, secondCreateVariables);
  const createdSecondId = readCreatedDiscountId(secondCreate, 'discountCodeBasicCreate');
  if (typeof createdSecondId !== 'string') {
    throw new Error(`Second discount create did not return an id: ${JSON.stringify(secondCreate)}`);
  }
  secondId = createdSecondId;

  await delay(1100);

  const updateFirstVariables = {
    id: firstId,
    input: basicInput(`HAR-603 first updated ${runId}`, firstCode, 0.2),
  };
  updateFirst = await runGraphqlRaw(updateDocument, updateFirstVariables);
  const updatedFirstId = readCreatedDiscountId(updateFirst, 'discountCodeBasicUpdate');
  if (typeof updatedFirstId !== 'string') {
    throw new Error(`First discount update did not return an id: ${JSON.stringify(updateFirst)}`);
  }

  readAfterUpdate = await runGraphqlRaw(readDocument, {
    firstId,
    secondId,
    firstCode,
    secondCode,
  });

  firstCleanup = await runGraphqlRaw(deleteDocument, { id: firstId });
  secondCleanup = await runGraphqlRaw(deleteDocument, { id: secondId });

  const output = {
    variables: {
      firstId,
      secondId,
      firstCode,
      secondCode,
    },
    requests: {
      firstCreate: { query: createDocument, variables: firstCreateVariables },
      secondCreate: { query: createDocument, variables: secondCreateVariables },
      updateFirst: {
        query: updateDocument,
        variables: {
          id: firstId,
          input: basicInput(`HAR-603 first updated ${runId}`, firstCode, 0.2),
        },
      },
      read: { query: readDocument },
      delete: { query: deleteDocument },
    },
    scopeProbe,
    firstCreate,
    secondCreate,
    updateFirst,
    readAfterUpdate,
    firstCleanup,
    secondCleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join(outputDir, 'discount-timestamps-monotonic.json');
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        outputPath,
        firstId,
        secondId,
        firstCode,
        secondCode,
      },
      null,
      2,
    ),
  );
} catch (error) {
  if (firstId && !firstCleanup) {
    try {
      firstCleanup = await runGraphqlRaw(deleteDocument, { id: firstId });
      console.error(`Cleaned up ${firstId} after capture failure: ${JSON.stringify(firstCleanup)}`);
    } catch (cleanupError) {
      console.error(`Failed to clean up ${firstId}: ${String(cleanupError)}`);
    }
  }
  if (secondId && !secondCleanup) {
    try {
      secondCleanup = await runGraphqlRaw(deleteDocument, { id: secondId });
      console.error(`Cleaned up ${secondId} after capture failure: ${JSON.stringify(secondCleanup)}`);
    } catch (cleanupError) {
      console.error(`Failed to clean up ${secondId}: ${String(cleanupError)}`);
    }
  }
  throw error;
}
