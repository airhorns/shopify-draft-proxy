import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const snapshotConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const passthroughConfig: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'passthrough',
};

describe('admin platform utility query shapes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('serves safe utility read roots in snapshot mode without hitting upstream', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('admin platform utility reads should resolve locally in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query AdminPlatformUtilityReads($ids: [ID!]!, $domainId: ID!, $jobId: ID!) {
          publicApiVersions {
            __typename
            handle
            displayName
            supported
          }
          node(id: "gid://shopify/Product/0") {
            __typename
            id
          }
          nodes(ids: $ids) {
            __typename
            id
          }
          job(id: $jobId) {
            __typename
            id
            done
            query {
              __typename
            }
          }
          domain(id: $domainId) {
            id
            host
            url
            sslEnabled
          }
          backupRegion {
            __typename
            id
            name
            ... on MarketRegionCountry {
              code
            }
          }
          taxonomy {
            categories(first: 2, search: "zzzzzz-no-match-har-315") {
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
          }
        }`,
        variables: {
          ids: ['gid://shopify/Product/0', 'gid://shopify/Job/0', 'gid://shopify/Domain/0'],
          domainId: 'gid://shopify/Domain/0',
          jobId: 'gid://shopify/Job/0',
        },
      });

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      data: {
        publicApiVersions: [
          { __typename: 'ApiVersion', handle: '2025-07', displayName: '2025-07', supported: true },
          { __typename: 'ApiVersion', handle: '2025-10', displayName: '2025-10', supported: true },
          { __typename: 'ApiVersion', handle: '2026-01', displayName: '2026-01', supported: true },
          { __typename: 'ApiVersion', handle: '2026-04', displayName: '2026-04 (Latest)', supported: true },
          {
            __typename: 'ApiVersion',
            handle: '2026-07',
            displayName: '2026-07 (Release candidate)',
            supported: false,
          },
          { __typename: 'ApiVersion', handle: 'unstable', displayName: 'unstable', supported: false },
        ],
        node: null,
        nodes: [null, null, null],
        job: {
          __typename: 'Job',
          id: 'gid://shopify/Job/0',
          done: true,
          query: {
            __typename: 'QueryRoot',
          },
        },
        domain: null,
        backupRegion: {
          __typename: 'MarketRegionCountry',
          id: 'gid://shopify/MarketRegionCountry/4062110417202',
          name: 'Canada',
          code: 'CA',
        },
        taxonomy: {
          categories: {
            nodes: [],
            edges: [],
            pageInfo: {
              hasNextPage: false,
              hasPreviousPage: false,
              startCursor: null,
              endCursor: null,
            },
          },
        },
      },
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('mirrors captured staff utility access blockers locally in snapshot mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      throw new Error('staff utility blockers should resolve locally in snapshot mode');
    });

    const app = createApp(snapshotConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .send({
        query: `query StaffUtilityRead {
          staffMember {
            id
            exists
            active
          }
          staffMembers(first: 1) {
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
        }`,
      });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual({
      staffMember: null,
      staffMembers: null,
    });
    expect(response.body.errors).toEqual([
      expect.objectContaining({
        message: expect.stringContaining('Access denied for staffMember field.'),
        path: ['staffMember'],
        extensions: expect.objectContaining({ code: 'ACCESS_DENIED' }),
      }),
      expect.objectContaining({
        message: 'Access denied for staffMembers field.',
        path: ['staffMembers'],
        extensions: expect.objectContaining({ code: 'ACCESS_DENIED' }),
      }),
    ]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('keeps Flow utility mutations as unsupported side-effect passthroughs', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { flowTriggerReceive: null } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(passthroughConfig).callback();
    const response = await request(app)
      .post('/admin/api/2026-04/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({
        query: `mutation FlowTriggerReceive {
          flowTriggerReceive(handle: "har-315", payload: "{}") {
            userErrors {
              message
            }
          }
        }`,
      });

    expect(response.status).toBe(200);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(store.getLog()).toHaveLength(1);
    expect(store.getLog()[0]).toMatchObject({
      operationName: 'FlowTriggerReceive',
      status: 'proxied',
      interpreted: {
        registeredOperation: {
          name: 'flowTriggerReceive',
          domain: 'admin-platform',
          execution: 'stage-locally',
          implemented: false,
        },
        safety: {
          classification: 'unsupported-flow-side-effect-mutation',
          wouldProxyToShopify: true,
        },
      },
      notes:
        'Unsupported Flow utility mutation would be proxied to Shopify. Flow signature generation and trigger delivery require local signing/trigger semantics plus raw commit replay before support.',
    });
  });
});
