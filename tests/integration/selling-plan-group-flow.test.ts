import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const sellingPlanGroupSelection = `
  id
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
  productVariants(first: 5) { nodes { id title product { id } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
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

describe('selling plan group flow', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('stages group lifecycle and product/variant membership locally', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('selling plan group staging must not fetch upstream'));
    const app = createApp(config).callback();

    const productCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id title variants(first: 1) { nodes { id title } } }
            userErrors { field message }
          }
        }`,
        variables: {
          product: {
            title: 'Subscription coffee',
          },
        },
      });
    expect(productCreateResponse.status).toBe(200);
    const product = productCreateResponse.body.data.productCreate.product;
    const productId = product.id as string;
    const variantId = product.variants.nodes[0].id as string;

    const createMutation = `mutation CreateSellingPlanGroup(
      $input: SellingPlanGroupInput!
      $resources: SellingPlanGroupResourceInput
      $productId: ID!
      $variantId: ID!
    ) {
      sellingPlanGroupCreate(input: $input, resources: $resources) {
        sellingPlanGroup { ${sellingPlanGroupSelection} }
        userErrors { field message code }
      }
    }`;
    const createVariables = {
      productId,
      variantId,
      resources: {
        productIds: [productId],
      },
      input: {
        name: 'Subscribe and save',
        merchantCode: 'subscribe-save',
        description: 'Subscription group',
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
      },
    };

    const createResponse = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: createMutation,
      variables: createVariables,
    });
    expect(createResponse.status).toBe(200);
    const createdPayload = createResponse.body.data.sellingPlanGroupCreate;
    expect(createdPayload.userErrors).toEqual([]);
    const group = createdPayload.sellingPlanGroup;
    expect(group).toMatchObject({
      id: expect.stringMatching(/^gid:\/\/shopify\/SellingPlanGroup\/[0-9]+\?shopify-draft-proxy=synthetic$/u),
      name: 'Subscribe and save',
      merchantCode: 'subscribe-save',
      productsCount: { count: 1, precision: 'EXACT' },
      productVariantsCount: { count: 0, precision: 'EXACT' },
      appliesToProduct: true,
      appliesToProductVariant: false,
      appliesToProductVariants: false,
      sellingPlans: {
        nodes: [
          expect.objectContaining({
            name: 'Monthly delivery',
            billingPolicy: expect.objectContaining({ interval: 'MONTH', intervalCount: 1 }),
            pricingPolicies: [
              expect.objectContaining({
                adjustmentType: 'PERCENTAGE',
                adjustmentValue: { __typename: 'SellingPlanPricingPolicyPercentageValue', percentage: 10 },
              }),
            ],
          }),
        ],
      },
    });
    const groupId = group.id as string;
    const planId = group.sellingPlans.nodes[0].id as string;

    const downstreamQuery = `query DownstreamSellingPlanRead($productId: ID!, $variantId: ID!) {
      product(id: $productId) {
        id
        requiresSellingPlan
        sellingPlanGroupsCount { count precision }
        sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
      }
      productVariant(id: $variantId) {
        id
        sellingPlanGroupsCount { count precision }
        sellingPlanGroups(first: 5) { nodes { id name merchantCode } }
      }
    }`;
    const downstreamAfterCreate = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: downstreamQuery,
      variables: { productId, variantId },
    });
    expect(downstreamAfterCreate.body.data.product).toMatchObject({
      id: productId,
      requiresSellingPlan: false,
      sellingPlanGroupsCount: { count: 1, precision: 'EXACT' },
      sellingPlanGroups: { nodes: [{ id: groupId, name: 'Subscribe and save', merchantCode: 'subscribe-save' }] },
    });
    expect(downstreamAfterCreate.body.data.productVariant).toMatchObject({
      id: variantId,
      sellingPlanGroupsCount: { count: 0, precision: 'EXACT' },
      sellingPlanGroups: { nodes: [{ id: groupId, name: 'Subscribe and save', merchantCode: 'subscribe-save' }] },
    });

    const removeProductResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation RemoveProducts($id: ID!, $productIds: [ID!]!) {
          sellingPlanGroupRemoveProducts(id: $id, productIds: $productIds) {
            removedProductIds
            userErrors { field message code }
          }
        }`,
        variables: { id: groupId, productIds: [productId] },
      });
    expect(removeProductResponse.body.data.sellingPlanGroupRemoveProducts).toEqual({
      removedProductIds: [productId],
      userErrors: [],
    });

    const addProductResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation AddProductMembership($id: ID!, $productIds: [ID!]!, $productId: ID!, $variantId: ID!) {
          sellingPlanGroupAddProducts(id: $id, productIds: $productIds) {
            sellingPlanGroup { ${sellingPlanGroupSelection} }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: groupId,
          productIds: [productId],
          productId,
          variantId,
        },
      });
    expect(addProductResponse.body.data.sellingPlanGroupAddProducts.userErrors).toEqual([]);

    const addVariantResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation AddVariantMembership($id: ID!, $productVariantIds: [ID!]!, $productId: ID!, $variantId: ID!) {
          sellingPlanGroupAddProductVariants(id: $id, productVariantIds: $productVariantIds) {
            sellingPlanGroup { ${sellingPlanGroupSelection} }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: groupId,
          productIds: [productId],
          productVariantIds: [variantId],
          productId,
          variantId,
        },
      });
    expect(addVariantResponse.body.data.sellingPlanGroupAddProductVariants.sellingPlanGroup).toMatchObject({
      productsCount: { count: 1, precision: 'EXACT' },
      productVariantsCount: { count: 1, precision: 'EXACT' },
      appliesToProduct: true,
      appliesToProductVariant: true,
      appliesToProductVariants: true,
    });

    const downstreamAfterVariant = await request(app).post('/admin/api/2026-04/graphql.json').send({
      query: downstreamQuery,
      variables: { productId, variantId },
    });
    expect(downstreamAfterVariant.body.data.productVariant).toMatchObject({
      sellingPlanGroupsCount: { count: 1, precision: 'EXACT' },
      sellingPlanGroups: { nodes: [{ id: groupId, name: 'Subscribe and save', merchantCode: 'subscribe-save' }] },
    });

    const updateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UpdateSellingPlanGroup($id: ID!, $input: SellingPlanGroupInput!, $productId: ID!, $variantId: ID!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            deletedSellingPlanIds
            sellingPlanGroup { ${sellingPlanGroupSelection} }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: groupId,
          productId,
          variantId,
          input: {
            name: 'Subscribe and save updated',
            merchantCode: 'subscribe-save-updated',
            options: ['Delivery cadence'],
            sellingPlansToUpdate: [{ id: planId, name: 'Monthly delivery updated', options: ['Every month'] }],
          },
        },
      });
    expect(updateResponse.body.data.sellingPlanGroupUpdate).toMatchObject({
      deletedSellingPlanIds: [],
      userErrors: [],
      sellingPlanGroup: {
        id: groupId,
        name: 'Subscribe and save updated',
        merchantCode: 'subscribe-save-updated',
        options: ['Delivery cadence'],
        sellingPlans: {
          nodes: [
            expect.objectContaining({
              id: planId,
              name: 'Monthly delivery updated',
              options: ['Every month'],
              pricingPolicies: [],
            }),
          ],
        },
      },
    });

    const unknownUpdateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UnknownSellingPlanGroupUpdate($id: ID!, $input: SellingPlanGroupInput!) {
          sellingPlanGroupUpdate(id: $id, input: $input) {
            deletedSellingPlanIds
            sellingPlanGroup { id }
            userErrors { field message code }
          }
        }`,
        variables: {
          id: 'gid://shopify/SellingPlanGroup/999999999999',
          input: { name: 'Nope' },
        },
      });
    const unknownRemoveResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation UnknownSellingPlanGroupRemove($id: ID!, $productIds: [ID!]!) {
          sellingPlanGroupRemoveProducts(id: $id, productIds: $productIds) {
            removedProductIds
            userErrors { field message code }
          }
        }`,
        variables: {
          id: 'gid://shopify/SellingPlanGroup/999999999999',
          input: { name: 'Nope' },
          productIds: [productId],
        },
      });
    expect(unknownUpdateResponse.body.data.sellingPlanGroupUpdate).toEqual({
      deletedSellingPlanIds: null,
      sellingPlanGroup: null,
      userErrors: [{ field: ['id'], message: 'Selling plan group does not exist.', code: 'GROUP_DOES_NOT_EXIST' }],
    });
    expect(unknownRemoveResponse.body.data.sellingPlanGroupRemoveProducts).toEqual({
      removedProductIds: null,
      userErrors: [{ field: ['id'], message: 'Selling plan group does not exist.', code: 'GROUP_DOES_NOT_EXIST' }],
    });

    const deleteResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation DeleteSellingPlanGroup($id: ID!) {
          sellingPlanGroupDelete(id: $id) {
            deletedSellingPlanGroupId
            userErrors { field message code }
          }
        }`,
        variables: { id: groupId },
      });
    expect(deleteResponse.body.data.sellingPlanGroupDelete).toEqual({
      deletedSellingPlanGroupId: groupId,
      userErrors: [],
    });

    const readAfterDelete = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query ReadAfterDelete($id: ID!, $productId: ID!, $variantId: ID!) {
          sellingPlanGroup(id: $id) { id }
          sellingPlanGroups(first: 5) { nodes { id } }
          product(id: $productId) { sellingPlanGroups(first: 5) { nodes { id } } }
          productVariant(id: $variantId) { sellingPlanGroups(first: 5) { nodes { id } } }
        }`,
        variables: { id: groupId, productId, variantId },
      });
    expect(readAfterDelete.body.data).toEqual({
      sellingPlanGroup: null,
      sellingPlanGroups: { nodes: [] },
      product: { sellingPlanGroups: { nodes: [] } },
      productVariant: { sellingPlanGroups: { nodes: [] } },
    });

    const logResponse = await request(app).get('/__meta/log');
    const logRoots = logResponse.body.entries.map(
      (entry: { interpreted: { primaryRootField: string } }) => entry.interpreted.primaryRootField,
    );
    expect(logRoots).toEqual([
      'productCreate',
      'sellingPlanGroupCreate',
      'sellingPlanGroupRemoveProducts',
      'sellingPlanGroupAddProducts',
      'sellingPlanGroupAddProductVariants',
      'sellingPlanGroupUpdate',
      'sellingPlanGroupUpdate',
      'sellingPlanGroupRemoveProducts',
      'sellingPlanGroupDelete',
    ]);
    expect(logResponse.body.entries[1].requestBody.variables).toMatchObject(createVariables);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('stages product and variant selling-plan join/leave roots locally', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValue(new Error('selling plan membership roots must not fetch upstream'));
    const app = createApp(config).callback();

    const productCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateProduct($product: ProductCreateInput!) {
          productCreate(product: $product) {
            product { id title variants(first: 1) { nodes { id title } } }
            userErrors { field message }
          }
        }`,
        variables: {
          product: {
            title: 'Membership coffee',
          },
        },
      });
    expect(productCreateResponse.status).toBe(200);
    const productId = productCreateResponse.body.data.productCreate.product.id as string;
    const variantId = productCreateResponse.body.data.productCreate.product.variants.nodes[0].id as string;

    const groupCreateResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `mutation CreateSellingPlanGroup($input: SellingPlanGroupInput!) {
          sellingPlanGroupCreate(input: $input) {
            sellingPlanGroup { id name merchantCode productsCount { count precision } productVariantsCount { count precision } }
            userErrors { field message code }
          }
        }`,
        variables: {
          input: {
            name: 'Weekly subscription',
            merchantCode: 'weekly-subscription',
            options: ['Frequency'],
            sellingPlansToCreate: [
              {
                name: 'Weekly',
                options: ['Weekly'],
                billingPolicy: { recurring: { interval: 'WEEK', intervalCount: 1 } },
                deliveryPolicy: { recurring: { interval: 'WEEK', intervalCount: 1 } },
                pricingPolicies: [],
              },
            ],
          },
        },
      });
    expect(groupCreateResponse.status).toBe(200);
    const groupId = groupCreateResponse.body.data.sellingPlanGroupCreate.sellingPlanGroup.id as string;

    const joinProductResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation JoinProduct($id: ID!, $sellingPlanGroupIds: [ID!]!) { productJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) { product { id sellingPlanGroups(first: 5) { nodes { id name merchantCode } } sellingPlanGroupsCount { count precision } } userErrors { field message code } } }',
        variables: {
          id: productId,
          sellingPlanGroupIds: [groupId],
        },
      });
    expect(joinProductResponse.status).toBe(200);
    expect(joinProductResponse.body.data.productJoinSellingPlanGroups).toEqual({
      product: {
        id: productId,
        sellingPlanGroups: {
          nodes: [{ id: groupId, name: 'Weekly subscription', merchantCode: 'weekly-subscription' }],
        },
        sellingPlanGroupsCount: { count: 1, precision: 'EXACT' },
      },
      userErrors: [],
    });

    const joinVariantResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation JoinVariant($id: ID!, $sellingPlanGroupIds: [ID!]!) { productVariantJoinSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) { productVariant { id sellingPlanGroups(first: 5) { nodes { id name merchantCode } } sellingPlanGroupsCount { count precision } } userErrors { field message code } } }',
        variables: {
          id: variantId,
          sellingPlanGroupIds: [groupId],
        },
      });
    expect(joinVariantResponse.status).toBe(200);
    expect(joinVariantResponse.body.data.productVariantJoinSellingPlanGroups).toEqual({
      productVariant: {
        id: variantId,
        sellingPlanGroups: {
          nodes: [{ id: groupId, name: 'Weekly subscription', merchantCode: 'weekly-subscription' }],
        },
        sellingPlanGroupsCount: { count: 1, precision: 'EXACT' },
      },
      userErrors: [],
    });

    const leaveProductResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation LeaveProduct($id: ID!, $sellingPlanGroupIds: [ID!]!) { productLeaveSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) { product { id sellingPlanGroups(first: 5) { nodes { id } } sellingPlanGroupsCount { count precision } } userErrors { field message code } } }',
        variables: {
          id: productId,
          sellingPlanGroupIds: [groupId],
        },
      });
    expect(leaveProductResponse.status).toBe(200);
    expect(leaveProductResponse.body.data.productLeaveSellingPlanGroups).toEqual({
      product: {
        id: productId,
        sellingPlanGroups: { nodes: [{ id: groupId }] },
        sellingPlanGroupsCount: { count: 0, precision: 'EXACT' },
      },
      userErrors: [],
    });

    const leaveVariantResponse = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query:
          'mutation LeaveVariant($id: ID!, $sellingPlanGroupIds: [ID!]!) { productVariantLeaveSellingPlanGroups(id: $id, sellingPlanGroupIds: $sellingPlanGroupIds) { productVariant { id sellingPlanGroups(first: 5) { nodes { id } } sellingPlanGroupsCount { count precision } } userErrors { field message code } } }',
        variables: {
          id: variantId,
          sellingPlanGroupIds: [groupId],
        },
      });
    expect(leaveVariantResponse.status).toBe(200);
    expect(leaveVariantResponse.body.data.productVariantLeaveSellingPlanGroups).toEqual({
      productVariant: {
        id: variantId,
        sellingPlanGroups: { nodes: [] },
        sellingPlanGroupsCount: { count: 0, precision: 'EXACT' },
      },
      userErrors: [],
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
