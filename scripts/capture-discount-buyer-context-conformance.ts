/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-buyer-context-lifecycle.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

// Shared verbatim with the Rust proxy so the recorded cassette calls byte-match
// what the de-seeded create/update path forwards upstream to resolve buyer-context
// member names the real way (the cassette matcher is strict on query text +
// variables). `CustomerHydrate` stages the customer's displayName, the segment
// hydrate stages the segment's name, and `DiscountUniquenessCheck` resolves the
// code uniqueness probe the code-discount create forwards.
const customerHydrateQuery = await readFile(
  'config/parity-requests/customers/customer-mutation-hydrate.graphql',
  'utf8',
);
const segmentHydrateQuery = await readFile(
  'config/parity-requests/discounts/discount-context-segment-hydrate.graphql',
  'utf8',
);
const uniquenessQuery = await readFile('config/parity-requests/discounts/discount-uniqueness-check.graphql', 'utf8');
const upstreamCalls: unknown[] = [];

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const runId = Date.now();
const marker = `har390-buyer-${runId}`;
const startsAt = '2023-01-01T00:00:00Z';
const initialCode = `HAR390CTX${runId}`;
const updatedCode = `HAR390SEG${runId}`;

const customerCreateDocument = `#graphql
  mutation DiscountBuyerContextCustomerCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        firstName
        lastName
        displayName
        email
        tags
        createdAt
        updatedAt
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const customerDeleteDocument = `#graphql
  mutation DiscountBuyerContextCustomerDelete($input: CustomerDeleteInput!) {
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
  mutation DiscountBuyerContextSegmentCreate($name: String!, $query: String!) {
    segmentCreate(name: $name, query: $query) {
      segment {
        id
        name
        query
        creationDate
        lastEditDate
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const segmentDeleteDocument = `#graphql
  mutation DiscountBuyerContextSegmentDelete($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

const codeDiscountSelection = `#graphql
  codeDiscountNode {
    id
    codeDiscount {
      __typename
      ... on DiscountCodeBasic {
        title
        status
        codes(first: 1) {
          nodes {
            code
            asyncUsageCount
          }
        }
        context {
          __typename
          ... on DiscountCustomers {
            customers {
              __typename
              id
              displayName
            }
          }
          ... on DiscountCustomerSegments {
            segments {
              __typename
              id
              name
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
`;

const automaticDiscountSelection = `#graphql
  automaticDiscountNode {
    id
    automaticDiscount {
      __typename
      ... on DiscountAutomaticBasic {
        title
        status
        context {
          __typename
          ... on DiscountCustomers {
            customers {
              __typename
              id
              displayName
            }
          }
          ... on DiscountCustomerSegments {
            segments {
              __typename
              id
              name
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
`;

const codeCreateDocument = `#graphql
  mutation DiscountCodeBasicBuyerContextCreate($input: DiscountCodeBasicInput!) {
    discountCodeBasicCreate(basicCodeDiscount: $input) {
      ${codeDiscountSelection}
    }
  }
`;

const codeUpdateDocument = `#graphql
  mutation DiscountCodeBasicBuyerContextUpdate($id: ID!, $input: DiscountCodeBasicInput!) {
    discountCodeBasicUpdate(id: $id, basicCodeDiscount: $input) {
      ${codeDiscountSelection}
    }
  }
`;

const codeReadDocument = `#graphql
  query DiscountCodeBasicBuyerContextRead($id: ID!, $code: String!) {
    discountNode(id: $id) {
      id
      discount {
        __typename
        ... on DiscountCodeBasic {
          title
          context {
            __typename
            ... on DiscountCustomerSegments {
              segments {
                __typename
                id
                name
              }
            }
          }
        }
      }
    }
    codeDiscountNodeByCode(code: $code) {
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          context {
            __typename
            ... on DiscountCustomerSegments {
              segments {
                __typename
                id
                name
              }
            }
          }
        }
      }
    }
  }
`;

const codeDeleteDocument = `#graphql
  mutation DiscountCodeBasicBuyerContextDelete($id: ID!) {
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
  mutation DiscountAutomaticBasicBuyerContextCreate($input: DiscountAutomaticBasicInput!) {
    discountAutomaticBasicCreate(automaticBasicDiscount: $input) {
      ${automaticDiscountSelection}
    }
  }
`;

const automaticUpdateDocument = `#graphql
  mutation DiscountAutomaticBasicBuyerContextUpdate($id: ID!, $input: DiscountAutomaticBasicInput!) {
    discountAutomaticBasicUpdate(id: $id, automaticBasicDiscount: $input) {
      ${automaticDiscountSelection}
    }
  }
`;

const automaticReadDocument = `#graphql
  query DiscountAutomaticBasicBuyerContextRead($id: ID!) {
    automaticDiscountNode(id: $id) {
      id
      automaticDiscount {
        __typename
        ... on DiscountAutomaticBasic {
          title
          context {
            __typename
            ... on DiscountCustomerSegments {
              segments {
                __typename
                id
                name
              }
            }
          }
        }
      }
    }
  }
`;

const automaticDeleteDocument = `#graphql
  mutation DiscountAutomaticBasicBuyerContextDelete($id: ID!) {
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

const customerCreateVariables = {
  input: {
    firstName: 'HAR390',
    lastName: 'Buyer Context',
    email: `har390-buyer-context-${runId}@example.com`,
    tags: [marker],
  },
};
const segmentCreateVariables = {
  name: `HAR-390 buyer context ${runId}`,
  query: `customer_tags CONTAINS '${marker}'`,
};

let customerId: string | null = null;
let segmentId: string | null = null;
let codeDiscountId: string | null = null;
let automaticDiscountId: string | null = null;
let customerCreate: ConformanceGraphqlResult | null = null;
let segmentCreate: ConformanceGraphqlResult | null = null;
let codeCreate: ConformanceGraphqlResult | null = null;
let codeUpdate: ConformanceGraphqlResult | null = null;
let codeReadAfterUpdate: ConformanceGraphqlResult | null = null;
let automaticCreate: ConformanceGraphqlResult | null = null;
let automaticUpdate: ConformanceGraphqlResult | null = null;
let automaticReadAfterUpdate: ConformanceGraphqlResult | null = null;
let codeCreateVariables: Record<string, unknown> | null = null;
let codeUpdateVariables: Record<string, unknown> | null = null;
let automaticCreateVariables: Record<string, unknown> | null = null;
let automaticUpdateVariables: Record<string, unknown> | null = null;
const cleanup: Record<string, ConformanceGraphqlResult> = {};

try {
  customerCreate = await runGraphqlRaw(customerCreateDocument, customerCreateVariables);
  assertSuccess(customerCreate, 'customerCreate setup');
  customerId = readRequiredString(customerCreate, ['data', 'customerCreate', 'customer', 'id'], 'customerCreate setup');

  segmentCreate = await runGraphqlRaw(segmentCreateDocument, segmentCreateVariables);
  assertSuccess(segmentCreate, 'segmentCreate setup');
  segmentId = readRequiredString(segmentCreate, ['data', 'segmentCreate', 'segment', 'id'], 'segmentCreate setup');

  // Record the reads the de-seeded proxy forwards to resolve the buyer-context
  // member names from real store state (instead of a seeded precondition): the
  // customer hydrate (customer-context create), the segment hydrate
  // (segment-context update), and the code uniqueness probe (code-discount
  // create). Each is issued against the live store while the disposable customer /
  // segment still exist, and stored in the cassette entry shape so parity replays
  // them byte-for-byte.
  const customerHydrate = await runGraphqlRaw(customerHydrateQuery, { id: customerId });
  assertSuccess(customerHydrate, 'customer hydrate forward');
  upstreamCalls.push({
    operationName: 'CustomerHydrate',
    variables: { id: customerId },
    query: customerHydrateQuery,
    response: { status: customerHydrate.status, body: customerHydrate.payload },
  });

  const segmentHydrate = await runGraphqlRaw(segmentHydrateQuery, { id: segmentId });
  assertSuccess(segmentHydrate, 'segment hydrate forward');
  upstreamCalls.push({
    operationName: 'DiscountContextSegmentHydrate',
    variables: { id: segmentId },
    query: segmentHydrateQuery,
    response: { status: segmentHydrate.status, body: segmentHydrate.payload },
  });

  const uniquenessCheck = await runGraphqlRaw(uniquenessQuery, { code: initialCode });
  assertSuccess(uniquenessCheck, 'code uniqueness forward');
  upstreamCalls.push({
    operationName: 'DiscountUniquenessCheck',
    variables: { code: initialCode },
    query: uniquenessQuery,
    response: { status: uniquenessCheck.status, body: uniquenessCheck.payload },
  });

  codeCreateVariables = {
    input: {
      title: `HAR-390 code customer context ${runId}`,
      code: initialCode,
      startsAt,
      combinesWith: {
        productDiscounts: false,
        orderDiscounts: true,
        shippingDiscounts: false,
      },
      context: {
        customers: {
          add: [customerId],
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
  codeCreate = await runGraphqlRaw(codeCreateDocument, codeCreateVariables);
  assertSuccess(codeCreate, 'discountCodeBasicCreate customer context');
  codeDiscountId = readRequiredString(
    codeCreate,
    ['data', 'discountCodeBasicCreate', 'codeDiscountNode', 'id'],
    'discountCodeBasicCreate customer context',
  );

  codeUpdateVariables = {
    id: codeDiscountId,
    input: {
      title: `HAR-390 code segment context ${runId}`,
      code: updatedCode,
      startsAt,
      combinesWith: {
        productDiscounts: false,
        orderDiscounts: true,
        shippingDiscounts: false,
      },
      context: {
        customerSegments: {
          add: [segmentId],
        },
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
    },
  };
  codeUpdate = await runGraphqlRaw(codeUpdateDocument, codeUpdateVariables);
  assertSuccess(codeUpdate, 'discountCodeBasicUpdate segment context');
  codeReadAfterUpdate = await runGraphqlRaw(codeReadDocument, { id: codeDiscountId, code: updatedCode });
  assertSuccess(codeReadAfterUpdate, 'discount code buyer-context read after update');

  automaticCreateVariables = {
    input: {
      title: `HAR-390 automatic customer context ${runId}`,
      startsAt,
      combinesWith: {
        productDiscounts: false,
        orderDiscounts: true,
        shippingDiscounts: false,
      },
      context: {
        customers: {
          add: [customerId],
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
  automaticCreate = await runGraphqlRaw(automaticCreateDocument, automaticCreateVariables);
  assertSuccess(automaticCreate, 'discountAutomaticBasicCreate customer context');
  automaticDiscountId = readRequiredString(
    automaticCreate,
    ['data', 'discountAutomaticBasicCreate', 'automaticDiscountNode', 'id'],
    'discountAutomaticBasicCreate customer context',
  );

  automaticUpdateVariables = {
    id: automaticDiscountId,
    input: {
      title: `HAR-390 automatic segment context ${runId}`,
      startsAt,
      combinesWith: {
        productDiscounts: true,
        orderDiscounts: false,
        shippingDiscounts: false,
      },
      context: {
        customerSegments: {
          add: [segmentId],
        },
      },
      customerGets: {
        value: {
          discountAmount: {
            amount: '3.00',
            appliesOnEachItem: false,
          },
        },
        items: {
          all: true,
        },
      },
    },
  };
  automaticUpdate = await runGraphqlRaw(automaticUpdateDocument, automaticUpdateVariables);
  assertSuccess(automaticUpdate, 'discountAutomaticBasicUpdate segment context');
  automaticReadAfterUpdate = await runGraphqlRaw(automaticReadDocument, { id: automaticDiscountId });
  assertSuccess(automaticReadAfterUpdate, 'discount automatic buyer-context read after update');
} finally {
  if (codeDiscountId) {
    const codeDelete = await runGraphqlRaw(codeDeleteDocument, { id: codeDiscountId });
    assertSuccess(codeDelete, 'discountCodeDelete cleanup');
    cleanup['codeDelete'] = codeDelete;
  }
  if (automaticDiscountId) {
    const automaticDelete = await runGraphqlRaw(automaticDeleteDocument, { id: automaticDiscountId });
    assertSuccess(automaticDelete, 'discountAutomaticDelete cleanup');
    cleanup['automaticDelete'] = automaticDelete;
  }
  if (segmentId) {
    const segmentDelete = await runGraphqlRaw(segmentDeleteDocument, { id: segmentId });
    assertSuccess(segmentDelete, 'segmentDelete cleanup');
    cleanup['segmentDelete'] = segmentDelete;
  }
  if (customerId) {
    const customerDelete = await runGraphqlRaw(customerDeleteDocument, { input: { id: customerId } });
    assertSuccess(customerDelete, 'customerDelete cleanup');
    cleanup['customerDelete'] = customerDelete;
  }
}

const output = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  variables: {
    customerId,
    segmentId,
    codeDiscountId,
    automaticDiscountId,
    initialCode,
    updatedCode,
  },
  scopeProbe,
  // De-seeded: the buyer-context member names are resolved by forwarding the reads
  // in `upstreamCalls` (customer / segment hydrate) rather than injecting
  // `seedCustomers` / `seedSegments` preconditions. The disposable customer and
  // segment are still created (and deleted) as live setup so those reads return
  // real data at capture time.
  upstreamCalls,
  setup: {
    customerCreate: { query: customerCreateDocument, variables: customerCreateVariables, response: customerCreate },
    segmentCreate: { query: segmentCreateDocument, variables: segmentCreateVariables, response: segmentCreate },
  },
  requests: {
    codeCreate: { query: codeCreateDocument, variables: codeCreateVariables },
    codeUpdate: { query: codeUpdateDocument, variables: codeUpdateVariables },
    codeRead: { query: codeReadDocument },
    codeDelete: { query: codeDeleteDocument },
    automaticCreate: { query: automaticCreateDocument, variables: automaticCreateVariables },
    automaticUpdate: { query: automaticUpdateDocument, variables: automaticUpdateVariables },
    automaticRead: { query: automaticReadDocument },
    automaticDelete: { query: automaticDeleteDocument },
  },
  codeCreate,
  codeUpdate,
  codeReadAfterUpdate,
  automaticCreate,
  automaticUpdate,
  automaticReadAfterUpdate,
  cleanup,
  notes: [
    'Live Shopify 2026-04 capture for code-basic and automatic-basic discount buyer context transitions from explicit customer selection to customer-segment selection.',
    'The disposable discount, customer, and segment records are deleted in cleanup after capture.',
    'De-seeded: buyer-context member names are resolved by forwarding the customer / segment hydrate reads recorded in upstreamCalls, not by seedCustomers / seedSegments preconditions.',
  ],
};

if (
  !codeCreate ||
  !codeUpdate ||
  !codeReadAfterUpdate ||
  !automaticCreate ||
  !automaticUpdate ||
  !automaticReadAfterUpdate
) {
  throw new Error(`Buyer-context capture did not complete all required phases: ${JSON.stringify(output, null, 2)}`);
}

await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputPath,
      customerId,
      segmentId,
      codeDiscountId,
      automaticDiscountId,
    },
    null,
    2,
  ),
);
