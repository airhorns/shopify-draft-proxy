import request from 'supertest';
import { describe, expect, it } from 'vitest';
import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { createDraftProxy } from '../../src/proxy-instance.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'passthrough',
};

const productCreateBody = {
  query:
    'mutation CreateDraft($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
  operationName: 'CreateDraft',
  variables: {
    product: {
      title: 'Library Staged Hat',
      status: 'DRAFT',
    },
  },
};

describe('draft proxy public instance API', () => {
  it('processes meta requests without creating a Koa app', async () => {
    const proxy = createDraftProxy(config);

    const health = await proxy.processRequest({
      method: 'GET',
      path: '/__meta/health',
    });

    expect(health.status).toBe(200);
    expect(health.body).toEqual({
      ok: true,
      message: 'shopify-draft-proxy is running',
    });
    expect(proxy.getConfig()).toEqual({
      runtime: { readMode: 'passthrough' },
      proxy: {
        port: 3000,
        shopifyAdminOrigin: 'https://example.myshopify.com',
      },
      snapshot: {
        enabled: false,
        path: null,
      },
    });
  });

  it('stages supported mutations and keeps instance state isolated', async () => {
    const firstProxy = createDraftProxy(config);
    const secondProxy = createDraftProxy(config);

    const createResponse = await firstProxy.processGraphQLRequest(productCreateBody, {
      apiVersion: '2025-01',
    });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body).toMatchObject({
      data: {
        productCreate: {
          product: {
            title: 'Library Staged Hat',
          },
          userErrors: [],
        },
      },
    });
    expect(firstProxy.getLog().entries).toHaveLength(1);
    expect(secondProxy.getLog().entries).toEqual([]);

    expect(firstProxy.clear()).toEqual({
      ok: true,
      message: 'state reset',
    });
    expect(firstProxy.getLog().entries).toEqual([]);
  });

  it('lets the Koa app mount a provided proxy instance', async () => {
    const proxy = createDraftProxy(config);
    const app = createApp(config, proxy).callback();

    const createResponse = await request(app).post('/admin/api/2025-01/graphql.json').send(productCreateBody);

    expect(createResponse.status).toBe(200);
    expect(proxy.getLog().entries).toHaveLength(1);
    expect(proxy.getLog().entries[0]).toMatchObject({
      operationName: 'productCreate',
      path: '/admin/api/2025-01/graphql.json',
      status: 'staged',
    });
  });
});
