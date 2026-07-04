/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

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

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const outputPath = path.join(outputDir, 'selling-plan-group-lifecycle.json');
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
            ... on MoneyV2 { amount currencyCode }
          }
        }
        ... on SellingPlanRecurringPricingPolicy {
          afterCycle
          createdAt
          adjustmentType
          adjustmentValue {
            __typename
            ... on SellingPlanPricingPolicyPercentageValue { percentage }
            ... on MoneyV2 { amount currencyCode }
          }
        }
      }
    }
    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
  }
`;

const schemaAndAccessQuery = `#graphql
  query SellingPlanGroupSchemaAndAccess {
    currentAppInstallation { accessScopes { handle } }
    groupInput: __type(name: "SellingPlanGroupInput") { inputFields { name } }
    resourceInput: __type(name: "SellingPlanGroupResourceInput") { inputFields { name } }
    groupUserError: __type(name: "SellingPlanGroupUserError") { fields { name } }
  }
`;

const productCreateMutation = `#graphql
  mutation CreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id title variants(first: 1) { nodes { id title } } }
      userErrors { field message }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation DeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) { deletedProductId userErrors { field message } }
  }
`;

const createGroupMutation = `#graphql
  mutation CreateSellingPlanGroup(
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

const updateGroupMutation = `#graphql
  mutation UpdateSellingPlanGroup($id: ID!, $input: SellingPlanGroupInput!, $productId: ID!, $variantId: ID!) {
    sellingPlanGroupUpdate(id: $id, input: $input) {
      deletedSellingPlanIds
      sellingPlanGroup { ${sellingPlanGroupFields} }
      userErrors { field message code }
    }
  }
`;

const addProductsMutation = `#graphql
  mutation AddProducts($id: ID!, $productIds: [ID!]!, $productId: ID!, $variantId: ID!) {
    sellingPlanGroupAddProducts(id: $id, productIds: $productIds) {
      sellingPlanGroup { ${sellingPlanGroupFields} }
      userErrors { field message code }
    }
  }
`;

const removeProductsMutation = `#graphql
  mutation RemoveProducts($id: ID!, $productIds: [ID!]!) {
    sellingPlanGroupRemoveProducts(id: $id, productIds: $productIds) {
      removedProductIds
      userErrors { field message code }
    }
  }
`;

const addVariantsMutation = `#graphql
  mutation AddVariants($id: ID!, $productVariantIds: [ID!]!, $productId: ID!, $variantId: ID!) {
    sellingPlanGroupAddProductVariants(id: $id, productVariantIds: $productVariantIds) {
      sellingPlanGroup { ${sellingPlanGroupFields} }
      userErrors { field message code }
    }
  }
`;

const removeVariantsMutation = `#graphql
  mutation RemoveVariants($id: ID!, $productVariantIds: [ID!]!) {
    sellingPlanGroupRemoveProductVariants(id: $id, productVariantIds: $productVariantIds) {
      removedProductVariantIds
      userErrors { field message code }
    }
  }
`;

const deleteGroupMutation = `#graphql
  mutation DeleteSellingPlanGroup($id: ID!) {
    sellingPlanGroupDelete(id: $id) {
      deletedSellingPlanGroupId
      userErrors { field message code }
    }
  }
`;

const readGroupQuery = `#graphql
  query ReadSellingPlanGroup($id: ID!, $productId: ID!, $variantId: ID!) {
    sellingPlanGroup(id: $id) { ${sellingPlanGroupFields} }
  }
`;

const catalogQuery = `#graphql
  query ReadSellingPlanGroups($productId: ID!, $variantId: ID!) {
    sellingPlanGroups(first: 5) {
      nodes { ${sellingPlanGroupFields} }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query DownstreamSellingPlanRead($productId: ID!, $variantId: ID!) {
    product(id: $productId) {
      id
      title
      requiresSellingPlan
      sellingPlanGroupsCount { count precision }
      sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
    }
    productVariant(id: $variantId) {
      id
      title
      sellingPlanGroupsCount { count precision }
      sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
    }
  }
`;

const connectionArgsQuery = `#graphql
  query SellingPlanGroupConnectionArgs(
    $productId: ID!
    $variantId: ID!
    $matchQuery: String!
    $betaQuery: String!
    $percentageQuery: String!
    $frequencyQuery: String!
    $categoryQuery: String!
    $unknownQuery: String!
  ) {
    defaultId: sellingPlanGroups(first: 3, query: $matchQuery) {
      nodes { id name merchantCode }
    }
    nameReverse: sellingPlanGroups(first: 2, query: $matchQuery, sortKey: NAME, reverse: true) {
      nodes { id name merchantCode }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    updatedReverse: sellingPlanGroups(first: 1, query: $matchQuery, sortKey: UPDATED_AT, reverse: true) {
      nodes { id name merchantCode }
    }
    betaOnly: sellingPlanGroups(first: 5, query: $betaQuery, sortKey: ID) {
      nodes { id name merchantCode }
    }
    percentageOnly: sellingPlanGroups(first: 5, query: $percentageQuery, sortKey: ID) {
      nodes { id name merchantCode }
    }
    frequencyMatch: sellingPlanGroups(first: 5, query: $frequencyQuery, sortKey: ID) {
      nodes { id name merchantCode }
    }
    categoryMatch: sellingPlanGroups(first: 5, query: $categoryQuery, sortKey: ID) {
      nodes { id name merchantCode }
    }
    unknownFilter: sellingPlanGroups(first: 5, query: $unknownQuery) {
      nodes { id name merchantCode }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    product(id: $productId) {
      id
      sellingPlanGroupsCount { count precision }
      sellingPlanGroups(first: 2, reverse: true) {
        nodes { id name merchantCode }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
    }
    productVariant(id: $variantId) {
      id
      sellingPlanGroupsCount { count precision }
      sellingPlanGroups(first: 2, reverse: true) {
        nodes { id name merchantCode }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
    }
  }
`;

const connectionAfterArgsQuery = `#graphql
  query SellingPlanGroupConnectionAfterArgs(
    $productId: ID!
    $variantId: ID!
    $topAfter: String!
    $productAfter: String!
    $variantAfter: String!
    $matchQuery: String!
  ) {
    afterWindow: sellingPlanGroups(first: 1, after: $topAfter, query: $matchQuery, sortKey: NAME, reverse: true) {
      nodes { id name merchantCode }
      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
    }
    product(id: $productId) {
      id
      sellingPlanGroups(first: 1, after: $productAfter, reverse: true) {
        nodes { id name merchantCode }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
    }
    productVariant(id: $variantId) {
      id
      sellingPlanGroups(first: 1, after: $variantAfter, reverse: true) {
        nodes { id name merchantCode }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
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

async function captureConnectionArgsWithRetry(variables: Record<string, unknown>): Promise<Capture> {
  let lastCapture: Capture | null = null;
  for (let attempt = 1; attempt <= 15; attempt += 1) {
    const result = await runGraphqlRaw(connectionArgsQuery, variables);
    assertHttpOk(result, `connection args read attempt ${attempt}`);
    lastCapture = {
      label: 'connection args read',
      request: { query: connectionArgsQuery, variables },
      status: result.status,
      response: result.payload,
    };
    const data = captureData(lastCapture);
    const topAfter = readObject(readObject(data['nameReverse'])['pageInfo'])['endCursor'];
    if (typeof topAfter === 'string' && topAfter.length > 0) {
      return lastCapture;
    }
    await sleep(2000);
  }
  throw new Error(`connection args read never returned a top-level cursor: ${JSON.stringify(lastCapture, null, 2)}`);
}

function captureData(captureResult: Capture): Record<string, unknown> {
  return readObject(readObject(captureResult.response)['data']);
}

function sellingPlanGroupFromCapture(captureResult: Capture): Record<string, unknown> {
  return readObject(readObject(captureData(captureResult)['sellingPlanGroupCreate'])['sellingPlanGroup']);
}

function connectionGroupInput(name: string, merchantCode: string, percentage?: number): Record<string, unknown> {
  const pricingPolicies =
    percentage === undefined ? [] : [{ fixed: { adjustmentType: 'PERCENTAGE', adjustmentValue: { percentage } } }];
  return {
    name,
    merchantCode,
    description: `Temporary connection-argument group ${name}`,
    position: 1,
    options: ['Delivery frequency'],
    sellingPlansToCreate: [
      {
        name: `${name} monthly`,
        description: `Temporary connection-argument plan ${name}`,
        options: ['Monthly'],
        position: 1,
        category: 'SUBSCRIPTION',
        billingPolicy: { recurring: { interval: 'MONTH', intervalCount: 1 } },
        deliveryPolicy: { recurring: { interval: 'MONTH', intervalCount: 1, cutoff: 0 } },
        inventoryPolicy: { reserve: 'ON_FULFILLMENT' },
        pricingPolicies,
      },
    ],
  };
}

const suffix = Date.now().toString(36);
let productId: string | null = null;
let variantId: string | null = null;
let groupId: string | null = null;
const extraGroupIds: string[] = [];
let seedProducts: unknown[] = [];
const captures: Capture[] = [];
const cleanup: Array<{ label: string; status: number; response: unknown }> = [];

try {
  captures.push(await capture('schema and access', schemaAndAccessQuery));
  captures.push(
    await capture('productCreate setup', productCreateMutation, {
      product: { title: `Selling plan conformance ${suffix}` },
    }),
  );
  const createdProduct = readObject(readObject(captureData(captures.at(-1)!)['productCreate'])['product']);
  productId = createdProduct['id'] as string;
  seedProducts = [createdProduct];
  const variantNodes = readArray(readObject(createdProduct['variants'])['nodes']);
  variantId = readObject(variantNodes[0])['id'] as string;

  const createInput = {
    name: `Subscription ${suffix}`,
    merchantCode: `selling-plan-${suffix}`,
    description: 'Temporary selling plan group for conformance capture',
    options: ['Delivery frequency'],
    position: 1,
    sellingPlansToCreate: [
      {
        name: 'Intro and recurring discount delivery',
        description: 'Ships every month with fixed and recurring discounts',
        options: ['Intro and recurring discount'],
        position: 1,
        category: 'SUBSCRIPTION',
        billingPolicy: { recurring: { interval: 'MONTH', intervalCount: 1, minCycles: 1, maxCycles: 12 } },
        deliveryPolicy: { recurring: { interval: 'MONTH', intervalCount: 1, cutoff: 0 } },
        inventoryPolicy: { reserve: 'ON_FULFILLMENT' },
        pricingPolicies: [
          { fixed: { adjustmentType: 'PERCENTAGE', adjustmentValue: { percentage: 10 } } },
          { recurring: { adjustmentType: 'PERCENTAGE', adjustmentValue: { percentage: 10 }, afterCycle: 1 } },
        ],
      },
      {
        name: 'Mixed discount delivery',
        description: 'Ships every month with fixed and recurring discounts',
        options: ['Mixed discount'],
        position: 2,
        category: 'SUBSCRIPTION',
        billingPolicy: { recurring: { interval: 'MONTH', intervalCount: 1, minCycles: 1, maxCycles: 12 } },
        deliveryPolicy: { recurring: { interval: 'MONTH', intervalCount: 1, cutoff: 0 } },
        inventoryPolicy: { reserve: 'ON_FULFILLMENT' },
        pricingPolicies: [
          { fixed: { adjustmentType: 'PERCENTAGE', adjustmentValue: { percentage: 10 } } },
          { recurring: { adjustmentType: 'PERCENTAGE', adjustmentValue: { percentage: 5 }, afterCycle: 2 } },
        ],
      },
    ],
  };

  captures.push(
    await capture('sellingPlanGroupCreate success', createGroupMutation, {
      input: createInput,
      resources: { productIds: [productId] },
      productId,
      variantId,
    }),
  );
  const createPayload = readObject(captureData(captures.at(-1)!)['sellingPlanGroupCreate']);
  if (!createPayload['sellingPlanGroup']) {
    throw new Error(`sellingPlanGroupCreate returned no group: ${JSON.stringify(createPayload, null, 2)}`);
  }
  const createdGroup = readObject(createPayload['sellingPlanGroup']);
  groupId = createdGroup['id'] as string;
  const planId = readObject(readArray(readObject(createdGroup['sellingPlans'])['nodes'])[0])['id'];

  captures.push(await capture('read after create', readGroupQuery, { id: groupId, productId, variantId }));
  captures.push(
    await capture('downstream after product create membership', downstreamReadQuery, { productId, variantId }),
  );
  captures.push(
    await capture('sellingPlanGroupRemoveProducts success', removeProductsMutation, {
      id: groupId,
      productIds: [productId],
    }),
  );
  captures.push(await capture('downstream after product removal', downstreamReadQuery, { productId, variantId }));
  captures.push(
    await capture('sellingPlanGroupAddProducts success', addProductsMutation, {
      id: groupId,
      productIds: [productId],
      productId,
      variantId,
    }),
  );
  captures.push(
    await capture('sellingPlanGroupRemoveProductVariants empty success', removeVariantsMutation, {
      id: groupId,
      productVariantIds: [variantId],
    }),
  );
  captures.push(
    await capture('sellingPlanGroupAddProductVariants success', addVariantsMutation, {
      id: groupId,
      productVariantIds: [variantId],
      productId,
      variantId,
    }),
  );
  captures.push(await capture('downstream after variant add', downstreamReadQuery, { productId, variantId }));
  captures.push(
    await capture('sellingPlanGroupUpdate success', updateGroupMutation, {
      id: groupId,
      input: {
        name: 'Subscription updated',
        merchantCode: 'selling-plan-updated',
        description: 'Updated temporary selling plan group for conformance capture',
        options: ['Delivery cadence'],
        position: 2,
        sellingPlansToUpdate: [
          { id: planId, name: 'Recurring discount updated', options: ['Every month'], position: 1 },
        ],
      },
      productId,
      variantId,
    }),
  );
  captures.push(await capture('catalog after update', catalogQuery, { productId, variantId }));
  const unknownId = 'gid://shopify/SellingPlanGroup/999999999999';
  captures.push(
    await capture('sellingPlanGroupUpdate unknown', updateGroupMutation, {
      id: unknownId,
      input: { name: 'Nope' },
      productId,
      variantId,
    }),
  );
  captures.push(
    await capture('sellingPlanGroupRemoveProducts unknown', removeProductsMutation, {
      id: unknownId,
      productIds: [productId],
    }),
  );
  captures.push(
    await capture('sellingPlanGroupRemoveProductVariants unknown', removeVariantsMutation, {
      id: unknownId,
      productVariantIds: [variantId],
    }),
  );
  captures.push(await capture('sellingPlanGroupDelete success', deleteGroupMutation, { id: groupId }));
  captures.push(await capture('read after delete', readGroupQuery, { id: groupId, productId, variantId }));

  const connectionToken = `connection${suffix}`;
  captures.push(
    await capture('connection setup alpha group', createGroupMutation, {
      input: connectionGroupInput(`${connectionToken}alpha`, `${connectionToken}-alpha`),
      resources: { productIds: [productId] },
      productId,
      variantId,
    }),
  );
  const alphaGroupId = sellingPlanGroupFromCapture(captures.at(-1)!)['id'] as string;
  extraGroupIds.push(alphaGroupId);
  captures.push(
    await capture('connection setup beta group', createGroupMutation, {
      input: connectionGroupInput(`${connectionToken}beta`, `${connectionToken}-beta`, 15),
      resources: { productIds: [productId] },
      productId,
      variantId,
    }),
  );
  const betaGroupId = sellingPlanGroupFromCapture(captures.at(-1)!)['id'] as string;
  extraGroupIds.push(betaGroupId);
  captures.push(
    await capture('connection setup gamma group', createGroupMutation, {
      input: connectionGroupInput(`${connectionToken}gamma`, `${connectionToken}-gamma`),
      resources: { productIds: [productId] },
      productId,
      variantId,
    }),
  );
  const gammaGroupId = sellingPlanGroupFromCapture(captures.at(-1)!)['id'] as string;
  extraGroupIds.push(gammaGroupId);
  await sleep(1500);
  captures.push(
    await capture('connection beta update for updatedAt sort', updateGroupMutation, {
      id: betaGroupId,
      input: { description: `Updated ${connectionToken}beta for UPDATED_AT sort` },
      productId,
      variantId,
    }),
  );
  captures.push(
    await captureConnectionArgsWithRetry({
      productId,
      variantId,
      matchQuery: connectionToken,
      betaQuery: `name:${connectionToken}beta`,
      percentageQuery: `name:${connectionToken}beta AND percentage_off:15`,
      frequencyQuery: `${connectionToken} AND delivery_frequency:MONTH`,
      categoryQuery: `${connectionToken} AND category:SUBSCRIPTION`,
      unknownQuery: `name:${connectionToken} AND unknown_filter:beta`,
    }),
  );
  const connectionReadData = captureData(captures.at(-1)!);
  const topAfter = readObject(readObject(connectionReadData['nameReverse'])['pageInfo'])['endCursor'];
  const productAfter = readObject(
    readObject(readObject(connectionReadData['product'])['sellingPlanGroups'])['pageInfo'],
  )['endCursor'];
  const variantAfter = readObject(
    readObject(readObject(connectionReadData['productVariant'])['sellingPlanGroups'])['pageInfo'],
  )['endCursor'];
  captures.push(
    await capture('connection after-window read', connectionAfterArgsQuery, {
      productId,
      variantId,
      topAfter,
      productAfter,
      variantAfter,
      matchQuery: connectionToken,
    }),
  );
} finally {
  for (const id of extraGroupIds) {
    const result = await runGraphqlRaw(deleteGroupMutation, { id });
    cleanup.push({ label: 'cleanup extra sellingPlanGroupDelete', status: result.status, response: result.payload });
  }
  if (groupId) {
    const result = await runGraphqlRaw(deleteGroupMutation, { id: groupId });
    cleanup.push({ label: 'cleanup sellingPlanGroupDelete', status: result.status, response: result.payload });
  }
  if (productId) {
    const result = await runGraphqlRaw(productDeleteMutation, { input: { id: productId } });
    cleanup.push({ label: 'cleanup productDelete', status: result.status, response: result.payload });
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Captures selling-plan group read/mutation payloads and downstream product/variant membership reads.',
        'The script creates a disposable product and selling-plan group, then deletes both during cleanup.',
      ],
      productId,
      variantId,
      groupId,
      seedProducts,
      captures,
      cleanup,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`Wrote selling-plan group conformance fixture to ${outputPath}`);
