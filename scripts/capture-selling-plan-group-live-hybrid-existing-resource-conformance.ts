/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import {
  createConformanceCapture,
  readArray,
  readRecord,
  requireString,
  type JsonRecord,
} from './conformance-capture-lib.js';

type Scenario = {
  label: string;
  query: string;
  variables: JsonRecord;
  status: number;
  response: unknown;
};

const capture = await createConformanceCapture();
const createGroupMutation = await capture.readRequest('products', 'selling-plan-group-create.graphql');
const readGroupQuery = await capture.readRequest('products', 'selling-plan-group-read.graphql');
const updateGroupMutation = await capture.readRequest('products', 'selling-plan-group-update.graphql');
const downstreamReadQuery = await capture.readRequest('products', 'selling-plan-group-downstream-read.graphql');

const productCreateMutation = `#graphql
  mutation CreateProduct($product: ProductCreateInput!) {
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
  mutation DeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteGroupMutation = `#graphql
  mutation DeleteSellingPlanGroup($id: ID!) {
    sellingPlanGroupDelete(id: $id) {
      deletedSellingPlanGroupId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const sellingPlanGroupHydrateNodesQuery = `
query sellingPlanGroupHydrateNodes($ids: [ID!]!) {
  nodes(ids: $ids) {
    __typename
    id
    ... on SellingPlanGroup {
      appId
      name
      merchantCode
      description
      options
      position
      createdAt
      products(first: 250) {
        edges {
          cursor
          node {
          __typename
          id
          title
          handle
          status
          createdAt
          updatedAt
          variants(first: 50) {
            edges {
              cursor
              node {
              __typename
              id
              title
              sku
              barcode
              price
              compareAtPrice
              taxable
              inventoryPolicy
              inventoryQuantity
              selectedOptions { name value }
              inventoryItem { id tracked requiresShipping }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          }
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      productVariants(first: 250) {
        edges {
          cursor
          node {
          __typename
          id
          title
          sku
          barcode
          price
          compareAtPrice
          taxable
          inventoryPolicy
          inventoryQuantity
          selectedOptions { name value }
          inventoryItem { id tracked requiresShipping }
          product { id title handle status createdAt updatedAt }
          }
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
      sellingPlans(first: 250) {
        edges {
          cursor
          node {
          __typename
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
        }
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
      }
    }
  }
}
`;

async function runScenario(label: string, query: string, variables: JsonRecord): Promise<Scenario> {
  const result = await capture.runGraphqlRequest(query, variables);
  if (result.status < 200 || result.status >= 300 || readRecord(result.payload)?.['errors']) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
  return {
    label,
    query,
    variables,
    status: result.status,
    response: result.payload,
  };
}

function mutationRoot(scenario: Scenario, rootName: string): JsonRecord {
  const payload = readRecord(scenario.response);
  if (!payload) throw new Error(`${scenario.label} returned non-object payload.`);
  return capture.mutationRoot(payload, rootName, scenario.label);
}

function dataRecord(scenario: Scenario): JsonRecord {
  const payload = readRecord(scenario.response);
  const data = readRecord(payload?.['data']);
  if (!data) throw new Error(`${scenario.label} missing data: ${JSON.stringify(scenario.response, null, 2)}`);
  return data;
}

function sellingPlanGroupInput(suffix: string): JsonRecord {
  return {
    name: `LiveHybrid existing ${suffix}`,
    merchantCode: `live-hybrid-existing-${suffix}`,
    description: 'Disposable selling-plan group for cold LiveHybrid parity',
    options: ['Delivery frequency'],
    position: 1,
    sellingPlansToCreate: [
      {
        name: 'Monthly delivery',
        description: 'Ships every month',
        options: ['Monthly'],
        position: 1,
        category: 'SUBSCRIPTION',
        billingPolicy: {
          recurring: {
            interval: 'MONTH',
            intervalCount: 1,
            minCycles: 1,
            maxCycles: 12,
          },
        },
        deliveryPolicy: {
          recurring: {
            interval: 'MONTH',
            intervalCount: 1,
            cutoff: 0,
          },
        },
        inventoryPolicy: {
          reserve: 'ON_FULFILLMENT',
        },
        pricingPolicies: [
          {
            fixed: {
              adjustmentType: 'PERCENTAGE',
              adjustmentValue: { percentage: 10 },
            },
          },
        ],
      },
    ],
  };
}

const scenarios: Record<string, Scenario> = {};
const cleanup: Record<string, Scenario> = {};
let productId: string | null = null;
let variantId: string | null = null;
let groupId: string | null = null;
let planId: string | null = null;

try {
  scenarios['productCreate'] = await runScenario('productCreate', productCreateMutation, {
    product: { title: `Selling plan LiveHybrid parity ${capture.stamp}` },
  });
  const product = readRecord(mutationRoot(scenarios['productCreate'], 'productCreate')['product']);
  if (!product) {
    throw new Error(`productCreate returned no product: ${JSON.stringify(scenarios['productCreate'], null, 2)}`);
  }
  productId = requireString(product['id'], 'productCreate.product.id');
  const variantNodes = readArray(readRecord(product['variants'])?.['nodes']);
  variantId = requireString(readRecord(variantNodes[0])?.['id'], 'productCreate.product.variants.nodes[0].id');

  scenarios['sellingPlanGroupCreate'] = await runScenario('sellingPlanGroupCreate', createGroupMutation, {
    input: sellingPlanGroupInput(capture.stamp),
    resources: { productIds: [productId] },
    productId,
    variantId,
  });
  const group = readRecord(
    mutationRoot(scenarios['sellingPlanGroupCreate'], 'sellingPlanGroupCreate')['sellingPlanGroup'],
  );
  if (!group) {
    throw new Error(
      `sellingPlanGroupCreate returned no group: ${JSON.stringify(scenarios['sellingPlanGroupCreate'], null, 2)}`,
    );
  }
  groupId = requireString(group['id'], 'sellingPlanGroupCreate.sellingPlanGroup.id');
  const sellingPlans = readArray(readRecord(group['sellingPlans'])?.['nodes']);
  planId = requireString(
    readRecord(sellingPlans[0])?.['id'],
    'sellingPlanGroupCreate.sellingPlanGroup.sellingPlans[0].id',
  );

  scenarios['coldGroupRead'] = await runScenario('coldGroupRead', readGroupQuery, {
    id: groupId,
    productId,
    variantId,
  });
  const readGroup = readRecord(dataRecord(scenarios['coldGroupRead'])['sellingPlanGroup']);
  if (!readGroup) {
    throw new Error(`coldGroupRead returned null: ${JSON.stringify(scenarios['coldGroupRead'], null, 2)}`);
  }

  scenarios['hydrateBeforeLocalReplay'] = await runScenario(
    'hydrateBeforeLocalReplay',
    sellingPlanGroupHydrateNodesQuery,
    {
      ids: [groupId],
    },
  );

  scenarios['updateExistingGroup'] = await runScenario('updateExistingGroup', updateGroupMutation, {
    id: groupId,
    productId,
    variantId,
    input: {
      name: `LiveHybrid existing updated ${capture.stamp}`,
      merchantCode: `live-hybrid-existing-updated-${capture.stamp}`,
      description: 'Updated by live Shopify capture before local replay',
      options: ['Delivery cadence'],
      position: 2,
      sellingPlansToUpdate: [
        {
          id: planId,
          name: 'Monthly delivery updated',
          options: ['Monthly'],
          position: 1,
          pricingPolicies: [
            {
              fixed: {
                adjustmentType: 'PERCENTAGE',
                adjustmentValue: { percentage: 10 },
              },
            },
          ],
        },
      ],
    },
  });
  mutationRoot(scenarios['updateExistingGroup'], 'sellingPlanGroupUpdate');

  scenarios['downstreamAfterUpdate'] = await runScenario('downstreamAfterUpdate', downstreamReadQuery, {
    productId,
    variantId,
  });
} finally {
  if (groupId) {
    cleanup['sellingPlanGroupDelete'] = await runScenario('cleanup sellingPlanGroupDelete', deleteGroupMutation, {
      id: groupId,
    });
  }
  if (productId) {
    cleanup['productDelete'] = await runScenario('cleanup productDelete', productDeleteMutation, {
      input: { id: productId },
    });
  }
}

const outputPath = capture.fixturePath('selling-plans', 'selling-plan-group-live-hybrid-existing-resource.json');
await capture.writeJson(outputPath, {
  metadata: {
    scenario: 'Cold LiveHybrid selling-plan group read and existing-resource mutation replay',
    storeDomain: capture.storeDomain,
    apiVersion: capture.apiVersion,
    capturedAt: new Date().toISOString(),
    createdResources: {
      productId,
      variantId,
      groupId,
      planId,
    },
    notes: [
      'The proxy starts cold in parity replay; the group/product/variant context comes only from the recorded sellingPlanGroupHydrateNodes upstreamCalls entry.',
      'The update target uses isolatedProxy so a pre-fix proxy would forward the caller mutation upstream instead of hydrating and staging locally.',
    ],
  },
  scenarios,
  cleanup,
  upstreamCalls: [
    {
      operationName: 'sellingPlanGroupHydrateNodes',
      variables: { ids: [groupId] },
      query: sellingPlanGroupHydrateNodesQuery,
      response: {
        status: scenarios['hydrateBeforeLocalReplay'].status,
        body: scenarios['hydrateBeforeLocalReplay'].response,
      },
    },
  ],
});

console.log(JSON.stringify({ ok: true, outputPath, productId, variantId, groupId, planId }, null, 2));
