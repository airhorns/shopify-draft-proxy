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
