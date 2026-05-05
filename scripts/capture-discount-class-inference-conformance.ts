/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlPayload = {
  data?: Record<string, unknown>;
  errors?: unknown;
  extensions?: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-class-inference.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const runId = Date.now();
const titlePrefix = `HAR597CLASS${runId}`;
const startsAt = new Date(Date.now() - 60_000).toISOString();
const productQuery = `discount_class:product ${titlePrefix}`;

const userErrorsSelection = `#graphql
  userErrors {
    field
    message
    code
    extraInfo
  }
`;

const productCreateMutation = `#graphql
  mutation DiscountClassInferenceProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
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

const productDeleteMutation = `#graphql
  mutation DiscountClassInferenceProductDelete($input: ProductDeleteInput!) {
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
  mutation DiscountClassInferenceCollectionCreate($input: CollectionInput!) {
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

const collectionAddProductsMutation = `#graphql
  mutation DiscountClassInferenceCollectionAddProducts($id: ID!, $productIds: [ID!]!) {
    collectionAddProducts(id: $id, productIds: $productIds) {
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
  mutation DiscountClassInferenceCollectionDelete($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const createMutation = `#graphql
  mutation DiscountClassInferenceCreate(
    $basicAll: DiscountCodeBasicInput!
    $basicProduct: DiscountCodeBasicInput!
    $basicCollection: DiscountCodeBasicInput!
    $bxgy: DiscountCodeBxgyInput!
    $freeShipping: DiscountCodeFreeShippingInput!
  ) {
    basicAll: discountCodeBasicCreate(basicCodeDiscount: $basicAll) {
      codeDiscountNode {
        id
        codeDiscount {
          __typename
          ... on DiscountCodeBasic {
            title
            discountClasses
          }
        }
      }
      ${userErrorsSelection}
    }
    basicProduct: discountCodeBasicCreate(basicCodeDiscount: $basicProduct) {
      codeDiscountNode {
        id
        codeDiscount {
          __typename
          ... on DiscountCodeBasic {
            title
            discountClasses
          }
        }
      }
      ${userErrorsSelection}
    }
    basicCollection: discountCodeBasicCreate(basicCodeDiscount: $basicCollection) {
      codeDiscountNode {
        id
        codeDiscount {
          __typename
          ... on DiscountCodeBasic {
            title
            discountClasses
          }
        }
      }
      ${userErrorsSelection}
    }
    bxgy: discountCodeBxgyCreate(bxgyCodeDiscount: $bxgy) {
      codeDiscountNode {
        id
        codeDiscount {
          __typename
          ... on DiscountCodeBxgy {
            title
            discountClasses
          }
        }
      }
      ${userErrorsSelection}
    }
    freeShipping: discountCodeFreeShippingCreate(freeShippingCodeDiscount: $freeShipping) {
      codeDiscountNode {
        id
        codeDiscount {
          __typename
          ... on DiscountCodeFreeShipping {
            title
            discountClasses
          }
        }
      }
      ${userErrorsSelection}
    }
  }
`;

const readProductClassQuery = `#graphql
  query DiscountClassInferenceRead($productQuery: String!) {
    discountNodesCount(query: $productQuery) {
      count
      precision
    }
  }
`;

const discountDeleteMutation = `#graphql
  mutation DiscountClassInferenceDiscountDelete($id: ID!) {
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

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (typeof current !== 'object' || current === null || !(segment in current)) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

function readRequiredString(value: unknown, pathSegments: string[], label: string): string {
  const found = readPath(value, pathSegments);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`${label} did not return a string at ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
  }
  return found;
}

function readUserErrors(value: unknown, pathSegments: string[]): unknown[] {
  const found = readPath(value, pathSegments);
  return Array.isArray(found) ? found : [];
}

function assertNoUserErrors(value: unknown, pathSegments: string[], label: string): void {
  const userErrors = readUserErrors(value, pathSegments);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function assertGraphqlSuccess(payload: GraphqlPayload, label: string): void {
  if (payload.errors) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(payload.errors)}`);
  }
}

function discountId(payload: GraphqlPayload, alias: string): string {
  return readRequiredString(payload, ['data', alias, 'codeDiscountNode', 'id'], alias);
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function readProductClassUntilIndexed(variables: { productQuery: string }): Promise<GraphqlPayload> {
  let lastResponse: GraphqlPayload | null = null;
  for (let attempt = 0; attempt < 10; attempt += 1) {
    lastResponse = await runGraphql(readProductClassQuery, variables);
    assertGraphqlSuccess(lastResponse, 'discount_class product read');
    const productClassCount = readPath(lastResponse, ['data', 'discountNodesCount', 'count']);
    if (productClassCount === 3) {
      return lastResponse;
    }
    await sleep(2_000);
  }

  const finalCount = readPath(lastResponse, ['data', 'discountNodesCount', 'count']);
  throw new Error(`Expected product discount_class query to isolate three captured discounts, got ${finalCount}`);
}

const cleanup: Array<() => Promise<unknown>> = [];
let setupProductResponse: GraphqlPayload | null = null;
let bxgyBuyProductResponse: GraphqlPayload | null = null;
let collectionCreateResponse: GraphqlPayload | null = null;
let collectionAddProductsResponse: GraphqlPayload | null = null;
let createResponse: GraphqlPayload | null = null;
let readProductClassResponse: GraphqlPayload | null = null;
let cleanupResponse: unknown[] = [];

try {
  setupProductResponse = await runGraphql(productCreateMutation, {
    product: { title: `${titlePrefix} product entitlement` },
  });
  assertNoUserErrors(setupProductResponse, ['data', 'productCreate', 'userErrors'], 'productCreate entitlement');
  const productId = readRequiredString(
    setupProductResponse,
    ['data', 'productCreate', 'product', 'id'],
    'productCreate',
  );
  cleanup.push(() => runGraphqlRaw(productDeleteMutation, { input: { id: productId } }));

  bxgyBuyProductResponse = await runGraphql(productCreateMutation, {
    product: { title: `${titlePrefix} bxgy buy` },
  });
  assertNoUserErrors(bxgyBuyProductResponse, ['data', 'productCreate', 'userErrors'], 'productCreate bxgy buy');
  const bxgyBuyProductId = readRequiredString(
    bxgyBuyProductResponse,
    ['data', 'productCreate', 'product', 'id'],
    'bxgy productCreate',
  );
  cleanup.push(() => runGraphqlRaw(productDeleteMutation, { input: { id: bxgyBuyProductId } }));

  collectionCreateResponse = await runGraphql(collectionCreateMutation, {
    input: { title: `${titlePrefix} collection entitlement` },
  });
  assertNoUserErrors(collectionCreateResponse, ['data', 'collectionCreate', 'userErrors'], 'collectionCreate');
  const collectionId = readRequiredString(
    collectionCreateResponse,
    ['data', 'collectionCreate', 'collection', 'id'],
    'collectionCreate',
  );
  cleanup.push(() => runGraphqlRaw(collectionDeleteMutation, { input: { id: collectionId } }));

  collectionAddProductsResponse = await runGraphql(collectionAddProductsMutation, {
    id: collectionId,
    productIds: [productId],
  });
  assertNoUserErrors(
    collectionAddProductsResponse,
    ['data', 'collectionAddProducts', 'userErrors'],
    'collectionAddProducts',
  );

  const createVariables = {
    basicAll: {
      title: `${titlePrefix} basic order`,
      code: `HAR597ORDER${runId}`,
      startsAt,
      context: {
        all: 'ALL',
      },
      customerGets: {
        value: { percentage: 0.1 },
        items: { all: true },
      },
    },
    basicProduct: {
      title: `${titlePrefix} basic product`,
      code: `HAR597PRODUCT${runId}`,
      startsAt,
      context: {
        all: 'ALL',
      },
      customerGets: {
        value: { percentage: 0.1 },
        items: { products: { productsToAdd: [productId] } },
      },
    },
    basicCollection: {
      title: `${titlePrefix} basic collection`,
      code: `HAR597COLL${runId}`,
      startsAt,
      context: {
        all: 'ALL',
      },
      customerGets: {
        value: { percentage: 0.1 },
        items: { collections: { add: [collectionId] } },
      },
    },
    bxgy: {
      title: `${titlePrefix} bxgy product`,
      code: `HAR597BXGY${runId}`,
      startsAt,
      context: {
        all: 'ALL',
      },
      customerBuys: {
        value: { quantity: '1' },
        items: { products: { productsToAdd: [bxgyBuyProductId] } },
      },
      customerGets: {
        value: {
          discountOnQuantity: {
            quantity: '1',
            effect: { percentage: 0.5 },
          },
        },
        items: { products: { productsToAdd: [productId] } },
      },
    },
    freeShipping: {
      title: `${titlePrefix} free shipping`,
      code: `HAR597SHIP${runId}`,
      startsAt,
      context: {
        all: 'ALL',
      },
      destination: { all: true },
    },
  } satisfies Record<string, unknown>;

  createResponse = await runGraphql(createMutation, createVariables);
  assertGraphqlSuccess(createResponse, 'discount class create');
  for (const alias of ['basicAll', 'basicProduct', 'basicCollection', 'bxgy', 'freeShipping']) {
    assertNoUserErrors(createResponse, ['data', alias, 'userErrors'], alias);
    const id = discountId(createResponse, alias);
    cleanup.push(() => runGraphqlRaw(discountDeleteMutation, { id }));
  }

  const readProductClassVariables = { productQuery };
  readProductClassResponse = await readProductClassUntilIndexed(readProductClassVariables);

  cleanupResponse = await Promise.allSettled([...cleanup].reverse().map((run) => run()));

  const fixture = {
    variables: {
      runId,
      titlePrefix,
      productQuery,
    },
    requests: {
      setupProduct: {
        query: productCreateMutation,
        variables: { product: { title: `${titlePrefix} product entitlement` } },
        response: setupProductResponse,
      },
      setupBxgyBuyProduct: {
        query: productCreateMutation,
        variables: { product: { title: `${titlePrefix} bxgy buy` } },
        response: bxgyBuyProductResponse,
      },
      setupCollection: {
        query: collectionCreateMutation,
        variables: { input: { title: `${titlePrefix} collection entitlement` } },
        response: collectionCreateResponse,
      },
      setupCollectionProducts: {
        query: collectionAddProductsMutation,
        response: collectionAddProductsResponse,
      },
      create: {
        query: createMutation,
        variables: createVariables,
      },
      readProductClass: {
        query: readProductClassQuery,
        variables: readProductClassVariables,
      },
    },
    create: {
      payload: createResponse,
    },
    readProductClass: {
      payload: readProductClassResponse,
    },
    cleanup: cleanupResponse,
    upstreamCalls: [],
  };

  await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  if (createResponse) {
    await Promise.allSettled(
      ['basicAll', 'basicProduct', 'basicCollection', 'bxgy', 'freeShipping'].map(async (alias) => {
        const id = readPath(createResponse, ['data', alias, 'codeDiscountNode', 'id']);
        if (typeof id === 'string' && id.length > 0) {
          await runGraphqlRaw(discountDeleteMutation, { id });
        }
      }),
    );
  }
  cleanupResponse = await Promise.allSettled([...cleanup].reverse().map((run) => run()));
  console.error('Cleanup after failed capture:', JSON.stringify(cleanupResponse, null, 2));
  throw error;
}
