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

describe('meta routes', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('exposes health and reset endpoints', async () => {
    const app = createApp(config);
    const server = app.callback();

    const health = await request(server).get('/__meta/health');
    expect(health.status).toBe(200);
    expect(health.body).toEqual({
      ok: true,
      message: 'shopify-draft-proxy is running',
    });

    const reset = await request(server).post('/__meta/reset');
    expect(reset.status).toBe(200);
    expect(reset.body).toEqual({
      ok: true,
      message: 'state reset',
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
