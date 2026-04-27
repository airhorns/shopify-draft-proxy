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

function productCreateBodyWithTitle(title: string): typeof productCreateBody {
  return {
    ...productCreateBody,
    variables: {
      product: {
        title,
        status: 'DRAFT',
      },
    },
  };
}

function readProductCreateId(responseBody: unknown): string {
  const data = (responseBody as { data?: { productCreate?: { product?: { id?: unknown } } } }).data;
  const id = data?.productCreate?.product?.id;

  if (typeof id !== 'string') {
    throw new Error('Expected productCreate.product.id in response body');
  }

  return id;
}

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

  it('keeps staged state, logs, resets, and synthetic IDs isolated by instance', async () => {
    const firstProxy = createDraftProxy(config);
    const secondProxy = createDraftProxy(config);

    const firstCreateResponse = await firstProxy.processGraphQLRequest(
      productCreateBodyWithTitle('First Instance Hat'),
      {
        apiVersion: '2025-01',
      },
    );

    expect(firstCreateResponse.status).toBe(200);
    expect(firstCreateResponse.body).toMatchObject({
      data: {
        productCreate: {
          product: {
            title: 'First Instance Hat',
          },
          userErrors: [],
        },
      },
    });
    const firstProductId = readProductCreateId(firstCreateResponse.body);

    expect(firstProxy.getLog().entries).toHaveLength(1);
    expect(secondProxy.getLog().entries).toEqual([]);
    expect(Object.keys(firstProxy.getState().stagedState.products)).toEqual([firstProductId]);
    expect(secondProxy.getState().stagedState.products).toEqual({});

    const secondCreateResponse = await secondProxy.processGraphQLRequest(
      productCreateBodyWithTitle('Second Instance Hat'),
      {
        apiVersion: '2025-01',
      },
    );

    expect(secondCreateResponse.status).toBe(200);
    const secondProductId = readProductCreateId(secondCreateResponse.body);

    expect(secondProductId).toBe(firstProductId);
    expect(firstProxy.getState().stagedState.products[firstProductId]?.title).toBe('First Instance Hat');
    expect(secondProxy.getState().stagedState.products[secondProductId]?.title).toBe('Second Instance Hat');

    expect(secondProxy.clear()).toEqual({
      ok: true,
      message: 'state reset',
    });
    expect(secondProxy.getLog().entries).toEqual([]);
    expect(firstProxy.getLog().entries).toHaveLength(1);
    expect(firstProxy.getState().stagedState.products[firstProductId]?.title).toBe('First Instance Hat');
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
