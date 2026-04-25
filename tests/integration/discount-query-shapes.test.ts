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
