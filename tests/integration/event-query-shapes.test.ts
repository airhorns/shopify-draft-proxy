import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const emptyConnection = {
  nodes: [],
  edges: [],
  pageInfo: {
    hasNextPage: false,
    hasPreviousPage: false,
    startCursor: null,
    endCursor: null,
  },
};

describe('event query shapes', () => {
  beforeEach(() => {
    store.reset();
    vi.restoreAllMocks();
  });

  it('returns Shopify-like empty event catalogs, exact zero counts, and null detail reads in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('event snapshot reads stay local'));
    const app = createApp(config).callback();

    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query EventsEmpty($eventId: ID!, $query: String!, $first: Int!) {
          event(id: $eventId) {
            id
            message
          }
          events(first: $first, query: $query, reverse: true) {
            nodes {
              id
              message
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
          eventsCount(query: $query) {
            count
            precision
          }
        }`,
        variables: {
          eventId: 'gid://shopify/BasicEvent/999999999999',
          first: 2,
          query: 'subject_id:0',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        event: null,
        events: emptyConnection,
        eventsCount: {
          count: 0,
          precision: 'EXACT',
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
