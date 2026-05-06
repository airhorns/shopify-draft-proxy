/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type ProductCreateData = {
  productCreate?: {
    product?: {
      id?: unknown;
      title?: unknown;
      variants?: { nodes?: Array<{ id?: unknown; title?: unknown }> } | null;
    } | null;
    userErrors?: Array<{ field?: unknown; message?: unknown }> | null;
  } | null;
};

type CollectionCreateData = {
  collectionCreate?: {
    collection?: {
      id?: unknown;
      title?: unknown;
    } | null;
    userErrors?: Array<{ field?: unknown; message?: unknown }> | null;
  } | null;
};

type ProductRecord = {
  id: string;
  title: string;
  variantId: string;
  variantTitle: string;
};

type CollectionRecord = {
  id: string;
  title: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-items-refs-validation.json');
const requestPath = 'config/parity-requests/discounts/discount-items-refs-validation.graphql';
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const productCreateMutation = `#graphql
  mutation DiscountItemsRefsProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            title
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation DiscountItemsRefsProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionCreateMutation = `#graphql
  mutation DiscountItemsRefsCollectionCreate($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        id
        title
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionDeleteMutation = `#graphql
  mutation DiscountItemsRefsCollectionDelete($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const discountDeleteMutation = `#graphql
  mutation DiscountItemsRefsDiscountDelete($id: ID!) {
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

const discountUniquenessQuery = `#graphql
  query DiscountUniquenessCheck($code: String!) {
    codeDiscountNodeByCode(code: $code) {
      id
    }
  }
`;

async function readProductsHydrateNodesQuery(): Promise<string> {
  const source = await readFile('src/shopify_draft_proxy/proxy/products/products_core.gleam', 'utf8');
  const match = source.match(/pub const product_hydrate_nodes_query: String = "\n([\s\S]*?)\n"\n\n@internal/);
  if (!match) {
    throw new Error('Unable to read product_hydrate_nodes_query from products_core.gleam');
  }
  const query = match[1];
  if (query === undefined) {
    throw new Error('Unable to read product_hydrate_nodes_query body from products_core.gleam');
  }

  return query.replace(/\\"/g, '"');
}

function assertNoUserErrors(label: string, userErrors: unknown): void {
  if (Array.isArray(userErrors) && userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function readProduct(label: string, response: ConformanceGraphqlPayload<ProductCreateData>): ProductRecord {
  const create = response.data?.productCreate;
  assertNoUserErrors(label, create?.userErrors);

  const product = create?.product;
  const id = product?.id;
  const title = product?.title;
  const variant = product?.variants?.nodes?.[0];
  const variantId = variant?.id;
  const variantTitle = variant?.title;
  if (
    typeof id !== 'string' ||
    typeof title !== 'string' ||
    typeof variantId !== 'string' ||
    typeof variantTitle !== 'string'
  ) {
    throw new Error(`${label} did not return product and variant ids: ${JSON.stringify(response)}`);
  }

  return { id, title, variantId, variantTitle };
}

function readCollection(label: string, response: ConformanceGraphqlPayload<CollectionCreateData>): CollectionRecord {
  const create = response.data?.collectionCreate;
  assertNoUserErrors(label, create?.userErrors);

  const id = create?.collection?.id;
  const title = create?.collection?.title;
  if (typeof id !== 'string' || typeof title !== 'string') {
    throw new Error(`${label} did not return collection id/title: ${JSON.stringify(response)}`);
  }

  return { id, title };
}

function basicInput(stamp: number, suffix: string, items: Record<string, unknown>): Record<string, unknown> {
  return {
    title: `discount refs ${suffix} ${stamp}`,
    code: `REFS${suffix}${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    customerSelection: {
      all: true,
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items,
    },
  };
}

function bxgyInput(
  stamp: number,
  suffix: string,
  buyItems: Record<string, unknown>,
  getProductId: string,
): Record<string, unknown> {
  return {
    title: `discount refs bxgy ${suffix} ${stamp}`,
    code: `REFSBXGY${suffix}${stamp}`,
    startsAt: '2026-04-25T00:00:00Z',
    customerSelection: {
      all: true,
    },
    customerBuys: {
      value: {
        quantity: '1',
      },
      items: buyItems,
    },
    customerGets: {
      value: {
        discountOnQuantity: {
          quantity: '1',
          effect: {
            percentage: 1,
          },
        },
      },
      items: {
        products: {
          productsToAdd: [getProductId],
        },
      },
    },
  };
}

function collectCreatedDiscountIds(response: unknown): string[] {
  const data = (response as { data?: Record<string, unknown> }).data ?? {};
  return Object.values(data).flatMap((payload) => {
    const id = (payload as { codeDiscountNode?: { id?: unknown } } | null | undefined)?.codeDiscountNode?.id;
    return typeof id === 'string' ? [id] : [];
  });
}

function inputCode(input: unknown): string {
  const code = (input as { code?: unknown }).code;
  if (typeof code !== 'string') {
    throw new Error(`Discount input did not include a code: ${JSON.stringify(input)}`);
  }

  return code;
}

function shopifyGidTail(id: string): string {
  return id.split('?')[0]?.split('/').at(-1) ?? id;
}

function compareShopifyResourceIds(left: string, right: string): number {
  const leftTail = shopifyGidTail(left);
  const rightTail = shopifyGidTail(right);
  const leftNumeric = /^\d+$/.test(leftTail) ? BigInt(leftTail) : undefined;
  const rightNumeric = /^\d+$/.test(rightTail) ? BigInt(rightTail) : undefined;
  if (leftNumeric !== undefined && rightNumeric !== undefined && leftNumeric !== rightNumeric) {
    return leftNumeric < rightNumeric ? -1 : 1;
  }

  return left.localeCompare(right);
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const document = await readFile(requestPath, 'utf8');
const productsHydrateNodesQuery = await readProductsHydrateNodesQuery();
const stamp = Date.now();
const cleanup: Array<() => Promise<unknown>> = [];
const cleanupResponses: unknown[] = [];
const setupProducts: ProductRecord[] = [];
const setupCollections: CollectionRecord[] = [];
const upstreamCalls: unknown[] = [];
let variables: Record<string, unknown> | undefined;
let validationResponse: unknown;

try {
  const productOne = readProduct(
    'product one create',
    await runGraphql<ProductCreateData>(productCreateMutation, {
      product: {
        title: `discount refs product one ${stamp}`,
        status: 'ACTIVE',
        vendor: 'HERMES',
        productType: 'CONFORMANCE',
        tags: ['conformance', 'discount-items-refs-validation', String(stamp)],
      },
    }),
  );
  cleanup.push(() => runGraphqlRaw(productDeleteMutation, { input: { id: productOne.id } }));
  setupProducts.push(productOne);

  const productTwo = readProduct(
    'product two create',
    await runGraphql<ProductCreateData>(productCreateMutation, {
      product: {
        title: `discount refs product two ${stamp}`,
        status: 'ACTIVE',
        vendor: 'HERMES',
        productType: 'CONFORMANCE',
        tags: ['conformance', 'discount-items-refs-validation', String(stamp)],
      },
    }),
  );
  cleanup.push(() => runGraphqlRaw(productDeleteMutation, { input: { id: productTwo.id } }));
  setupProducts.push(productTwo);

  const collection = readCollection(
    'collection create',
    await runGraphql<CollectionCreateData>(collectionCreateMutation, {
      input: {
        title: `discount refs collection ${stamp}`,
      },
    }),
  );
  cleanup.push(() => runGraphqlRaw(collectionDeleteMutation, { input: { id: collection.id } }));
  setupCollections.push(collection);

  variables = {
    unknownProduct: basicInput(stamp, 'BADPROD', {
      products: {
        productsToAdd: ['gid://shopify/Product/999999999999'],
      },
    }),
    unknownVariant: basicInput(stamp, 'BADVAR', {
      products: {
        productVariantsToAdd: ['gid://shopify/ProductVariant/999999999999'],
      },
    }),
    unknownCollection: basicInput(stamp, 'BADCOLL', {
      collections: {
        add: ['gid://shopify/Collection/999999999999'],
      },
    }),
    bxgyUnknownBuy: bxgyInput(
      stamp,
      'BADBUY',
      {
        products: {
          productsToAdd: ['gid://shopify/Product/999999999999'],
        },
      },
      productTwo.id,
    ),
    collectionSentinel: basicInput(stamp, 'COLLZERO', {
      collections: {
        add: ['gid://shopify/Collection/0'],
      },
    }),
    successProduct: basicInput(stamp, 'OKPROD', {
      products: {
        productsToAdd: [productOne.id],
      },
    }),
    successVariant: basicInput(stamp, 'OKVAR', {
      products: {
        productVariantsToAdd: [productTwo.variantId],
      },
    }),
    successCollection: basicInput(stamp, 'OKCOLL', {
      collections: {
        add: [collection.id],
      },
    }),
  };

  const hydrationVariables = {
    ids: [
      'gid://shopify/Collection/0',
      collection.id,
      'gid://shopify/ProductVariant/999999999999',
      'gid://shopify/Collection/999999999999',
      'gid://shopify/Product/999999999999',
      productOne.id,
      productTwo.id,
      productTwo.variantId,
    ].sort(compareShopifyResourceIds),
  };
  const hydrateResponse = await runGraphqlRaw(productsHydrateNodesQuery, hydrationVariables);
  upstreamCalls.push({
    operationName: 'ProductsHydrateNodes',
    variables: hydrationVariables,
    query: productsHydrateNodesQuery,
    response: {
      status: hydrateResponse.status,
      body: hydrateResponse.payload,
    },
  });

  for (const input of Object.values(variables)) {
    const uniquenessVariables = { code: inputCode(input) };
    const uniquenessResponse = await runGraphqlRaw(discountUniquenessQuery, uniquenessVariables);
    upstreamCalls.push({
      operationName: 'DiscountUniquenessCheck',
      variables: uniquenessVariables,
      query: discountUniquenessQuery,
      response: {
        status: uniquenessResponse.status,
        body: uniquenessResponse.payload,
      },
    });
  }

  validationResponse = (await runGraphqlRaw(document, variables)).payload;

  for (const discountId of collectCreatedDiscountIds(validationResponse)) {
    cleanup.push(() => runGraphqlRaw(discountDeleteMutation, { id: discountId }));
  }
} finally {
  for (const cleanupStep of cleanup.reverse()) {
    try {
      cleanupResponses.push(await cleanupStep());
    } catch (error) {
      cleanupResponses.push({ error: error instanceof Error ? error.message : String(error) });
    }
  }
}

if (variables === undefined || validationResponse === undefined) {
  throw new Error('Capture did not complete validation variables/response.');
}

const fixture = {
  storeDomain,
  apiVersion,
  accessScopes: scopeProbe,
  setup: {
    products: setupProducts,
    collections: setupCollections,
  },
  validation: {
    query: document,
    variables,
    response: validationResponse,
  },
  cleanup: cleanupResponses.map((response) => {
    if (typeof response === 'object' && response !== null && 'payload' in response) {
      return (response as { payload: unknown }).payload;
    }

    return response;
  }),
  upstreamCalls,
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputPath,
      products: setupProducts.map((product) => product.id),
      collections: setupCollections.map((collectionRecord) => collectionRecord.id),
    },
    null,
    2,
  ),
);
