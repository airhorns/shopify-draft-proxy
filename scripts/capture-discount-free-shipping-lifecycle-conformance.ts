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

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
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
const startsAt = new Date(Date.now() - 60_000).toISOString();
const initialCode = `HAR196FREE${runId}`;
const updatedCode = `HAR196SHIP${runId}`;

const codeSelection = `#graphql
  codeDiscountNode {
    id
    codeDiscount {
      __typename
      ... on DiscountCodeFreeShipping {
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
        destinationSelection {
          __typename
          ... on DiscountCountryAll {
            allCountries
          }
          ... on DiscountCountries {
            countries
            includeRestOfWorld
          }
        }
        maximumShippingPrice {
          amount
          currencyCode
        }
        appliesOncePerCustomer
        appliesOnOneTimePurchase
        appliesOnSubscription
        recurringCycleLimit
        usageLimit
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
      ... on DiscountAutomaticFreeShipping {
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
        destinationSelection {
          __typename
          ... on DiscountCountryAll {
            allCountries
          }
          ... on DiscountCountries {
            countries
            includeRestOfWorld
          }
        }
        maximumShippingPrice {
          amount
          currencyCode
        }
        appliesOnOneTimePurchase
        appliesOnSubscription
        recurringCycleLimit
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
  mutation DiscountCodeFreeShippingLifecycleCreate($input: DiscountCodeFreeShippingInput!) {
    discountCodeFreeShippingCreate(freeShippingCodeDiscount: $input) {
      ${codeSelection}
    }
  }
`;

const codeUpdateDocument = `#graphql
  mutation DiscountCodeFreeShippingLifecycleUpdate($id: ID!, $input: DiscountCodeFreeShippingInput!) {
    discountCodeFreeShippingUpdate(id: $id, freeShippingCodeDiscount: $input) {
      ${codeSelection}
    }
  }
`;

const codeDeactivateDocument = `#graphql
  mutation DiscountCodeFreeShippingLifecycleDeactivate($id: ID!) {
    discountCodeDeactivate(id: $id) {
      ${codeSelection}
    }
  }
`;

const codeActivateDocument = `#graphql
  mutation DiscountCodeFreeShippingLifecycleActivate($id: ID!) {
    discountCodeActivate(id: $id) {
      ${codeSelection}
    }
  }
`;

const codeDeleteDocument = `#graphql
  mutation DiscountCodeFreeShippingLifecycleDelete($id: ID!) {
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

const automaticCreateDocument = `#graphql
  mutation DiscountAutomaticFreeShippingLifecycleCreate($input: DiscountAutomaticFreeShippingInput!) {
    discountAutomaticFreeShippingCreate(freeShippingAutomaticDiscount: $input) {
      ${automaticSelection}
    }
  }
`;

const automaticUpdateDocument = `#graphql
  mutation DiscountAutomaticFreeShippingLifecycleUpdate($id: ID!, $input: DiscountAutomaticFreeShippingInput!) {
    discountAutomaticFreeShippingUpdate(id: $id, freeShippingAutomaticDiscount: $input) {
      ${automaticSelection}
    }
  }
`;

const automaticDeactivateDocument = `#graphql
  mutation DiscountAutomaticFreeShippingLifecycleDeactivate($id: ID!) {
    discountAutomaticDeactivate(id: $id) {
      ${automaticSelection}
    }
  }
`;

const automaticActivateDocument = `#graphql
  mutation DiscountAutomaticFreeShippingLifecycleActivate($id: ID!) {
    discountAutomaticActivate(id: $id) {
      ${automaticSelection}
    }
  }
`;

const automaticDeleteDocument = `#graphql
  mutation DiscountAutomaticFreeShippingLifecycleDelete($id: ID!) {
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

const readDocument = `#graphql
  query DiscountFreeShippingLifecycleRead($codeId: ID!, $automaticId: ID!, $code: String!) {
    discountNode(id: $codeId) {
      id
      discount {
        __typename
        ... on DiscountCodeFreeShipping {
          title
          status
        }
      }
    }
    codeDiscountNodeByCode(code: $code) {
      id
    }
    automaticDiscountNode(id: $automaticId) {
      id
      automaticDiscount {
        __typename
        ... on DiscountAutomaticFreeShipping {
          title
          status
        }
      }
    }
    discountNodes(first: 5, query: "type:free_shipping") {
      nodes {
        id
      }
    }
    discountNodesCount(query: "type:free_shipping") {
      count
      precision
    }
  }
`;

const codeCreateVariables = {
  input: {
    title: `HAR-196 code free shipping ${runId}`,
    code: initialCode,
    startsAt,
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '10.00',
      },
    },
    destination: {
      all: true,
    },
    maximumShippingPrice: '25.00',
    appliesOncePerCustomer: true,
    appliesOnOneTimePurchase: true,
    appliesOnSubscription: false,
    recurringCycleLimit: 1,
    usageLimit: 5,
  },
};

const codeCreate = await runGraphqlRaw(codeCreateDocument, codeCreateVariables);
const codeDiscountId = (
  codeCreate.payload as {
    data?: { discountCodeFreeShippingCreate?: { codeDiscountNode?: { id?: unknown } } };
  }
).data?.discountCodeFreeShippingCreate?.codeDiscountNode?.id;

if (typeof codeDiscountId !== 'string') {
  throw new Error(`Code free-shipping create did not return an id: ${JSON.stringify(codeCreate)}`);
}

const codeUpdateVariables = {
  id: codeDiscountId,
  input: {
    title: `HAR-196 code free shipping updated ${runId}`,
    code: updatedCode,
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
        greaterThanOrEqualToSubtotal: '12.00',
      },
    },
    destination: {
      countries: {
        add: ['US', 'CA'],
        includeRestOfWorld: false,
      },
    },
    maximumShippingPrice: '30.00',
    appliesOncePerCustomer: false,
    appliesOnOneTimePurchase: false,
    appliesOnSubscription: true,
    recurringCycleLimit: 2,
    usageLimit: 10,
  },
};

const automaticCreateVariables = {
  input: {
    title: `HAR-196 automatic free shipping ${runId}`,
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
        greaterThanOrEqualToSubtotal: '15.00',
      },
    },
    destination: {
      all: true,
    },
    maximumShippingPrice: '20.00',
    appliesOnOneTimePurchase: true,
    appliesOnSubscription: false,
    recurringCycleLimit: 1,
  },
};

const automaticCreate = await runGraphqlRaw(automaticCreateDocument, automaticCreateVariables);
const automaticDiscountId = (
  automaticCreate.payload as {
    data?: { discountAutomaticFreeShippingCreate?: { automaticDiscountNode?: { id?: unknown } } };
  }
).data?.discountAutomaticFreeShippingCreate?.automaticDiscountNode?.id;

if (typeof automaticDiscountId !== 'string') {
  await runGraphqlRaw(codeDeleteDocument, { id: codeDiscountId });
  throw new Error(`Automatic free-shipping create did not return an id: ${JSON.stringify(automaticCreate)}`);
}

const automaticUpdateVariables = {
  id: automaticDiscountId,
  input: {
    title: `HAR-196 automatic free shipping updated ${runId}`,
    startsAt,
    endsAt: null,
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '18.00',
      },
    },
    destination: {
      countries: {
        add: ['US'],
        includeRestOfWorld: false,
      },
    },
    maximumShippingPrice: '22.00',
    appliesOnOneTimePurchase: false,
    appliesOnSubscription: true,
    recurringCycleLimit: 3,
  },
};

const codeUpdate = await runGraphqlRaw(codeUpdateDocument, codeUpdateVariables);
const automaticUpdate = await runGraphqlRaw(automaticUpdateDocument, automaticUpdateVariables);
const readAfterUpdate = await runGraphqlRaw(readDocument, {
  codeId: codeDiscountId,
  automaticId: automaticDiscountId,
  code: updatedCode,
});
const codeDeactivate = await runGraphqlRaw(codeDeactivateDocument, { id: codeDiscountId });
const automaticDeactivate = await runGraphqlRaw(automaticDeactivateDocument, { id: automaticDiscountId });
const codeActivate = await runGraphqlRaw(codeActivateDocument, { id: codeDiscountId });
const automaticActivate = await runGraphqlRaw(automaticActivateDocument, { id: automaticDiscountId });
const codeDelete = await runGraphqlRaw(codeDeleteDocument, { id: codeDiscountId });
const automaticDelete = await runGraphqlRaw(automaticDeleteDocument, { id: automaticDiscountId });
const readAfterDelete = await runGraphqlRaw(readDocument, {
  codeId: codeDiscountId,
  automaticId: automaticDiscountId,
  code: updatedCode,
});

const output = {
  variables: {
    codeDiscountId,
    automaticDiscountId,
    initialCode,
    updatedCode,
  },
  requests: {
    codeCreate: { query: codeCreateDocument, variables: codeCreateVariables },
    codeUpdate: { query: codeUpdateDocument, variables: codeUpdateVariables },
    codeDeactivate: { query: codeDeactivateDocument, variables: { id: codeDiscountId } },
    codeActivate: { query: codeActivateDocument, variables: { id: codeDiscountId } },
    codeDelete: { query: codeDeleteDocument, variables: { id: codeDiscountId } },
    automaticCreate: { query: automaticCreateDocument, variables: automaticCreateVariables },
    automaticUpdate: { query: automaticUpdateDocument, variables: automaticUpdateVariables },
    automaticDeactivate: { query: automaticDeactivateDocument, variables: { id: automaticDiscountId } },
    automaticActivate: { query: automaticActivateDocument, variables: { id: automaticDiscountId } },
    automaticDelete: { query: automaticDeleteDocument, variables: { id: automaticDiscountId } },
    read: { query: readDocument },
  },
  scopeProbe,
  codeCreate,
  codeUpdate,
  automaticCreate,
  automaticUpdate,
  readAfterUpdate,
  codeDeactivate,
  automaticDeactivate,
  codeActivate,
  automaticActivate,
  codeDelete,
  automaticDelete,
  readAfterDelete,
};

const outputPath = path.join(outputDir, 'discount-free-shipping-lifecycle.json');
await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputPath,
      codeDiscountId,
      automaticDiscountId,
      initialCode,
      updatedCode,
    },
    null,
    2,
  ),
);
