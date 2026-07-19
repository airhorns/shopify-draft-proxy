import { readFile } from 'node:fs/promises';

import type { AdminGraphqlClient, ConformanceGraphqlPayload } from './conformance-graphql-client.js';

type JsonRecord = Record<string, unknown>;

export type ProductMutationPreflightUpstreamCall = {
  operationName: 'ProductMutationPreflightHydrate';
  variables: {
    id: string;
    variantsAfter: string | null;
    mediaAfter: string | null;
    collectionsAfter: string | null;
  };
  query: string;
  response: {
    status: number;
    body: ConformanceGraphqlPayload;
  };
};

export type ProductOptionLifecycleUpstreamCall = {
  operationName: 'ProductOptionLifecycleHydrateNodes';
  variables: { ids: string[] };
  query: string;
  response: {
    status: number;
    body: ConformanceGraphqlPayload;
  };
};

const productMutationPreflightQueryPromise = readFile(
  'config/parity-requests/products/product-mutation-preflight-hydrate.graphql',
  'utf8',
);
const productOptionLifecycleHydrationQueryPromise = readFile(
  'config/parity-requests/products/product-option-lifecycle-hydrate-nodes.graphql',
  'utf8',
);

function record(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function pageState(product: JsonRecord, field: string): { hasNextPage: boolean; endCursor: string | null } {
  const pageInfo = record(record(product[field])?.['pageInfo']);
  return {
    hasNextPage: pageInfo?.['hasNextPage'] === true,
    endCursor: typeof pageInfo?.['endCursor'] === 'string' ? pageInfo['endCursor'] : null,
  };
}

export async function captureProductOptionLifecycleHydration(
  runGraphqlRaw: AdminGraphqlClient['runGraphqlRaw'],
  productId: string,
): Promise<ProductOptionLifecycleUpstreamCall> {
  const query = await productOptionLifecycleHydrationQueryPromise;
  const variables = { ids: [productId] };
  const response = await runGraphqlRaw(query, variables);
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`ProductOptionLifecycleHydrateNodes failed: ${JSON.stringify(response, null, 2)}`);
  }
  return {
    operationName: 'ProductOptionLifecycleHydrateNodes',
    variables,
    query,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

export async function captureProductMutationPreflight(
  runGraphqlRaw: AdminGraphqlClient['runGraphqlRaw'],
  productId: string,
): Promise<ProductMutationPreflightUpstreamCall[]> {
  const query = await productMutationPreflightQueryPromise;
  const calls: ProductMutationPreflightUpstreamCall[] = [];
  let variantsAfter: string | null = null;
  let mediaAfter: string | null = null;
  let collectionsAfter: string | null = null;
  const seenCursorSets = new Set<string>();

  for (;;) {
    const cursorKey = JSON.stringify([variantsAfter, mediaAfter, collectionsAfter]);
    if (seenCursorSets.has(cursorKey)) {
      throw new Error(`ProductMutationPreflightHydrate repeated a cursor set for ${productId}.`);
    }
    seenCursorSets.add(cursorKey);

    const variables = { id: productId, variantsAfter, mediaAfter, collectionsAfter };
    const response = await runGraphqlRaw(query, variables);
    if (response.status < 200 || response.status >= 300 || response.payload.errors) {
      throw new Error(`ProductMutationPreflightHydrate failed: ${JSON.stringify(response, null, 2)}`);
    }
    calls.push({
      operationName: 'ProductMutationPreflightHydrate',
      variables,
      query,
      response: {
        status: response.status,
        body: response.payload,
      },
    });

    const product = record(record(response.payload.data)?.['product']);
    if (!product) {
      return calls;
    }
    const variantsPage = pageState(product, 'variants');
    const mediaPage = pageState(product, 'media');
    const collectionsPage = pageState(product, 'collections');
    if (!variantsPage.hasNextPage && !mediaPage.hasNextPage && !collectionsPage.hasNextPage) {
      return calls;
    }

    if (variantsPage.hasNextPage) {
      if (!variantsPage.endCursor) throw new Error('Variant hydration page omitted its end cursor.');
      variantsAfter = variantsPage.endCursor;
    } else if (variantsAfter === null) {
      variantsAfter = variantsPage.endCursor;
    }
    if (mediaPage.hasNextPage) {
      if (!mediaPage.endCursor) throw new Error('Media hydration page omitted its end cursor.');
      mediaAfter = mediaPage.endCursor;
    } else if (mediaAfter === null) {
      mediaAfter = mediaPage.endCursor;
    }
    if (collectionsPage.hasNextPage) {
      if (!collectionsPage.endCursor) throw new Error('Collection hydration page omitted its end cursor.');
      collectionsAfter = collectionsPage.endCursor;
    } else if (collectionsAfter === null) {
      collectionsAfter = collectionsPage.endCursor;
    }
  }
}
