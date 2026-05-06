/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  label: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

type ProductSnapshot = {
  id: string;
  title: string;
  variantId: string;
  variantTitle: string;
};

type UpstreamCall = {
  operationName: string;
  variables: { ids: string[] };
  query: string;
  response: {
    status: number;
    body: {
      data: {
        nodes: Array<Record<string, unknown> | null>;
      };
    };
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'selling-plan-group-add-remove-validation.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const sellingPlanGroupFields = `#graphql
  id
  appId
  name
  merchantCode
  description
  options
  position
  summary
  createdAt
  productsCount { count precision }
  productVariantsCount { count precision }
  appliesToProduct(productId: $productId)
  appliesToProductVariant(productVariantId: $variantId)
  appliesToProductVariants(productId: $productId)
  products(first: 5) { nodes { id title } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
  productVariants(first: 5) {
    nodes { id title product { id } }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
  sellingPlans(first: 5) {
    nodes {
      id
      name
      description
      options
      position
      category
      createdAt
      billingPolicy {
        __typename
        ... on SellingPlanRecurringBillingPolicy { interval intervalCount minCycles maxCycles }
      }
      deliveryPolicy {
        __typename
        ... on SellingPlanRecurringDeliveryPolicy { interval intervalCount cutoff intent preAnchorBehavior }
      }
      inventoryPolicy { reserve }
      pricingPolicies {
        __typename
        ... on SellingPlanFixedPricingPolicy {
          adjustmentType
          adjustmentValue {
            __typename
            ... on SellingPlanPricingPolicyPercentageValue { percentage }
          }
        }
      }
    }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
`;

const productCreateMutation = `#graphql
  mutation CreateValidationProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id title variants(first: 1) { nodes { id title } } }
      userErrors { field message }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation DeleteValidationProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) { deletedProductId userErrors { field message } }
  }
`;

const createGroupMutation = `#graphql
  mutation CreateSellingPlanGroupValidation(
    $input: SellingPlanGroupInput!
    $resources: SellingPlanGroupResourceInput
    $productId: ID!
    $variantId: ID!
  ) {
    sellingPlanGroupCreate(input: $input, resources: $resources) {
      sellingPlanGroup { ${sellingPlanGroupFields} }
      userErrors { field message code }
    }
  }
`;

const addProductsMutation = `#graphql
  mutation AddProductsValidation($id: ID!, $productIds: [ID!]!, $productId: ID!, $variantId: ID!) {
    sellingPlanGroupAddProducts(id: $id, productIds: $productIds) {
      sellingPlanGroup { ${sellingPlanGroupFields} }
      userErrors { field message code }
    }
  }
`;

const addVariantsMutation = `#graphql
  mutation AddVariantsValidation($id: ID!, $productVariantIds: [ID!]!, $productId: ID!, $variantId: ID!) {
    sellingPlanGroupAddProductVariants(id: $id, productVariantIds: $productVariantIds) {
      sellingPlanGroup { ${sellingPlanGroupFields} }
      userErrors { field message code }
    }
  }
`;

const removeProductsMutation = `#graphql
  mutation RemoveProductsValidation($id: ID!, $productIds: [ID!]!) {
    sellingPlanGroupRemoveProducts(id: $id, productIds: $productIds) {
      removedProductIds
      userErrors { field message code }
    }
  }
`;

const removeVariantsMutation = `#graphql
  mutation RemoveVariantsValidation($id: ID!, $productVariantIds: [ID!]!) {
    sellingPlanGroupRemoveProductVariants(id: $id, productVariantIds: $productVariantIds) {
      removedProductVariantIds
      userErrors { field message code }
    }
  }
`;

const deleteGroupMutation = `#graphql
  mutation DeleteSellingPlanGroupValidation($id: ID!) {
    sellingPlanGroupDelete(id: $id) {
      deletedSellingPlanGroupId
      userErrors { field message code }
    }
  }
`;

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Expected object in conformance capture response.');
  }
  return value as Record<string, unknown>;
}

function readArray(value: unknown): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error('Expected array in conformance capture response.');
  }
  return value;
}

async function capture(label: string, query: string, variables: Record<string, unknown> = {}): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

async function captureAllowGraphqlErrors(
  label: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return {
    label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function captureData(captureResult: Capture): Record<string, unknown> {
  return readObject(readObject(captureResult.response)['data']);
}

function productSnapshotFromCreate(captureResult: Capture): ProductSnapshot {
  const product = readObject(readObject(captureData(captureResult)['productCreate'])['product']);
  const variant = readObject(readArray(readObject(product['variants'])['nodes'])[0]);
  return {
    id: product['id'] as string,
    title: product['title'] as string,
    variantId: variant['id'] as string,
    variantTitle: variant['title'] as string,
  };
}

function productHydrationNode(product: ProductSnapshot): Record<string, unknown> {
  return {
    __typename: 'Product',
    id: product.id,
    title: product.title,
    variants: {
      nodes: [
        {
          id: product.variantId,
          title: product.variantTitle,
        },
      ],
    },
  };
}

function variantHydrationNode(product: ProductSnapshot): Record<string, unknown> {
  return {
    __typename: 'ProductVariant',
    id: product.variantId,
    title: product.variantTitle,
    product: {
      id: product.id,
    },
  };
}

function upstreamCall(ids: string[], nodes: Array<Record<string, unknown> | null>): UpstreamCall {
  return {
    operationName: 'ProductsHydrateNodes',
    variables: { ids },
    query: 'hand-synthesized from live setup product evidence for mutation hydration',
    response: {
      status: 200,
      body: {
        data: { nodes },
      },
    },
  };
}

const suffix = Date.now().toString(36);
let memberProduct: ProductSnapshot | null = null;
let nonMemberProduct: ProductSnapshot | null = null;
let groupId: string | null = null;
const captures: Capture[] = [];
const cleanup: Array<{ label: string; status: number; response: unknown }> = [];

const unknownProductId = 'gid://shopify/Product/999999999999';
const unknownVariantId = 'gid://shopify/ProductVariant/999999999999';
const unknownGroupId = 'gid://shopify/SellingPlanGroup/999999999999';
const malformedProductId = 'not-a-product-gid';
const malformedVariantId = 'not-a-product-variant-gid';

function sellingPlanGroupInput(label: string): Record<string, unknown> {
  return {
    name: `Selling plan ${label} ${suffix}`,
    merchantCode: `selling-plan-${label}-${suffix}`,
    description: `Temporary selling plan group for ${label} capture`,
    options: ['Delivery frequency'],
    position: 1,
    sellingPlansToCreate: [
      {
        name: 'Monthly delivery',
        description: 'Ships every month',
        options: ['Monthly'],
        position: 1,
        category: 'SUBSCRIPTION',
        billingPolicy: { recurring: { interval: 'MONTH', intervalCount: 1, minCycles: 1, maxCycles: 12 } },
        deliveryPolicy: { recurring: { interval: 'MONTH', intervalCount: 1, cutoff: 0 } },
        inventoryPolicy: { reserve: 'ON_FULFILLMENT' },
        pricingPolicies: [{ fixed: { adjustmentType: 'PERCENTAGE', adjustmentValue: { percentage: 10 } } }],
      },
    ],
  };
}

try {
  captures.push(
    await capture('member productCreate setup', productCreateMutation, {
      product: { title: `Selling plan validation member ${suffix}` },
    }),
  );
  memberProduct = productSnapshotFromCreate(captures.at(-1)!);

  captures.push(
    await capture('non-member productCreate setup', productCreateMutation, {
      product: { title: `Selling plan validation non-member ${suffix}` },
    }),
  );
  nonMemberProduct = productSnapshotFromCreate(captures.at(-1)!);

  captures.push(
    await capture('sellingPlanGroupCreate validation setup', createGroupMutation, {
      input: sellingPlanGroupInput('validation'),
      resources: { productIds: [memberProduct.id], productVariantIds: [memberProduct.variantId] },
      productId: memberProduct.id,
      variantId: memberProduct.variantId,
    }),
  );
  const createdGroup = readObject(
    readObject(captureData(captures.at(-1)!)['sellingPlanGroupCreate'])['sellingPlanGroup'],
  );
  groupId = createdGroup['id'] as string;

  captures.push(
    await capture('sellingPlanGroupAddProducts duplicate', addProductsMutation, {
      id: groupId,
      productIds: [memberProduct.id],
      productId: memberProduct.id,
      variantId: memberProduct.variantId,
    }),
  );
  captures.push(
    await capture('sellingPlanGroupAddProductVariants duplicate', addVariantsMutation, {
      id: groupId,
      productVariantIds: [memberProduct.variantId],
      productId: memberProduct.id,
      variantId: memberProduct.variantId,
    }),
  );
  captures.push(
    await capture('sellingPlanGroupAddProducts unknown product', addProductsMutation, {
      id: groupId,
      productIds: [unknownProductId],
      productId: memberProduct.id,
      variantId: memberProduct.variantId,
    }),
  );
  captures.push(
    await capture('sellingPlanGroupAddProductVariants unknown variant', addVariantsMutation, {
      id: groupId,
      productVariantIds: [unknownVariantId],
      productId: memberProduct.id,
      variantId: memberProduct.variantId,
    }),
  );
  captures.push(
    await capture('sellingPlanGroupRemoveProducts known non-member', removeProductsMutation, {
      id: groupId,
      productIds: [nonMemberProduct.id],
    }),
  );
  captures.push(
    await capture('sellingPlanGroupRemoveProductVariants known non-member', removeVariantsMutation, {
      id: groupId,
      productVariantIds: [nonMemberProduct.variantId],
    }),
  );
  captures.push(
    await capture('sellingPlanGroupRemoveProducts unknown product', removeProductsMutation, {
      id: groupId,
      productIds: [unknownProductId],
    }),
  );
  captures.push(
    await capture('sellingPlanGroupRemoveProductVariants unknown variant', removeVariantsMutation, {
      id: groupId,
      productVariantIds: [unknownVariantId],
    }),
  );
  captures.push(
    await captureAllowGraphqlErrors('sellingPlanGroupRemoveProducts malformed id', removeProductsMutation, {
      id: groupId,
      productIds: [malformedProductId],
    }),
  );
  captures.push(
    await captureAllowGraphqlErrors('sellingPlanGroupRemoveProductVariants malformed id', removeVariantsMutation, {
      id: groupId,
      productVariantIds: [malformedVariantId],
    }),
  );
  captures.push(
    await capture('sellingPlanGroupAddProducts unknown group', addProductsMutation, {
      id: unknownGroupId,
      productIds: [memberProduct.id],
      productId: memberProduct.id,
      variantId: memberProduct.variantId,
    }),
  );
} finally {
  if (groupId) {
    const result = await runGraphqlRaw(deleteGroupMutation, { id: groupId });
    cleanup.push({
      label: `cleanup sellingPlanGroupDelete ${groupId}`,
      status: result.status,
      response: result.payload,
    });
  }
  for (const product of [memberProduct, nonMemberProduct]) {
    if (product) {
      const result = await runGraphqlRaw(productDeleteMutation, { input: { id: product.id } });
      cleanup.push({ label: `cleanup productDelete ${product.id}`, status: result.status, response: result.payload });
    }
  }
}

if (!memberProduct || !nonMemberProduct) {
  throw new Error('Capture did not create the required product setup.');
}

const upstreamCalls = [
  upstreamCall(
    [memberProduct.id, memberProduct.variantId],
    [productHydrationNode(memberProduct), variantHydrationNode(memberProduct)],
  ),
  upstreamCall([unknownProductId], [null]),
  upstreamCall([unknownVariantId], [null]),
  upstreamCall([nonMemberProduct.id], [productHydrationNode(nonMemberProduct)]),
  upstreamCall([nonMemberProduct.variantId], [variantHydrationNode(nonMemberProduct)]),
];

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Captures selling-plan group product and product-variant add/remove validation branches.',
        'The script creates disposable products and a disposable selling-plan group, then deletes them during cleanup.',
      ],
      memberProduct,
      nonMemberProduct,
      groupId,
      unknownProductId,
      unknownVariantId,
      unknownGroupId,
      malformedProductId,
      malformedVariantId,
      captures,
      cleanup,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote selling-plan group add/remove validation fixture to ${outputPath}`);
