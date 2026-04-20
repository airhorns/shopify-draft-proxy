import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import { store } from '../../src/state/store.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'passthrough',
};

const emptySnapshot = {
  products: {},
  productVariants: {},
  productOptions: {},
  collections: {},
  productCollections: {},
  productMedia: {},
  productMetafields: {},
  deletedProductIds: {},
  deletedCollectionIds: {},
};

describe('meta routes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('exposes a lightweight health endpoint', async () => {
    const app = createApp(config);
    const server = app.callback();

    const health = await request(server).get('/__meta/health');
    expect(health.status).toBe(200);
    expect(health.body).toEqual({
      ok: true,
      message: 'shopify-draft-proxy is running',
    });
  });

  it('exposes a reset endpoint', async () => {
    const app = createApp(config);
    const server = app.callback();

    const reset = await request(server).post('/__meta/reset');
    expect(reset.status).toBe(200);
    expect(reset.body).toEqual({
      ok: true,
      message: 'state reset',
    });
  });

  it('resets staged state, hydrated cache state, mutation logs, and synthetic IDs', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            products: {
              nodes: [
                {
                  id: 'gid://shopify/Product/9001',
                  title: 'Hydrated Base Hat',
                  handle: 'hydrated-base-hat',
                  status: 'ACTIVE',
                  createdAt: '2024-02-01T00:00:00.000Z',
                  updatedAt: '2024-02-02T00:00:00.000Z',
                },
              ],
            },
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );

    const app = createApp({ ...config, readMode: 'live-hybrid' }).callback();

    await request(app).post('/admin/api/2025-01/graphql.json').send({
      query: 'query { products(first: 10) { nodes { id title handle status createdAt updatedAt } } }',
    });

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Staged Reset Hat" }) { product { id title createdAt } userErrors { field message } } }',
    });

    const createdProduct = createResponse.body.data.productCreate.product;
    const stateBeforeReset = await request(app).get('/__meta/state');
    const logBeforeReset = await request(app).get('/__meta/log');

    expect(stateBeforeReset.body.baseState.products['gid://shopify/Product/9001']).toMatchObject({
      title: 'Hydrated Base Hat',
    });
    expect(stateBeforeReset.body.stagedState.products[createdProduct.id]).toMatchObject({
      title: 'Staged Reset Hat',
    });
    expect(logBeforeReset.body.entries).toHaveLength(1);
    expect(logBeforeReset.body.entries[0]).toMatchObject({
      operationName: 'productCreate',
      status: 'staged',
    });

    const resetResponse = await request(app).post('/__meta/reset');
    const stateAfterReset = await request(app).get('/__meta/state');
    const logAfterReset = await request(app).get('/__meta/log');

    expect(resetResponse.status).toBe(200);
    expect(resetResponse.body).toEqual({
      ok: true,
      message: 'state reset',
    });
    expect(stateAfterReset.body).toEqual({
      baseState: emptySnapshot,
      stagedState: emptySnapshot,
    });
    expect(logAfterReset.body).toEqual({ entries: [] });

    const createAfterReset = await request(app).post('/admin/api/2025-01/graphql.json').send({
      query:
        'mutation { productCreate(product: { title: "Staged Reset Hat" }) { product { id title createdAt } userErrors { field message } } }',
    });

    expect(createAfterReset.body.data.productCreate.product).toEqual(createdProduct);
  });
});
