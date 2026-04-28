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
const outputPath = path.join(outputDir, 'discount-bxgy-lifecycle.json');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const userErrorsSelection = `#graphql
  userErrors {
    field
    message
    code
    extraInfo
  }
`;

const linkedItemsSelection = `#graphql
  __typename
  ... on DiscountProducts {
    products(first: 5) {
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    productVariants(first: 5) {
      nodes {
        id
        title
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
  ... on DiscountCollections {
    collections(first: 5) {
      nodes {
        id
        title
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

const codeBxgySelection = `#graphql
  codeDiscountNode {
    id
    codeDiscount {
      __typename
      ... on DiscountCodeBxgy {
        title
        status
        summary
        startsAt
        endsAt
        createdAt
        updatedAt
        asyncUsageCount
        discountClasses
        usageLimit
        usesPerOrderLimit
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
        customerBuys {
          value {
            __typename
            ... on DiscountQuantity {
              quantity
            }
          }
          items {
            ${linkedItemsSelection}
          }
        }
        customerGets {
          value {
            __typename
            ... on DiscountOnQuantity {
              quantity {
                quantity
              }
              effect {
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
            }
          }
          items {
            ${linkedItemsSelection}
          }
          appliesOnOneTimePurchase
          appliesOnSubscription
        }
      }
    }
  }
  ${userErrorsSelection}
`;

const automaticBxgySelection = `#graphql
  automaticDiscountNode {
    id
    automaticDiscount {
      __typename
      ... on DiscountAutomaticBxgy {
        title
        status
        summary
        startsAt
        endsAt
        createdAt
        updatedAt
        asyncUsageCount
        discountClasses
        usesPerOrderLimit
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
        customerBuys {
          value {
            __typename
            ... on DiscountQuantity {
              quantity
            }
          }
          items {
            ${linkedItemsSelection}
          }
        }
        customerGets {
          value {
            __typename
            ... on DiscountOnQuantity {
              quantity {
                quantity
              }
              effect {
                __typename
                ... on DiscountPercentage {
                  percentage
                }
              }
            }
          }
          items {
            ${linkedItemsSelection}
          }
          appliesOnOneTimePurchase
          appliesOnSubscription
        }
      }
    }
  }
  ${userErrorsSelection}
`;

const productCreateMutation = `#graphql
  mutation DiscountBxgyProductCreate($product: ProductCreateInput!) {
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
  mutation DiscountBxgyProductDelete($input: ProductDeleteInput!) {
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
  mutation DiscountBxgyCollectionCreate($input: CollectionInput!) {
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
  mutation DiscountBxgyCollectionAddProducts($id: ID!, $productIds: [ID!]!) {
    collectionAddProducts(id: $id, productIds: $productIds) {
      collection {
        id
        title
        products(first: 5) {
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

const collectionDeleteMutation = `#graphql
  mutation DiscountBxgyCollectionDelete($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const codeCreateMutation = `#graphql
  mutation DiscountCodeBxgyLifecycleCreate($input: DiscountCodeBxgyInput!) {
    discountCodeBxgyCreate(bxgyCodeDiscount: $input) {
      ${codeBxgySelection}
    }
  }
`;

const codeUpdateMutation = `#graphql
  mutation DiscountCodeBxgyLifecycleUpdate($id: ID!, $input: DiscountCodeBxgyInput!) {
    discountCodeBxgyUpdate(id: $id, bxgyCodeDiscount: $input) {
      ${codeBxgySelection}
    }
  }
`;

const codeDeactivateMutation = `#graphql
  mutation DiscountCodeBxgyLifecycleDeactivate($id: ID!) {
    discountCodeDeactivate(id: $id) {
      ${codeBxgySelection}
    }
  }
`;

const codeActivateMutation = `#graphql
  mutation DiscountCodeBxgyLifecycleActivate($id: ID!) {
    discountCodeActivate(id: $id) {
      ${codeBxgySelection}
    }
  }
`;

const codeDeleteMutation = `#graphql
  mutation DiscountCodeBxgyLifecycleDelete($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      ${userErrorsSelection}
    }
  }
`;

const automaticCreateMutation = `#graphql
  mutation DiscountAutomaticBxgyLifecycleCreate($input: DiscountAutomaticBxgyInput!) {
    discountAutomaticBxgyCreate(automaticBxgyDiscount: $input) {
      ${automaticBxgySelection}
    }
  }
`;

const automaticUpdateMutation = `#graphql
  mutation DiscountAutomaticBxgyLifecycleUpdate($id: ID!, $input: DiscountAutomaticBxgyInput!) {
    discountAutomaticBxgyUpdate(id: $id, automaticBxgyDiscount: $input) {
      ${automaticBxgySelection}
    }
  }
`;

const automaticDeactivateMutation = `#graphql
  mutation DiscountAutomaticBxgyLifecycleDeactivate($id: ID!) {
    discountAutomaticDeactivate(id: $id) {
      ${automaticBxgySelection}
    }
  }
`;

const automaticActivateMutation = `#graphql
  mutation DiscountAutomaticBxgyLifecycleActivate($id: ID!) {
    discountAutomaticActivate(id: $id) {
      ${automaticBxgySelection}
    }
  }
`;

const automaticDeleteMutation = `#graphql
  mutation DiscountAutomaticBxgyLifecycleDelete($id: ID!) {
    discountAutomaticDelete(id: $id) {
      deletedAutomaticDiscountId
      ${userErrorsSelection}
    }
  }
`;

const readMutation = `#graphql
  query DiscountBxgyLifecycleRead($codeId: ID!, $automaticId: ID!, $code: String!) {
    discountNode(id: $codeId) {
      id
      discount {
        __typename
        ... on DiscountCodeBxgy {
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
        ... on DiscountAutomaticBxgy {
          title
          status
        }
      }
    }
  }
`;

function assertNoUserErrors(pathLabel: string, userErrors: unknown): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`${pathLabel} returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

function readProduct(response: unknown): { id: string; variantId: string } {
  const product = (response as { data?: { productCreate?: { product?: { id?: unknown; variants?: unknown } } } }).data
    ?.productCreate?.product;
  const id = product?.id;
  const variantId = (product?.variants as { nodes?: Array<{ id?: unknown }> } | undefined)?.nodes?.[0]?.id;
  if (typeof id !== 'string' || typeof variantId !== 'string') {
    throw new Error(`productCreate did not return product and variant IDs: ${JSON.stringify(response)}`);
  }

  return { id, variantId };
}

function readCollectionId(response: unknown): string {
  const id = (response as { data?: { collectionCreate?: { collection?: { id?: unknown } } } }).data?.collectionCreate
    ?.collection?.id;
  if (typeof id !== 'string') {
    throw new Error(`collectionCreate did not return an id: ${JSON.stringify(response)}`);
  }

  return id;
}

function readCodeDiscountId(response: unknown): string {
  const id = (response as { data?: { discountCodeBxgyCreate?: { codeDiscountNode?: { id?: unknown } } } }).data
    ?.discountCodeBxgyCreate?.codeDiscountNode?.id;
  if (typeof id !== 'string') {
    throw new Error(`discountCodeBxgyCreate did not return an id: ${JSON.stringify(response)}`);
  }

  return id;
}

function readAutomaticDiscountId(response: unknown): string {
  const id = (response as { data?: { discountAutomaticBxgyCreate?: { automaticDiscountNode?: { id?: unknown } } } })
    .data?.discountAutomaticBxgyCreate?.automaticDiscountNode?.id;
  if (typeof id !== 'string') {
    throw new Error(`discountAutomaticBxgyCreate did not return an id: ${JSON.stringify(response)}`);
  }

  return id;
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const stamp = Date.now();
const startsAt = '2026-04-25T00:00:00Z';
const cleanup: Array<() => Promise<unknown>> = [];
let capture: Record<string, unknown> = {};

try {
  const buyProductVariables = {
    product: {
      title: `HAR-195 BXGY buy product ${stamp}`,
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'CONFORMANCE',
      tags: ['conformance', 'discount-bxgy', String(stamp)],
    },
  };
  const buyProductCreate = await runGraphql(productCreateMutation, buyProductVariables);
  const buyProduct = readProduct(buyProductCreate);
  cleanup.push(() => runGraphqlRaw(productDeleteMutation, { input: { id: buyProduct.id } }));

  const getProductVariables = {
    product: {
      title: `HAR-195 BXGY get product ${stamp}`,
      status: 'ACTIVE',
      vendor: 'HERMES',
      productType: 'CONFORMANCE',
      tags: ['conformance', 'discount-bxgy', String(stamp)],
    },
  };
  const getProductCreate = await runGraphql(productCreateMutation, getProductVariables);
  const getProduct = readProduct(getProductCreate);
  cleanup.push(() => runGraphqlRaw(productDeleteMutation, { input: { id: getProduct.id } }));

  const collectionVariables = {
    input: {
      title: `HAR-195 BXGY collection ${stamp}`,
    },
  };
  const collectionCreate = await runGraphql(collectionCreateMutation, collectionVariables);
  const collectionId = readCollectionId(collectionCreate);
  cleanup.push(() => runGraphqlRaw(collectionDeleteMutation, { input: { id: collectionId } }));
  const collectionAddProducts = await runGraphql(collectionAddProductsMutation, {
    id: collectionId,
    productIds: [getProduct.id],
  });

  const code = `HAR195BXGY${stamp}`;
  const codeCreateVariables = {
    input: {
      title: `HAR-195 code BXGY ${stamp}`,
      code,
      startsAt,
      combinesWith: {
        productDiscounts: true,
        orderDiscounts: false,
        shippingDiscounts: false,
      },
      context: {
        all: 'ALL',
      },
      customerBuys: {
        value: {
          quantity: '2',
        },
        items: {
          products: {
            productsToAdd: [buyProduct.id],
            productVariantsToAdd: [buyProduct.variantId],
          },
        },
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
          collections: {
            add: [collectionId],
          },
        },
      },
      usesPerOrderLimit: 1,
    },
  };
  const codeCreate = await runGraphql(codeCreateMutation, codeCreateVariables);
  const codeCreateData = codeCreate.data as
    | { discountCodeBxgyCreate?: { userErrors?: unknown; codeDiscountNode?: { id?: unknown } } }
    | undefined;
  assertNoUserErrors('discountCodeBxgyCreate', codeCreateData?.discountCodeBxgyCreate?.userErrors);
  const codeDiscountId = readCodeDiscountId(codeCreate);
  cleanup.push(() => runGraphqlRaw(codeDeleteMutation, { id: codeDiscountId }));

  const codeUpdateVariables = {
    id: codeDiscountId,
    input: {
      ...codeCreateVariables.input,
      title: `HAR-195 code BXGY updated ${stamp}`,
      code: `HAR195BXGYUP${stamp}`,
      customerGets: {
        value: {
          discountOnQuantity: {
            quantity: '2',
            effect: {
              percentage: 0.5,
            },
          },
        },
        items: {
          collections: {
            add: [collectionId],
          },
        },
      },
    },
  };
  const codeUpdate = await runGraphql(codeUpdateMutation, codeUpdateVariables);
  const codeUpdateData = codeUpdate.data as { discountCodeBxgyUpdate?: { userErrors?: unknown } } | undefined;
  assertNoUserErrors('discountCodeBxgyUpdate', codeUpdateData?.discountCodeBxgyUpdate?.userErrors);
  const codeDeactivate = await runGraphql(codeDeactivateMutation, { id: codeDiscountId });
  const codeActivate = await runGraphql(codeActivateMutation, { id: codeDiscountId });

  const automaticCreateVariables = {
    input: {
      title: `HAR-195 automatic BXGY ${stamp}`,
      startsAt,
      combinesWith: {
        productDiscounts: true,
        orderDiscounts: false,
        shippingDiscounts: false,
      },
      context: {
        all: 'ALL',
      },
      customerBuys: {
        value: {
          quantity: '1',
        },
        items: {
          collections: {
            add: [collectionId],
          },
        },
      },
      customerGets: {
        value: {
          discountOnQuantity: {
            quantity: '1',
            effect: {
              percentage: 0.5,
            },
          },
        },
        items: {
          products: {
            productsToAdd: [getProduct.id],
          },
        },
      },
      usesPerOrderLimit: '1',
    },
  };
  const automaticCreate = await runGraphql(automaticCreateMutation, automaticCreateVariables);
  const automaticCreateData = automaticCreate.data as
    | { discountAutomaticBxgyCreate?: { userErrors?: unknown; automaticDiscountNode?: { id?: unknown } } }
    | undefined;
  assertNoUserErrors('discountAutomaticBxgyCreate', automaticCreateData?.discountAutomaticBxgyCreate?.userErrors);
  const automaticDiscountId = readAutomaticDiscountId(automaticCreate);
  cleanup.push(() => runGraphqlRaw(automaticDeleteMutation, { id: automaticDiscountId }));

  const automaticUpdateVariables = {
    id: automaticDiscountId,
    input: {
      ...automaticCreateVariables.input,
      title: `HAR-195 automatic BXGY updated ${stamp}`,
      customerBuys: {
        value: {
          quantity: '3',
        },
        items: {
          collections: {
            add: [collectionId],
          },
        },
      },
    },
  };
  const automaticUpdate = await runGraphql(automaticUpdateMutation, automaticUpdateVariables);
  const automaticUpdateData = automaticUpdate.data as
    | { discountAutomaticBxgyUpdate?: { userErrors?: unknown } }
    | undefined;
  assertNoUserErrors('discountAutomaticBxgyUpdate', automaticUpdateData?.discountAutomaticBxgyUpdate?.userErrors);
  const automaticDeactivate = await runGraphql(automaticDeactivateMutation, { id: automaticDiscountId });
  const automaticActivate = await runGraphql(automaticActivateMutation, { id: automaticDiscountId });
  const read = await runGraphql(readMutation, {
    codeId: codeDiscountId,
    automaticId: automaticDiscountId,
    code: codeUpdateVariables.input.code,
  });

  const codeDelete = await runGraphqlRaw(codeDeleteMutation, { id: codeDiscountId });
  cleanup.pop();
  const automaticDelete = await runGraphqlRaw(automaticDeleteMutation, { id: automaticDiscountId });
  cleanup.pop();

  capture = {
    metadata: {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      runId: stamp,
    },
    prerequisites: {
      buyProduct: {
        variables: buyProductVariables,
        response: buyProductCreate,
      },
      getProduct: {
        variables: getProductVariables,
        response: getProductCreate,
      },
      collection: {
        variables: collectionVariables,
        response: collectionCreate,
      },
      collectionAddProducts,
    },
    code: {
      create: {
        variables: codeCreateVariables,
        response: codeCreate,
      },
      update: {
        variables: codeUpdateVariables,
        response: codeUpdate,
      },
      deactivate: codeDeactivate,
      activate: codeActivate,
      delete: codeDelete,
    },
    automatic: {
      create: {
        variables: automaticCreateVariables,
        response: automaticCreate,
      },
      update: {
        variables: automaticUpdateVariables,
        response: automaticUpdate,
      },
      deactivate: automaticDeactivate,
      activate: automaticActivate,
      delete: automaticDelete,
    },
    read,
  };
} finally {
  for (const cleanupStep of cleanup.reverse()) {
    try {
      await cleanupStep();
    } catch (error) {
      console.error(error instanceof Error ? error.message : String(error));
    }
  }
}

await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputPath,
    },
    null,
    2,
  ),
);
