import { type AdminGraphqlOptions, createAdminGraphqlClient } from './conformance-graphql-client.js';

export const DISCOUNT_REQUIRED_SCOPES = ['read_discounts', 'write_discounts'] as const;

export type DiscountRequiredScope = (typeof DISCOUNT_REQUIRED_SCOPES)[number];

export type DiscountScopeProbeResult = {
  availableScopes: string[];
  requiredScopes: Array<{
    handle: DiscountRequiredScope;
    present: boolean;
  }>;
  missingScopes: DiscountRequiredScope[];
  hasRequiredScopes: boolean;
};

export type DiscountReadCapture = {
  discountNodesCount: unknown;
  discountNodesCatalog: unknown;
};

export type DiscountDetailCapture = {
  variables: Record<string, unknown>;
  create: unknown;
  response: unknown;
  cleanup: unknown;
};

type ScopeNode = {
  handle?: unknown;
};

type AccessScopesPayload = {
  data?: {
    currentAppInstallation?: {
      accessScopes?: ScopeNode[];
    };
  };
};

export const DISCOUNT_ACCESS_SCOPES_QUERY = `#graphql
  query DiscountConformanceAccessScopes {
    currentAppInstallation {
      accessScopes {
        handle
      }
    }
  }
`;

export const DISCOUNT_NODES_COUNT_QUERY = `#graphql
  query DiscountNodesCountRead($query: String) {
    discountNodesCount(query: $query) {
      count
      precision
    }
  }
`;

export const DISCOUNT_NODES_CATALOG_QUERY = `#graphql
  query DiscountNodesCatalogRead($first: Int!, $query: String) {
    discountNodes(first: $first, query: $query) {
      edges {
        cursor
        node {
          id
          discount {
            __typename
            ... on DiscountCodeBasic {
              title
              status
              summary
              startsAt
              endsAt
            }
            ... on DiscountCodeBxgy {
              title
              status
              summary
              startsAt
              endsAt
            }
            ... on DiscountCodeFreeShipping {
              title
              status
              summary
              startsAt
              endsAt
            }
            ... on DiscountAutomaticBasic {
              title
              status
              summary
              startsAt
              endsAt
            }
            ... on DiscountAutomaticBxgy {
              title
              status
              summary
              startsAt
              endsAt
            }
            ... on DiscountAutomaticFreeShipping {
              title
              status
              summary
              startsAt
              endsAt
            }
          }
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const DISCOUNT_CODE_BASIC_CREATE_MUTATION = `#graphql
  mutation DiscountCodeBasicDetailCreate($input: DiscountCodeBasicInput!) {
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
      }
    }
  }
`;

const DISCOUNT_AUTOMATIC_BASIC_CREATE_MUTATION = `#graphql
  mutation DiscountAutomaticBasicDetailCreate($input: DiscountAutomaticBasicInput!) {
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
      }
    }
  }
`;

export const DISCOUNT_CODE_DETAIL_QUERY = `#graphql
  query DiscountCodeDetailRead($id: ID!, $code: String!) {
    discountNode(id: $id) {
      id
      discount {
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
      metafield(namespace: "custom", key: "har192_missing") {
        id
      }
      metafields(first: 2) {
        nodes {
          id
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      events(first: 2) {
        edges {
          cursor
          node {
            __typename
            ... on BasicEvent {
              id
              action
              message
              createdAt
              subjectId
              subjectType
            }
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
    }
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
        }
      }
    }
    codeDiscountNodeByCode(code: $code) {
      id
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
        }
      }
    }
  }
`;

export const DISCOUNT_AUTOMATIC_DETAIL_QUERY = `#graphql
  query DiscountAutomaticDetailRead($id: ID!) {
    discountNode(id: $id) {
      id
      discount {
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
    automaticDiscountNode(id: $id) {
      id
      automaticDiscount {
        __typename
        ... on DiscountAutomaticBasic {
          title
          status
          summary
          startsAt
          endsAt
          asyncUsageCount
          discountClasses
          combinesWith {
            productDiscounts
            orderDiscounts
            shippingDiscounts
          }
          customerGets {
            value {
              __typename
              ... on DiscountPercentage {
                percentage
              }
            }
          }
          minimumRequirement {
            __typename
            ... on DiscountMinimumQuantity {
              greaterThanOrEqualToQuantity
            }
          }
        }
      }
      metafields(first: 2) {
        edges {
          cursor
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
      events(first: 2) {
        edges {
          cursor
          node {
            __typename
            ... on BasicEvent {
              id
              action
              message
              createdAt
              subjectId
              subjectType
            }
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
    }
  }
`;

const DISCOUNT_CODE_DELETE_MUTATION = `#graphql
  mutation DiscountCodeDetailCleanup($id: ID!) {
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

const DISCOUNT_AUTOMATIC_DELETE_MUTATION = `#graphql
  mutation DiscountAutomaticDetailCleanup($id: ID!) {
    discountAutomaticDelete(id: $id) {
      deletedAutomaticDiscountId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

export async function probeDiscountConformanceScopes(options: AdminGraphqlOptions): Promise<DiscountScopeProbeResult> {
  const { runGraphql } = createAdminGraphqlClient(options);
  const payload = (await runGraphql(DISCOUNT_ACCESS_SCOPES_QUERY)) as AccessScopesPayload;
  const scopeNodes = payload.data?.currentAppInstallation?.accessScopes ?? [];
  const availableScopes = scopeNodes
    .map((scope) => (typeof scope.handle === 'string' ? scope.handle : null))
    .filter((handle): handle is string => handle !== null)
    .sort((left, right) => left.localeCompare(right));
  const availableScopeSet = new Set(availableScopes);
  const requiredScopes = DISCOUNT_REQUIRED_SCOPES.map((handle) => ({
    handle,
    present: availableScopeSet.has(handle),
  }));
  const missingScopes = requiredScopes
    .filter((scope): scope is { handle: DiscountRequiredScope; present: false } => !scope.present)
    .map((scope) => scope.handle);

  return {
    availableScopes,
    requiredScopes,
    missingScopes,
    hasRequiredScopes: missingScopes.length === 0,
  };
}

export function assertDiscountConformanceScopes(probe: DiscountScopeProbeResult): void {
  if (probe.hasRequiredScopes) {
    return;
  }

  throw new Error(
    [
      `Discount conformance capture requires Shopify Admin scopes ${DISCOUNT_REQUIRED_SCOPES.join(' and ')}.`,
      `Missing: ${probe.missingScopes.join(', ')}.`,
      "Generate a new grant with `corepack pnpm conformance:auth-link` and exchange it with `corepack pnpm conformance:exchange-auth -- '<full callback url>'`.",
      'Credential access must come from `scripts/shopify-conformance-auth.mts`; do not place admin access tokens in repo `.env` files.',
    ].join(' '),
  );
}

export async function captureDiscountReadEvidence(
  options: AdminGraphqlOptions,
  variables: { first: number; query?: string | null },
): Promise<DiscountReadCapture> {
  const { runGraphql } = createAdminGraphqlClient(options);
  const requestVariables = {
    first: variables.first,
    query: variables.query ?? null,
  };

  const discountNodesCount = await runGraphql(DISCOUNT_NODES_COUNT_QUERY, {
    query: requestVariables.query,
  });
  const discountNodesCatalog = await runGraphql(DISCOUNT_NODES_CATALOG_QUERY, requestVariables);

  return {
    discountNodesCount,
    discountNodesCatalog,
  };
}

function readCreatedCodeDiscountId(response: unknown): string {
  const id = (response as { data?: { discountCodeBasicCreate?: { codeDiscountNode?: { id?: unknown } } } }).data
    ?.discountCodeBasicCreate?.codeDiscountNode?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`discountCodeBasicCreate did not return a codeDiscountNode id: ${JSON.stringify(response)}`);
  }
  return id;
}

function readCreatedAutomaticDiscountId(response: unknown): string {
  const id = (response as { data?: { discountAutomaticBasicCreate?: { automaticDiscountNode?: { id?: unknown } } } })
    .data?.discountAutomaticBasicCreate?.automaticDiscountNode?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(
      `discountAutomaticBasicCreate did not return an automaticDiscountNode id: ${JSON.stringify(response)}`,
    );
  }
  return id;
}

export async function captureDiscountDetailEvidence(options: AdminGraphqlOptions): Promise<{
  codeDetail: DiscountDetailCapture;
  automaticDetail: DiscountDetailCapture;
}> {
  const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient(options);
  const stamp = Date.now();
  const startsAt = '2026-04-25T00:00:00Z';
  const code = `HAR192DETAIL${stamp}`;

  const codeCreateVariables = {
    input: {
      title: `HAR-192 detail code ${stamp}`,
      code,
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
  const codeCreate = await runGraphql(DISCOUNT_CODE_BASIC_CREATE_MUTATION, codeCreateVariables);
  const codeId = readCreatedCodeDiscountId(codeCreate);
  const codeDetailVariables = { id: codeId, code };
  const codeResponse = await runGraphql(DISCOUNT_CODE_DETAIL_QUERY, codeDetailVariables);
  const codeCleanup = await runGraphqlRaw(DISCOUNT_CODE_DELETE_MUTATION, { id: codeId });

  const automaticCreateVariables = {
    input: {
      title: `HAR-192 detail automatic ${stamp}`,
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
  const automaticCreate = await runGraphql(DISCOUNT_AUTOMATIC_BASIC_CREATE_MUTATION, automaticCreateVariables);
  const automaticId = readCreatedAutomaticDiscountId(automaticCreate);
  const automaticDetailVariables = { id: automaticId };
  const automaticResponse = await runGraphql(DISCOUNT_AUTOMATIC_DETAIL_QUERY, automaticDetailVariables);
  const automaticCleanup = await runGraphqlRaw(DISCOUNT_AUTOMATIC_DELETE_MUTATION, { id: automaticId });

  return {
    codeDetail: {
      variables: codeDetailVariables,
      create: codeCreate,
      response: codeResponse,
      cleanup: codeCleanup,
    },
    automaticDetail: {
      variables: automaticDetailVariables,
      create: automaticCreate,
      response: automaticResponse,
      cleanup: automaticCleanup,
    },
  };
}
