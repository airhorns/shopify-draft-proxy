/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

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
const initialCode = `HAR193LIFE${runId}`;
const updatedCode = `HAR193LIVE${runId}`;
const startsAt = new Date(Date.now() - 60_000).toISOString();

const lifecycleSelection = `#graphql
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
  mutation DiscountCodeBasicLifecycleCreate($input: DiscountCodeBasicInput!) {
    discountCodeBasicCreate(basicCodeDiscount: $input) {
      ${lifecycleSelection}
    }
  }
`;

const updateDocument = `#graphql
  mutation DiscountCodeBasicLifecycleUpdate($id: ID!, $input: DiscountCodeBasicInput!) {
    discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
      ${lifecycleSelection}
    }
  }
`;

const deactivateDocument = `#graphql
  mutation DiscountCodeBasicLifecycleDeactivate($id: ID!) {
    discountCodeDeactivate(id: $id) {
      ${lifecycleSelection}
    }
  }
`;

const activateDocument = `#graphql
  mutation DiscountCodeBasicLifecycleActivate($id: ID!) {
    discountCodeActivate(id: $id) {
      ${lifecycleSelection}
    }
  }
`;

const deleteDocument = `#graphql
  mutation DiscountCodeBasicLifecycleDelete($id: ID!) {
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

const readDocument = `#graphql
  query DiscountCodeBasicLifecycleRead($id: ID!, $code: String!) {
    discountNode(id: $id) {
      id
      discount {
        __typename
        ... on DiscountCodeBasic {
          title
          status
        }
      }
    }
    codeDiscountNodeByCode(code: $code) {
      id
    }
    discountNodes(first: 5, query: "status:active") {
      nodes {
        id
      }
    }
    discountNodesCount(query: "status:active") {
      count
      precision
    }
  }
`;

const createVariables = {
  input: {
    title: `HAR-193 lifecycle ${runId}`,
    code: initialCode,
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
  },
};

const create = await runGraphqlRaw(createDocument, createVariables);
const discountId = (
  create.payload as {
    data?: { discountCodeBasicCreate?: { codeDiscountNode?: { id?: unknown } } };
  }
).data?.discountCodeBasicCreate?.codeDiscountNode?.id;

if (typeof discountId !== 'string') {
  throw new Error(`Discount lifecycle create did not return an id: ${JSON.stringify(create)}`);
}

const updateVariables = {
  id: discountId,
  input: {
    ...createVariables.input,
    title: `HAR-193 lifecycle updated ${runId}`,
    code: updatedCode,
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
        greaterThanOrEqualToSubtotal: '2.00',
      },
    },
  },
};

const update = await runGraphqlRaw(updateDocument, updateVariables);
const readAfterUpdate = await runGraphqlRaw(readDocument, { id: discountId, code: updatedCode });
const deactivate = await runGraphqlRaw(deactivateDocument, { id: discountId });
const readAfterDeactivate = await runGraphqlRaw(readDocument, { id: discountId, code: updatedCode });
const activate = await runGraphqlRaw(activateDocument, { id: discountId });
const readAfterActivate = await runGraphqlRaw(readDocument, { id: discountId, code: updatedCode });
const cleanup = await runGraphqlRaw(deleteDocument, { id: discountId });
const readAfterDelete = await runGraphqlRaw(readDocument, { id: discountId, code: updatedCode });

const output = {
  variables: {
    id: discountId,
    initialCode,
    updatedCode,
  },
  requests: {
    create: { query: createDocument, variables: createVariables },
    update: { query: updateDocument, variables: updateVariables },
    deactivate: { query: deactivateDocument, variables: { id: discountId } },
    activate: { query: activateDocument, variables: { id: discountId } },
    delete: { query: deleteDocument, variables: { id: discountId } },
    read: { query: readDocument },
  },
  scopeProbe,
  create,
  update,
  readAfterUpdate,
  deactivate,
  readAfterDeactivate,
  activate,
  readAfterActivate,
  cleanup,
  readAfterDelete,
};

const outputPath = path.join(outputDir, 'discount-code-basic-lifecycle.json');
await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputPath,
      discountId,
      initialCode,
      updatedCode,
    },
    null,
    2,
  ),
);
