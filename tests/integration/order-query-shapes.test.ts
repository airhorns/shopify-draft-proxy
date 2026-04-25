import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import type { OrderRecord } from '../../src/state/types.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const liveHybridConfig: AppConfig = {
  ...snapshotConfig,
  readMode: 'live-hybrid',
};

function makeOrder(id: string, name: string, createdAt: string, overrides: Partial<OrderRecord> = {}): OrderRecord {
  return {
    id,
    name,
    createdAt,
    updatedAt: createdAt,
    displayFinancialStatus: 'PENDING',
    displayFulfillmentStatus: 'UNFULFILLED',
    note: null,
    tags: [],
    customAttributes: [],
    billingAddress: null,
    shippingAddress: null,
    subtotalPriceSet: {
      shopMoney: {
        amount: '10.0',
        currencyCode: 'CAD',
      },
    },
    currentTotalPriceSet: {
      shopMoney: {
        amount: '10.0',
        currencyCode: 'CAD',
      },
    },
    totalPriceSet: {
      shopMoney: {
        amount: '10.0',
        currencyCode: 'CAD',
      },
    },
    totalRefundedSet: {
      shopMoney: {
        amount: '0.0',
        currencyCode: 'CAD',
      },
    },
    customer: null,
    shippingLines: [],
    lineItems: [],
    transactions: [],
    refunds: [],
    returns: [],
    ...overrides,
  };
}

describe('order query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('returns the captured empty-state baseline for order roots in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { order: { id: 'gid://shopify/Order/1' } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query: `query OrderEmptyState($missingOrderId: ID!, $first: Int!) {
          lookup: order(id: $missingOrderId) {
            id
            name
          }
          catalog: orders(first: $first, sortKey: CREATED_AT, reverse: true) {
            edges {
              cursor
              node {
                id
                name
              }
            }
            pageInfo {
              hasNextPage
              hasPreviousPage
              startCursor
              endCursor
            }
          }
          totals: ordersCount {
            count
            precision
          }
        }`,
        variables: {
          missingOrderId: 'gid://shopify/Order/0',
          first: 1,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        lookup: null,
        catalog: {
          edges: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        totals: {
          count: 0,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('passes order roots through upstream in live-hybrid mode while order overlay storage is still unimplemented', async () => {
    const upstreamPayload = {
      data: {
        order: {
          id: 'gid://shopify/Order/101',
          name: '#101',
        },
        orders: {
          edges: [
            {
              cursor: 'opaque-cursor',
              node: {
                id: 'gid://shopify/Order/101',
                name: '#101',
              },
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: 'opaque-cursor',
            endCursor: 'opaque-cursor',
          },
        },
        ordersCount: {
          count: 1,
          precision: 'EXACT',
        },
      },
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(
      async () =>
        new Response(JSON.stringify(upstreamPayload), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
    );

    const app = createApp(liveHybridConfig).callback();
    const response = await request(app)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test_token')
      .send({
        query: `query OrderLiveHybrid($id: ID!, $first: Int!) {
          order(id: $id) { id name }
          orders(first: $first, sortKey: CREATED_AT, reverse: true) {
            edges { cursor node { id name } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          ordersCount { count precision }
        }`,
        variables: {
          id: 'gid://shopify/Order/101',
          first: 1,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual(upstreamPayload);
    expect(fetchSpy).toHaveBeenCalledOnce();
  });

  it('filters, sorts, paginates, and count-limits local order catalog reads in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('snapshot order catalog reads must not hit upstream');
    });

    store.stageCreateOrder(
      makeOrder('gid://shopify/Order/1001', '#1001', '2026-04-20T00:00:00.000Z', {
        displayFinancialStatus: 'PAID',
        tags: ['vip'],
        email: 'vip-one@example.com',
      }),
    );
    store.stageCreateOrder(
      makeOrder('gid://shopify/Order/1002', '#1002', '2026-04-22T00:00:00.000Z', {
        displayFinancialStatus: 'PENDING',
        displayFulfillmentStatus: 'FULFILLED',
        tags: ['wholesale'],
        email: 'wholesale@example.com',
      }),
    );
    store.stageCreateOrder(
      makeOrder('gid://shopify/Order/1003', '#1003', '2026-04-21T00:00:00.000Z', {
        displayFinancialStatus: 'PAID',
        tags: ['vip', 'priority'],
        email: 'vip-two@example.com',
      }),
    );

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query OrderCatalogSemantics($tagQuery: String!, $statusQuery: String!, $after: String, $limit: Int, $unlimited: Int) {
          newestVip: orders(first: 1, query: $tagQuery, sortKey: CREATED_AT, reverse: true) {
            nodes { id name tags email }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          nextVip: orders(first: 1, after: $after, query: $tagQuery, sortKey: CREATED_AT, reverse: true) {
            nodes { id name tags email }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          oldestVip: orders(first: 1, query: $tagQuery, sortKey: CREATED_AT, reverse: false) {
            nodes { id name }
          }
          byName: orders(first: 3, query: "name:1003", sortKey: CREATED_AT, reverse: true) {
            nodes { id name }
          }
          byStatus: orders(first: 3, query: $statusQuery, sortKey: CREATED_AT, reverse: true) {
            nodes { id name displayFinancialStatus displayFulfillmentStatus }
          }
          savedSearch: orders(first: 3, savedSearchId: "gid://shopify/SavedSearch/1") {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          limitedVipCount: ordersCount(query: $tagQuery, limit: $limit) { count precision }
          exactVipCount: ordersCount(query: $tagQuery, limit: $unlimited) { count precision }
          savedSearchCount: ordersCount(savedSearchId: "gid://shopify/SavedSearch/1") { count precision }
        }`,
        variables: {
          tagQuery: 'tag:vip',
          statusQuery: 'financial_status:paid fulfillment_status:unfulfilled',
          after: 'cursor:gid://shopify/Order/1003',
          limit: 1,
          unlimited: null,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        newestVip: {
          nodes: [
            {
              id: 'gid://shopify/Order/1003',
              name: '#1003',
              tags: ['vip', 'priority'],
              email: 'vip-two@example.com',
            },
          ],
          pageInfo: {
            hasNextPage: true,
            hasPreviousPage: false,
            startCursor: 'cursor:gid://shopify/Order/1003',
            endCursor: 'cursor:gid://shopify/Order/1003',
          },
        },
        nextVip: {
          nodes: [
            {
              id: 'gid://shopify/Order/1001',
              name: '#1001',
              tags: ['vip'],
              email: 'vip-one@example.com',
            },
          ],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: true,
            startCursor: 'cursor:gid://shopify/Order/1001',
            endCursor: 'cursor:gid://shopify/Order/1001',
          },
        },
        oldestVip: {
          nodes: [
            {
              id: 'gid://shopify/Order/1001',
              name: '#1001',
            },
          ],
        },
        byName: {
          nodes: [
            {
              id: 'gid://shopify/Order/1003',
              name: '#1003',
            },
          ],
        },
        byStatus: {
          nodes: [
            {
              id: 'gid://shopify/Order/1003',
              name: '#1003',
              displayFinancialStatus: 'PAID',
              displayFulfillmentStatus: 'UNFULFILLED',
            },
            {
              id: 'gid://shopify/Order/1001',
              name: '#1001',
              displayFinancialStatus: 'PAID',
              displayFulfillmentStatus: 'UNFULFILLED',
            },
          ],
        },
        savedSearch: {
          nodes: [],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: null,
            endCursor: null,
          },
        },
        limitedVipCount: {
          count: 1,
          precision: 'AT_LEAST',
        },
        exactVipCount: {
          count: 2,
          precision: 'EXACT',
        },
        savedSearchCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('returns Shopify-like empty detail shapes for absent merchant order nested data', async () => {
    const order = makeOrder('gid://shopify/Order/112', '#112', '2026-04-24T00:00:00.000Z');
    store.upsertBaseOrders([order]);

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query OrderMerchantEmptyDetail($id: ID!) {
          order(id: $id) {
            id
            currentSubtotalPriceSet { shopMoney { amount currencyCode } }
            totalReceivedSet { shopMoney { amount currencyCode } }
            netPaymentSet { shopMoney { amount currencyCode } }
            totalRefundedShippingSet { shopMoney { amount currencyCode } }
            totalShippingPriceSet { shopMoney { amount currencyCode } }
            shippingLines(first: 2) {
              nodes { title }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            lineItems(first: 2) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            fulfillmentOrders(first: 2) {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            fulfillments(first: 2) {
              id
              status
            }
            returns(first: 2) {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }`,
        variables: {
          id: order.id,
        },
      });

    expect(response.status).toBe(200);
    expect(response.body.data.order).toEqual({
      id: order.id,
      currentSubtotalPriceSet: {
        shopMoney: {
          amount: '10.0',
          currencyCode: 'CAD',
        },
      },
      totalReceivedSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      netPaymentSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      totalRefundedShippingSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      totalShippingPriceSet: {
        shopMoney: {
          amount: '0.0',
          currencyCode: 'CAD',
        },
      },
      shippingLines: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      lineItems: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      fulfillmentOrders: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
      fulfillments: [],
      returns: {
        nodes: [],
        pageInfo: {
          hasNextPage: false,
          hasPreviousPage: false,
          startCursor: null,
          endCursor: null,
        },
      },
    });
  });
});
