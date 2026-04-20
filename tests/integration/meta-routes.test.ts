import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
  snapshotPath: 'fixtures/snapshots/dev-store.json',
};

describe('meta routes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('exposes health, config, log, state, and reset endpoints for the in-memory draft state', async () => {
    const app = createApp(config);
    const server = app.callback();

    const createResponse = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .send({
        query:
          'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title status } userErrors { field message } } }',
        variables: {
          product: {
            title: 'Meta Draft',
            status: 'DRAFT',
          },
        },
      });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body.data.productCreate.userErrors).toEqual([]);
    const createdId = createResponse.body.data.productCreate.product.id as string;

    const health = await request(server).get('/__meta/health');
    expect(health.status).toBe(200);
    expect(health.body).toEqual({
      ok: true,
      message: 'shopify-draft-proxy is running',
    });

    const metaConfig = await request(server).get('/__meta/config');
    expect(metaConfig.status).toBe(200);
    expect(metaConfig.body).toEqual({
      port: 3000,
      readMode: 'snapshot',
      shopifyAdminOrigin: 'https://example.myshopify.com',
      snapshotPath: 'fixtures/snapshots/dev-store.json',
    });

    const log = await request(server).get('/__meta/log');
    expect(log.status).toBe(200);
    expect(log.body).toEqual({
      entries: [
        expect.objectContaining({
          path: '/admin/api/2025-01/graphql.json',
          operationName: 'productCreate',
          status: 'staged',
          variables: {
            product: {
              title: 'Meta Draft',
              status: 'DRAFT',
            },
          },
        }),
      ],
    });

    const state = await request(server).get('/__meta/state');
    expect(state.status).toBe(200);
    expect(state.body.stagedState.products[createdId]).toMatchObject({
      id: createdId,
      title: 'Meta Draft',
      status: 'DRAFT',
    });

    const reset = await request(server).post('/__meta/reset');
    expect(reset.status).toBe(200);
    expect(reset.body).toEqual({
      ok: true,
      message: 'state reset',
    });

    const emptiedLog = await request(server).get('/__meta/log');
    expect(emptiedLog.body).toEqual({ entries: [] });
    const emptiedState = await request(server).get('/__meta/state');
    expect(emptiedState.body).toEqual({
      baseState: {
        products: {},
        productVariants: {},
        productOptions: {},
        collections: {},
        publications: {},
        customers: {},
        productCollections: {},
        productMedia: {},
        productMetafields: {},
        deletedProductIds: {},
        deletedCollectionIds: {},
        deletedCustomerIds: {},
      },
      stagedState: {
        products: {},
        productVariants: {},
        productOptions: {},
        collections: {},
        publications: {},
        customers: {},
        productCollections: {},
        productMedia: {},
        productMetafields: {},
        deletedProductIds: {},
        deletedCollectionIds: {},
        deletedCustomerIds: {},
      },
    });
  });

  it('replays staged mutations in original order, stops on the first upstream failure, and persists commit statuses in the log', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementationOnce(async () => {
      return new Response(JSON.stringify({ data: { productCreate: { product: { id: 'gid://shopify/Product/1' }, userErrors: [] } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }).mockImplementationOnce(async () => {
      return new Response(JSON.stringify({ errors: [{ message: 'write scope denied' }] }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    const app = createApp(config);
    const server = app.callback();

    for (const title of ['First Commit Draft', 'Second Commit Draft', 'Third Commit Draft']) {
      const createResponse = await request(server)
        .post('/admin/api/2025-01/graphql.json')
        .send({
          query:
            'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
          variables: {
            product: {
              title,
              status: 'DRAFT',
            },
          },
        });

      expect(createResponse.status).toBe(200);
      expect(createResponse.body.data.productCreate.userErrors).toEqual([]);
    }

    const commitResponse = await request(server)
      .post('/__meta/commit')
      .set('x-shopify-access-token', 'shpat_commit_test');

    expect(commitResponse.status).toBe(200);
    expect(commitResponse.body).toEqual({
      ok: false,
      stopIndex: 1,
      attempts: [
        expect.objectContaining({
          operationName: 'productCreate',
          path: '/admin/api/2025-01/graphql.json',
          status: 'committed',
          upstreamStatus: 200,
          responseBody: { data: { productCreate: { product: { id: 'gid://shopify/Product/1' }, userErrors: [] } } },
        }),
        expect.objectContaining({
          operationName: 'productCreate',
          path: '/admin/api/2025-01/graphql.json',
          status: 'failed',
          upstreamStatus: 200,
          responseBody: { errors: [{ message: 'write scope denied' }] },
        }),
      ],
    });

    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(fetchSpy.mock.calls[0]?.[0].toString()).toBe('https://example.myshopify.com/admin/api/2025-01/graphql.json');
    expect(fetchSpy.mock.calls[0]?.[1]).toMatchObject({
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        'x-shopify-access-token': 'shpat_commit_test',
      },
    });

    const log = await request(server).get('/__meta/log');
    expect(log.status).toBe(200);
    expect(log.body.entries).toHaveLength(3);
    expect(log.body.entries.map((entry: { status: string }) => entry.status)).toEqual(['committed', 'failed', 'staged']);
  });
});
