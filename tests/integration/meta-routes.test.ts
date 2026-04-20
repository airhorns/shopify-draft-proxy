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
  publications: {},
  customers: {},
  productCollections: {},
  productMedia: {},
  files: {},
  productMetafields: {},
  deletedProductIds: {},
  deletedCollectionIds: {},
  deletedCustomerIds: {},
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

  it('replays staged mutations in original order, stops on the first upstream failure, and persists commit statuses in the log', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(async () => {
        return new Response(
          JSON.stringify({ data: { productCreate: { product: { id: 'gid://shopify/Product/1' }, userErrors: [] } } }),
          {
            status: 200,
            headers: { 'content-type': 'application/json' },
          },
        );
      })
      .mockImplementationOnce(async () => {
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
    expect(log.body.entries.map((entry: { status: string }) => entry.status)).toEqual([
      'committed',
      'failed',
      'staged',
    ]);
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

  it('exposes safe effective proxy configuration and runtime mode', async () => {
    const app = createApp({
      ...config,
      port: 4123,
      readMode: 'snapshot',
      snapshotPath: 'fixtures/snapshots/dev-store.json',
    });

    const response = await request(app.callback()).get('/__meta/config');

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      runtime: {
        readMode: 'snapshot',
      },
      proxy: {
        port: 4123,
        shopifyAdminOrigin: 'https://example.myshopify.com',
      },
      snapshot: {
        enabled: true,
        path: 'fixtures/snapshots/dev-store.json',
      },
    });
  });

  it('reports disabled snapshot configuration without inventing a path', async () => {
    const app = createApp(config);

    const response = await request(app.callback()).get('/__meta/config');

    expect(response.status).toBe(200);
    expect(response.body.snapshot).toEqual({
      enabled: false,
      path: null,
    });
  });

  it('exposes an empty ordered mutation log before anything is staged', async () => {
    const app = createApp(config);

    const response = await request(app.callback()).get('/__meta/log');

    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      entries: [],
    });
  });

  it('exposes raw staged mutation documents with interpreted metadata', async () => {
    const app = createApp(config);
    const server = app.callback();
    const query =
      'mutation StageHat($title: String!) { productCreate(product: { title: $title }) { product { id title } userErrors { field message } } }';
    const variables = { title: 'Hat' };
    const secondQuery =
      'mutation StageShirt { productCreate(product: { title: "Shirt" }) { product { id title } userErrors { field message } } }';

    const mutation = await request(server).post('/admin/api/2025-01/graphql.json').send({
      query,
      variables,
    });
    const secondMutation = await request(server).post('/admin/api/2025-01/graphql.json').send({
      query: secondQuery,
    });

    expect(mutation.status).toBe(200);
    expect(secondMutation.status).toBe(200);

    const response = await request(server).get('/__meta/log');

    expect(response.status).toBe(200);
    expect(response.body.entries).toHaveLength(2);
    expect(response.body.entries.map((entry: { query: string }) => entry.query)).toEqual([query, secondQuery]);
    expect(response.body.entries[0]).toMatchObject({
      id: 'gid://shopify/MutationLogEntry/1',
      operationName: 'productCreate',
      query,
      variables,
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'StageHat',
        rootFields: ['productCreate'],
        primaryRootField: 'productCreate',
        capability: {
          operationName: 'productCreate',
          domain: 'products',
          execution: 'stage-locally',
        },
      },
    });
    expect(response.body.entries[1]).toMatchObject({
      operationName: 'productCreate',
      query: secondQuery,
      variables: {},
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'StageShirt',
        rootFields: ['productCreate'],
        primaryRootField: 'productCreate',
        capability: {
          operationName: 'productCreate',
          domain: 'products',
          execution: 'stage-locally',
        },
      },
    });
  });

  it('keeps unsupported mutation passthrough visible in the inspected log', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ data: { unsupportedMutation: { ok: true } } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const app = createApp(config);
    const server = app.callback();
    const query = 'mutation Passthrough { unsupportedMutation { ok } }';

    const mutation = await request(server)
      .post('/admin/api/2025-01/graphql.json')
      .set('x-shopify-access-token', 'shpat_test')
      .send({ query });

    expect(mutation.status).toBe(200);

    const response = await request(server).get('/__meta/log');

    expect(response.status).toBe(200);
    expect(response.body.entries).toHaveLength(1);
    expect(response.body.entries[0]).toMatchObject({
      operationName: 'Passthrough',
      query,
      variables: {},
      status: 'proxied',
      interpreted: {
        operationType: 'mutation',
        operationName: 'Passthrough',
        rootFields: ['unsupportedMutation'],
        primaryRootField: 'unsupportedMutation',
        capability: {
          operationName: 'Passthrough',
          domain: 'unknown',
          execution: 'passthrough',
        },
      },
    });
  });
});
