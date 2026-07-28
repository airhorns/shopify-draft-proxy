import { readFile } from 'node:fs/promises';

export type RecordedUpstreamCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

type GraphqlRequestRunner = (
  query: string,
  variables: Record<string, unknown>,
) => Promise<{ status: number; payload: unknown }>;

export const DRAFT_PROXY_SHOP_PRICING_HYDRATE_QUERY =
  'query DraftProxyShopPricingHydrate { shop { currencyCode taxesIncluded taxShipping } }';

export const GIFT_CARD_CREATE_CONFIGURATION_QUERY = `#graphql
  query GiftCardCreateConfiguration {
    giftCardConfiguration {
      issueLimit { amount currencyCode }
      purchaseLimit { amount currencyCode }
    }
  }
`;

export const DISCOUNT_CONTEXT_REFS_HYDRATE_QUERY = `#graphql
  query DiscountContextRefsHydrate($ids: [ID!]!) {
    nodes(ids: $ids) {
      __typename
      id
      ... on Customer {
        displayName
        email
      }
      ... on Segment {
        name
        query
        creationDate
        lastEditDate
      }
    }
  }
`;

export const PRODUCT_PAYLOAD_SHOP_HYDRATE_QUERY = `#graphql
  query ProductPayloadShopHydrate {
    shop {
      id
      name
      myshopifyDomain
      url
      currencyCode
      primaryDomain {
        id
        host
        url
        sslEnabled
      }
    }
  }
`;

export const STORE_PROPERTIES_LOCATION_HYDRATE_QUERY =
  'query StorePropertiesLocationHydrate($id: ID!) { location(id: $id) { id legacyResourceId name activatable addressVerified createdAt deactivatable deactivatedAt deletable fulfillsOnlineOrders hasActiveInventory hasUnfulfilledOrders isActive isFulfillmentService isPrimary shipsInventory updatedAt fulfillmentService { id handle serviceName } address { address1 address2 city country countryCode formatted latitude longitude phone province provinceCode zip } suggestedAddresses { address1 countryCode formatted } metafield(namespace: "custom", key: "hours") { id namespace key value type } metafields(first: 3) { nodes { id namespace key value type } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } inventoryLevels(first: 3) { nodes { id item { id } location { id name } quantities(names: ["available", "committed", "on_hand"]) { name quantity updatedAt } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } } }';

let discountUniquenessQueryPromise: Promise<string> | undefined;
let discountItemRefsHydrateQueryPromise: Promise<string> | undefined;
let shopSubscriptionCapabilityQueryPromise: Promise<string> | undefined;

function readDiscountUniquenessQuery(): Promise<string> {
  discountUniquenessQueryPromise ??= readFile(
    new URL('../../../src/runtime_graphql/discounts/discount-uniqueness-check.graphql', import.meta.url),
    'utf8',
  );
  return discountUniquenessQueryPromise;
}

async function readDiscountItemRefsHydrateQuery(): Promise<string> {
  discountItemRefsHydrateQueryPromise ??= readFile(
    new URL('../../../src/runtime_graphql/discounts/discount-item-refs-hydrate.graphql.raw', import.meta.url),
    'utf8',
  );
  const query = await discountItemRefsHydrateQueryPromise;
  return query.endsWith('\n') ? query.slice(0, -1) : query;
}

function readShopSubscriptionCapabilityQuery(): Promise<string> {
  shopSubscriptionCapabilityQueryPromise ??= readFile(
    new URL('../../../src/runtime_graphql/discounts/discount-subscription-capability.graphql', import.meta.url),
    'utf8',
  );
  return shopSubscriptionCapabilityQueryPromise;
}

function responseHasTopLevelErrors(payload: unknown): boolean {
  return typeof payload === 'object' && payload !== null && 'errors' in payload;
}

export async function captureRuntimeHydrationCall({
  operationName,
  query,
  variables = {},
  runGraphqlRequest,
}: {
  operationName: string;
  query: string;
  variables?: Record<string, unknown>;
  runGraphqlRequest: GraphqlRequestRunner;
}): Promise<RecordedUpstreamCall> {
  const response = await runGraphqlRequest(query, variables);
  if (response.status < 200 || response.status >= 300 || responseHasTopLevelErrors(response.payload)) {
    throw new Error(
      `Runtime hydration capture ${operationName} failed: ${JSON.stringify(
        { status: response.status, payload: response.payload },
        null,
        2,
      )}`,
    );
  }
  return {
    operationName,
    variables,
    query,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

export async function captureDraftProxyShopPricingHydrate(
  runGraphqlRequest: GraphqlRequestRunner,
): Promise<RecordedUpstreamCall> {
  return await captureRuntimeHydrationCall({
    operationName: 'DraftProxyShopPricingHydrate',
    query: DRAFT_PROXY_SHOP_PRICING_HYDRATE_QUERY,
    runGraphqlRequest,
  });
}

export async function captureDraftProxyShopSubscriptionCapability(
  runGraphqlRequest: GraphqlRequestRunner,
): Promise<RecordedUpstreamCall> {
  return await captureRuntimeHydrationCall({
    operationName: 'DraftProxyShopSubscriptionCapability',
    query: await readShopSubscriptionCapabilityQuery(),
    runGraphqlRequest,
  });
}

export async function captureProductPayloadShopHydrate(
  runGraphqlRequest: GraphqlRequestRunner,
): Promise<RecordedUpstreamCall> {
  return await captureRuntimeHydrationCall({
    operationName: 'ProductPayloadShopHydrate',
    query: PRODUCT_PAYLOAD_SHOP_HYDRATE_QUERY,
    runGraphqlRequest,
  });
}

export async function captureGiftCardCreateConfiguration(
  runGraphqlRequest: GraphqlRequestRunner,
): Promise<RecordedUpstreamCall> {
  return await captureRuntimeHydrationCall({
    operationName: 'GiftCardCreateConfiguration',
    query: GIFT_CARD_CREATE_CONFIGURATION_QUERY,
    runGraphqlRequest,
  });
}

export async function captureDiscountContextRefsHydrate(
  runGraphqlRequest: GraphqlRequestRunner,
  ids: string[],
): Promise<RecordedUpstreamCall> {
  return await captureRuntimeHydrationCall({
    operationName: 'DiscountContextRefsHydrate',
    query: DISCOUNT_CONTEXT_REFS_HYDRATE_QUERY,
    variables: { ids },
    runGraphqlRequest,
  });
}

export async function captureDiscountUniquenessCheck(
  runGraphqlRequest: GraphqlRequestRunner,
  code: string,
): Promise<RecordedUpstreamCall> {
  return await captureRuntimeHydrationCall({
    operationName: 'DiscountUniquenessCheck',
    query: await readDiscountUniquenessQuery(),
    variables: { code },
    runGraphqlRequest,
  });
}

export async function captureDiscountItemRefsHydrate(
  runGraphqlRequest: GraphqlRequestRunner,
  ids: string[],
): Promise<RecordedUpstreamCall> {
  const sortedIds = [...new Set(ids)].sort((left, right) => {
    const leftTail = /\/(\d+)$/u.exec(left)?.[1];
    const rightTail = /\/(\d+)$/u.exec(right)?.[1];
    if (leftTail !== undefined && rightTail !== undefined) {
      const numericOrder = BigInt(leftTail) < BigInt(rightTail) ? -1 : BigInt(leftTail) > BigInt(rightTail) ? 1 : 0;
      if (numericOrder !== 0) return numericOrder;
    }
    return left.localeCompare(right);
  });
  return await captureRuntimeHydrationCall({
    operationName: 'ProductsHydrateNodes',
    query: await readDiscountItemRefsHydrateQuery(),
    variables: { ids: sortedIds },
    runGraphqlRequest,
  });
}

export async function captureStorePropertiesLocationHydrate(
  runGraphqlRequest: GraphqlRequestRunner,
  id: string,
): Promise<RecordedUpstreamCall> {
  return await captureRuntimeHydrationCall({
    operationName: 'StorePropertiesLocationHydrate',
    query: STORE_PROPERTIES_LOCATION_HYDRATE_QUERY,
    variables: { id },
    runGraphqlRequest,
  });
}
