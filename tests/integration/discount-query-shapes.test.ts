import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';
import type { DiscountRecord } from '../../src/state/types.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

function buildDiscount(overrides: Partial<DiscountRecord> = {}): DiscountRecord {
  return {
    id: 'gid://shopify/DiscountCodeNode/1688770478313',
    typeName: 'DiscountCodeBasic',
    method: 'code',
    title: 'HAR-191 catalog fixture HAR191CODE1777117036468',
    status: 'ACTIVE',
    summary: '10% off entire order',
    startsAt: '2024-04-25T00:00:00Z',
    endsAt: null,
    createdAt: '2026-04-25T11:37:16Z',
    updatedAt: '2026-04-25T11:37:16Z',
    asyncUsageCount: 0,
    discountClasses: ['ORDER'],
    combinesWith: {
      productDiscounts: true,
      orderDiscounts: false,
      shippingDiscounts: false,
    },
    codes: ['HAR191CODE1777117036468'],
    ...overrides,
  };
}

function buildAutomaticDiscount(overrides: Partial<DiscountRecord> = {}): DiscountRecord {
  return buildDiscount({
    id: 'gid://shopify/DiscountAutomaticNode/1688770479000',
    typeName: 'DiscountAutomaticBasic',
    method: 'automatic',
    title: 'HAR-192 automatic detail fixture',
    summary: '15% off entire order',
    discountClasses: ['ORDER'],
    codes: [],
    ...overrides,
  });
}

describe('discount query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns Shopify-like empty discount catalog and count in snapshot mode without upstream access', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('discount snapshot read should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query DiscountEmpty($query: String!) {
          discountNodes(first: 2, query: $query, sortKey: TITLE) {
            nodes {
              id
            }
            edges {
              cursor
              node {
                id
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          discountNodesCount(query: $query) {
            count
            precision
          }
        }`,
        variables: {
          query: 'title:__har191_discount_empty_probe__',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        discountNodes: {
          nodes: [],
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        discountNodesCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('serializes staged/base discounts with nodes, edges, pageInfo, count, and status search filtering', async () => {
    store.upsertBaseDiscounts([buildDiscount()]);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('discount snapshot read should not hit upstream fetch');
    });

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query DiscountCatalog($query: String!) {
          discountNodes(first: 2, query: $query, sortKey: TITLE) {
            nodes {
              id
              discount {
                __typename
                ... on DiscountCodeBasic {
                  title
                  status
                  summary
                  startsAt
                  endsAt
                  createdAt
                  updatedAt
                  asyncUsageCount
                  discountClasses
                  combinesWith {
                    productDiscounts
                    orderDiscounts
                    shippingDiscounts
                  }
                  codes(first: 1) {
                    nodes {
                      code
                    }
                  }
                }
              }
            }
            edges {
              cursor
              node {
                id
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          discountNodesCount(query: $query) {
            count
            precision
          }
        }`,
        variables: {
          query: 'status:active',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.discountNodes.nodes).toEqual([
      {
        id: 'gid://shopify/DiscountCodeNode/1688770478313',
        discount: {
          __typename: 'DiscountCodeBasic',
          title: 'HAR-191 catalog fixture HAR191CODE1777117036468',
          status: 'ACTIVE',
          summary: '10% off entire order',
          startsAt: '2024-04-25T00:00:00Z',
          endsAt: null,
          createdAt: '2026-04-25T11:37:16Z',
          updatedAt: '2026-04-25T11:37:16Z',
          asyncUsageCount: 0,
          discountClasses: ['ORDER'],
          combinesWith: {
            productDiscounts: true,
            orderDiscounts: false,
            shippingDiscounts: false,
          },
          codes: {
            nodes: [
              {
                code: 'HAR191CODE1777117036468',
              },
            ],
          },
        },
      },
    ]);
    expect(response.body.data.discountNodes.edges).toEqual([
      {
        cursor: 'cursor:gid://shopify/DiscountCodeNode/1688770478313',
        node: {
          id: 'gid://shopify/DiscountCodeNode/1688770478313',
        },
      },
    ]);
    expect(response.body.data.discountNodes.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: 'cursor:gid://shopify/DiscountCodeNode/1688770478313',
      endCursor: 'cursor:gid://shopify/DiscountCodeNode/1688770478313',
    });
    expect(response.body.data.discountNodesCount).toEqual({
      count: 1,
      precision: 'EXACT',
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('serializes singular code discount detail roots by id and redeem code with nested selected fields', async () => {
    store.upsertBaseDiscounts([
      buildDiscount({
        id: 'gid://shopify/DiscountCodeNode/192001',
        title: 'HAR-192 detail code fixture',
        summary: '10% off entire order - Minimum purchase of $1.00',
        redeemCodes: [
          {
            id: 'gid://shopify/DiscountRedeemCode/99001',
            code: 'HAR192DETAIL',
            asyncUsageCount: 0,
          },
        ],
        context: {
          typeName: 'DiscountBuyerSelectionAll',
          all: 'ALL',
        },
        customerGets: {
          value: {
            typeName: 'DiscountPercentage',
            percentage: 0.1,
          },
          items: {
            typeName: 'AllDiscountItems',
            allItems: true,
          },
          appliesOnOneTimePurchase: true,
          appliesOnSubscription: false,
        },
        minimumRequirement: {
          typeName: 'DiscountMinimumSubtotal',
          greaterThanOrEqualToSubtotal: {
            amount: '1.0',
            currencyCode: 'CAD',
          },
        },
      }),
    ]);

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query($id: ID!, $code: String!) {
          discountNode(id: $id) {
            id
            discount {
              __typename
              ... on DiscountCodeBasic {
                title
                status
                summary
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
                customerGets {
                  value {
                    __typename
                    ... on DiscountPercentage {
                      percentage
                    }
                  }
                  items {
                    __typename
                    ... on AllDiscountItems {
                      allItems
                    }
                  }
                  appliesOnOneTimePurchase
                  appliesOnSubscription
                }
                minimumRequirement {
                  __typename
                  ... on DiscountMinimumSubtotal {
                    greaterThanOrEqualToSubtotal {
                      amount
                      currencyCode
                    }
                  }
                }
              }
            }
            metafield(namespace: "custom", key: "missing") {
              id
            }
            metafields(first: 2) {
              nodes {
                id
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
            events(first: 2) {
              edges {
                cursor
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
          codeDiscountNode(id: $id) {
            id
            codeDiscount {
              __typename
              ... on DiscountCodeBasic {
                title
              }
            }
          }
          codeDiscountNodeByCode(code: $code) {
            id
            codeDiscount {
              __typename
              ... on DiscountCodeBasic {
                title
              }
            }
          }
        }`,
        variables: {
          id: 'gid://shopify/DiscountCodeNode/192001',
          code: 'HAR192DETAIL',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.discountNode).toEqual({
      id: 'gid://shopify/DiscountCodeNode/192001',
      discount: {
        __typename: 'DiscountCodeBasic',
        title: 'HAR-192 detail code fixture',
        status: 'ACTIVE',
        summary: '10% off entire order - Minimum purchase of $1.00',
        codes: {
          nodes: [
            {
              id: 'gid://shopify/DiscountRedeemCode/99001',
              code: 'HAR192DETAIL',
              asyncUsageCount: 0,
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'cursor:HAR192DETAIL',
            endCursor: 'cursor:HAR192DETAIL',
          },
        },
        context: {
          __typename: 'DiscountBuyerSelectionAll',
          all: 'ALL',
        },
        customerGets: {
          value: {
            __typename: 'DiscountPercentage',
            percentage: 0.1,
          },
          items: {
            __typename: 'AllDiscountItems',
            allItems: true,
          },
          appliesOnOneTimePurchase: true,
          appliesOnSubscription: false,
        },
        minimumRequirement: {
          __typename: 'DiscountMinimumSubtotal',
          greaterThanOrEqualToSubtotal: {
            amount: '1.0',
            currencyCode: 'CAD',
          },
        },
      },
      metafield: null,
      metafields: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      events: {
        edges: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });
    expect(response.body.data.codeDiscountNode).toEqual({
      id: 'gid://shopify/DiscountCodeNode/192001',
      codeDiscount: {
        __typename: 'DiscountCodeBasic',
        title: 'HAR-192 detail code fixture',
      },
    });
    expect(response.body.data.codeDiscountNodeByCode).toEqual(response.body.data.codeDiscountNode);
  });

  it('serializes singular automatic discount detail and returns null for mismatched or unknown roots', async () => {
    store.upsertBaseDiscounts([
      buildAutomaticDiscount({
        id: 'gid://shopify/DiscountAutomaticNode/192002',
        customerGets: {
          value: {
            typeName: 'DiscountPercentage',
            percentage: 0.15,
          },
          items: {
            typeName: 'AllDiscountItems',
            allItems: true,
          },
          appliesOnOneTimePurchase: true,
          appliesOnSubscription: false,
        },
        minimumRequirement: {
          typeName: 'DiscountMinimumQuantity',
          greaterThanOrEqualToQuantity: '2',
        },
      }),
    ]);

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query($id: ID!, $missing: ID!) {
          automaticDiscountNode(id: $id) {
            id
            automaticDiscount {
              __typename
              ... on DiscountAutomaticBasic {
                title
                customerGets {
                  value {
                    __typename
                    ... on DiscountPercentage {
                      percentage
                    }
                  }
                }
                minimumRequirement {
                  __typename
                  ... on DiscountMinimumQuantity {
                    greaterThanOrEqualToQuantity
                  }
                }
              }
            }
          }
          discountNode(id: $missing) {
            id
          }
        }`,
        variables: {
          id: 'gid://shopify/DiscountAutomaticNode/192002',
          missing: 'gid://shopify/DiscountAutomaticNode/404',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      automaticDiscountNode: {
        id: 'gid://shopify/DiscountAutomaticNode/192002',
        automaticDiscount: {
          __typename: 'DiscountAutomaticBasic',
          title: 'HAR-192 automatic detail fixture',
          customerGets: {
            value: {
              __typename: 'DiscountPercentage',
              percentage: 0.15,
            },
          },
          minimumRequirement: {
            __typename: 'DiscountMinimumQuantity',
            greaterThanOrEqualToQuantity: '2',
          },
        },
      },
      discountNode: null,
    });
  });

  it('preserves captured app-discount detail fields without inventing missing app data', async () => {
    store.upsertBaseDiscounts([
      buildDiscount({
        id: 'gid://shopify/DiscountCodeNode/192003',
        typeName: 'DiscountCodeApp',
        method: 'code',
        title: 'App discount boundary',
        appId: 'gid://shopify/App/1',
        discountId: 'gid://shopify/DiscountCodeNode/192003',
        appDiscountType: {
          appKey: 'app-client-id',
          functionId: '11111111-1111-4111-8111-111111111111',
          title: 'Volume discount',
          description: 'App-managed volume discount',
        },
      }),
      buildAutomaticDiscount({
        id: 'gid://shopify/DiscountAutomaticNode/192004',
        typeName: 'DiscountAutomaticApp',
        method: 'automatic',
        title: 'Automatic app discount boundary',
        status: 'SCHEDULED',
        combinesWith: {
          productDiscounts: false,
          orderDiscounts: true,
          shippingDiscounts: true,
        },
        appDiscountType: {
          appKey: 'automatic-app-client-id',
          functionId: '22222222-2222-4222-8222-222222222222',
          title: 'Automatic volume discount',
        },
      }),
    ]);

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query($codeId: ID!, $automaticId: ID!) {
          codeDiscountNode(id: $codeId) {
            codeDiscount {
              __typename
              ... on DiscountCodeApp {
                title
                status
                discountId
                combinesWith {
                  productDiscounts
                  orderDiscounts
                  shippingDiscounts
                }
                appDiscountType {
                  appKey
                  functionId
                  title
                  description
                }
                errorHistory {
                  firstOccurredAt
                }
              }
            }
          }
          automaticDiscountNode(id: $automaticId) {
            automaticDiscount {
              __typename
              ... on DiscountAutomaticApp {
                title
                status
                combinesWith {
                  productDiscounts
                  orderDiscounts
                  shippingDiscounts
                }
                appDiscountType {
                  appKey
                  functionId
                  title
                }
              }
            }
          }
        }`,
        variables: {
          codeId: 'gid://shopify/DiscountCodeNode/192003',
          automaticId: 'gid://shopify/DiscountAutomaticNode/192004',
        },
      });

    expect(response.body.data.codeDiscountNode.codeDiscount).toEqual({
      __typename: 'DiscountCodeApp',
      title: 'App discount boundary',
      status: 'ACTIVE',
      discountId: 'gid://shopify/DiscountCodeNode/192003',
      combinesWith: {
        productDiscounts: true,
        orderDiscounts: false,
        shippingDiscounts: false,
      },
      appDiscountType: {
        appKey: 'app-client-id',
        functionId: '11111111-1111-4111-8111-111111111111',
        title: 'Volume discount',
        description: 'App-managed volume discount',
      },
      errorHistory: null,
    });
    expect(response.body.data.automaticDiscountNode.automaticDiscount).toEqual({
      __typename: 'DiscountAutomaticApp',
      title: 'Automatic app discount boundary',
      status: 'SCHEDULED',
      combinesWith: {
        productDiscounts: false,
        orderDiscounts: true,
        shippingDiscounts: true,
      },
      appDiscountType: {
        appKey: 'automatic-app-client-id',
        functionId: '22222222-2222-4222-8222-222222222222',
        title: 'Automatic volume discount',
      },
    });
    expect(response.body.errors).toBeUndefined();
  });

  it('paginates and reverses discount catalog reads over the effective local discount graph', async () => {
    store.upsertBaseDiscounts([
      buildDiscount({
        id: 'gid://shopify/DiscountCodeNode/1001',
        title: 'Alpha discount',
        codes: ['ALPHA'],
        createdAt: '2026-04-25T11:00:00Z',
        updatedAt: '2026-04-25T11:00:00Z',
      }),
      buildDiscount({
        id: 'gid://shopify/DiscountCodeNode/1002',
        title: 'Bravo discount',
        codes: ['BRAVO'],
        createdAt: '2026-04-25T12:00:00Z',
        updatedAt: '2026-04-25T12:00:00Z',
      }),
    ]);

    const app = createApp(config).callback();
    const firstPage = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query {
          discountNodes(first: 1, sortKey: TITLE, reverse: true) {
            edges {
              cursor
              node {
                id
                discount {
                  ... on DiscountCodeBasic {
                    title
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
        }`,
      });

    expect(firstPage.body.data.discountNodes).toEqual({
      edges: [
        {
          cursor: 'cursor:gid://shopify/DiscountCodeNode/1002',
          node: {
            id: 'gid://shopify/DiscountCodeNode/1002',
            discount: {
              title: 'Bravo discount',
            },
          },
        },
      ],
      pageInfo: {
        hasNextPage: true,
        hasPreviousPage: false,
        startCursor: 'cursor:gid://shopify/DiscountCodeNode/1002',
        endCursor: 'cursor:gid://shopify/DiscountCodeNode/1002',
      },
    });

    const secondPage = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query($after: String!) {
          discountNodes(first: 1, after: $after, sortKey: TITLE, reverse: true) {
            edges {
              cursor
              node {
                id
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
        }`,
        variables: {
          after: firstPage.body.data.discountNodes.pageInfo.endCursor,
        },
      });

    expect(secondPage.body.data.discountNodes).toEqual({
      edges: [
        {
          cursor: 'cursor:gid://shopify/DiscountCodeNode/1001',
          node: {
            id: 'gid://shopify/DiscountCodeNode/1001',
          },
        },
      ],
      pageInfo: {
        hasNextPage: false,
        hasPreviousPage: true,
        startCursor: 'cursor:gid://shopify/DiscountCodeNode/1001',
        endCursor: 'cursor:gid://shopify/DiscountCodeNode/1001',
      },
    });
  });

  it('keeps the captured discountNodes code filter empty for native code discounts', async () => {
    store.upsertBaseDiscounts([buildDiscount()]);

    const app = createApp(config).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query($query: String!) {
          discountNodes(first: 2, query: $query) {
            edges {
              node {
                id
              }
            }
          }
          discountNodesCount(query: $query) {
            count
            precision
          }
        }`,
        variables: {
          query: 'code:HAR191CODE1777117036468',
        },
      });

    expect(response.body).toEqual({
      data: {
        discountNodes: {
          edges: [],
        },
        discountNodesCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
    });
  });
});
