import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const liveHybridConfig: AppConfig = {
  ...snapshotConfig,
  readMode: 'live-hybrid',
};

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

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
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
});