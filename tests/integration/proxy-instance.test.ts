import request from 'supertest';
import { afterEach, describe, expect, it } from 'vitest';
import { createApp } from '../support/runtime.js';
import { createApp as createRuntimeApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import {
  createDraftProxy,
  DRAFT_PROXY_STATE_DUMP_SCHEMA,
  type DraftProxy,
  type DraftProxyOptions,
  type DraftProxyStateDump,
} from '../../src/proxy-instance.js';
import type { MutationLogEntry } from '../../src/state/types.js';

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

const productQuery = {
  query: 'query ReadProduct($id: ID!) { product(id: $id) { id title } }',
  operationName: 'ReadProduct',
};

const trackedProxies: DraftProxy[] = [];

function createTrackedDraftProxy(proxyConfig: AppConfig, options?: DraftProxyOptions): DraftProxy {
  const proxy = createDraftProxy(proxyConfig, options);
  trackedProxies.push(proxy);
  return proxy;
}

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

function readProductTitle(responseBody: unknown): string {
  const data = (responseBody as { data?: { product?: { title?: unknown } } }).data;
  const title = data?.product?.title;

  if (typeof title !== 'string') {
    throw new Error('Expected product.title in response body');
  }

  return title;
}

function comparableLogEntry(entry: MutationLogEntry): Omit<MutationLogEntry, 'id' | 'receivedAt'> {
  const { id: _id, receivedAt: _receivedAt, ...comparable } = entry;
  return comparable;
}

afterEach(() => {
  for (const proxy of trackedProxies) {
    const dump = JSON.parse(JSON.stringify(proxy.dumpState())) as DraftProxyStateDump;
    const restoredProxy = createDraftProxy(proxy.config, { state: dump });

    expect(restoredProxy.getLog()).toEqual(proxy.getLog());
    expect(restoredProxy.getState()).toEqual(proxy.getState());
  }

  trackedProxies.length = 0;
});

describe('draft proxy public instance API', () => {
  it('processes meta requests without creating a Koa app', async () => {
    const proxy = createTrackedDraftProxy(config);

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
    const firstProxy = createTrackedDraftProxy(config);
    const secondProxy = createTrackedDraftProxy(config);

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

    expect(secondProxy.reset()).toBeUndefined();
    expect(secondProxy.getLog().entries).toEqual([]);
    expect(firstProxy.getLog().entries).toHaveLength(1);
    expect(firstProxy.getState().stagedState.products[firstProductId]?.title).toBe('First Instance Hat');
  });

  it('lets the Koa app mount a provided proxy instance', async () => {
    const proxy = createTrackedDraftProxy(config);
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

  it('keeps default Koa app runtime state isolated by app instance', async () => {
    const firstApp = createRuntimeApp(config).callback();
    const secondApp = createRuntimeApp(config).callback();

    const firstCreateResponse = await request(firstApp)
      .post('/admin/api/2025-01/graphql.json')
      .send(productCreateBodyWithTitle('First Server Hat'));

    expect(firstCreateResponse.status).toBe(200);

    const firstStateResponse = await request(firstApp).get('/__meta/state');
    const secondStateResponse = await request(secondApp).get('/__meta/state');
    const firstProductId = readProductCreateId(firstCreateResponse.body);

    expect(firstStateResponse.status).toBe(200);
    expect(secondStateResponse.status).toBe(200);
    expect(Object.keys(firstStateResponse.body.stagedState.products)).toEqual([firstProductId]);
    expect(secondStateResponse.body.stagedState.products).toEqual({});
  });

  it('records equivalent mutation logs through the direct instance and HTTP app paths', async () => {
    const directProxy = createTrackedDraftProxy(config);
    const httpProxy = createTrackedDraftProxy(config);
    const app = createRuntimeApp(config, httpProxy).callback();

    const directResponse = await directProxy.processGraphQLRequest(productCreateBody, {
      apiVersion: '2025-01',
    });
    const httpResponse = await request(app).post('/admin/api/2025-01/graphql.json').send(productCreateBody);

    expect(directResponse.status).toBe(200);
    expect(httpResponse.status).toBe(200);
    expect(directResponse.body).toEqual(httpResponse.body);

    const directEntry = directProxy.getLog().entries[0];
    const httpEntry = httpProxy.getLog().entries[0];

    expect(directEntry).toBeDefined();
    expect(httpEntry).toBeDefined();
    if (!directEntry || !httpEntry) {
      throw new Error('Expected both proxy paths to record a mutation log entry');
    }
    expect(comparableLogEntry(directEntry)).toEqual(comparableLogEntry(httpEntry));
  });

  it('dumps and restores staged proxy state through a JSON-compatible envelope', async () => {
    const snapshotConfig: AppConfig = {
      ...config,
      readMode: 'snapshot',
    };
    const proxy = createTrackedDraftProxy(snapshotConfig);

    const createResponse = await proxy.processGraphQLRequest(productCreateBodyWithTitle('Restored Library Hat'), {
      apiVersion: '2025-01',
    });
    expect(createResponse.status).toBe(200);
    const productId = readProductCreateId(createResponse.body);

    const dump = proxy.dumpState();
    expect(dump).toMatchObject({
      schema: DRAFT_PROXY_STATE_DUMP_SCHEMA,
      version: 1,
      store: { version: 1 },
      syntheticIdentity: { version: 1 },
      extensions: {},
    });

    const jsonRoundTrip = JSON.parse(JSON.stringify(dump)) as DraftProxyStateDump;
    expect(jsonRoundTrip).toEqual(dump);

    const restoredProxy = createTrackedDraftProxy(snapshotConfig, { state: jsonRoundTrip });
    expect(restoredProxy.getLog()).toEqual(proxy.getLog());
    expect(restoredProxy.getState().stagedState.products[productId]?.title).toBe('Restored Library Hat');

    const readResponse = await restoredProxy.processGraphQLRequest(
      {
        ...productQuery,
        variables: { id: productId },
      },
      { apiVersion: '2025-01' },
    );

    expect(readResponse.status).toBe(200);
    expect(readProductTitle(readResponse.body)).toBe('Restored Library Hat');

    const restoredSecondCreate = await restoredProxy.processGraphQLRequest(
      productCreateBodyWithTitle('Restored Second Hat'),
      { apiVersion: '2025-01' },
    );
    const restoredSecondProductId = readProductCreateId(restoredSecondCreate.body);

    expect(proxy.getState().stagedState.products[restoredSecondProductId]).toBeUndefined();

    const originalSecondCreate = await proxy.processGraphQLRequest(productCreateBodyWithTitle('Original Second Hat'), {
      apiVersion: '2025-01',
    });
    expect(readProductCreateId(originalSecondCreate.body)).toBe(restoredSecondProductId);
    expect(proxy.getState().stagedState.products[restoredSecondProductId]?.title).toBe('Original Second Hat');
    expect(restoredProxy.getState().stagedState.products[restoredSecondProductId]?.title).toBe('Restored Second Hat');

    restoredProxy.reset();
    expect(restoredProxy.getLog().entries).toEqual([]);
    expect(restoredProxy.getState().stagedState.products).toEqual({});
  });

  it('restores dumps with extension fields while ignoring unknown top-level metadata', async () => {
    const snapshotConfig: AppConfig = {
      ...config,
      readMode: 'snapshot',
    };
    const proxy = createTrackedDraftProxy(snapshotConfig);
    const createResponse = await proxy.processGraphQLRequest(productCreateBodyWithTitle('Forward Compatible Hat'), {
      apiVersion: '2025-01',
    });
    const productId = readProductCreateId(createResponse.body);
    const dump = JSON.parse(JSON.stringify(proxy.dumpState())) as DraftProxyStateDump & {
      futureMetadata?: Record<string, unknown>;
    };

    delete dump.extensions;
    dump.futureMetadata = { ignored: true };

    const restoredProxy = createTrackedDraftProxy(snapshotConfig);
    restoredProxy.restoreState(dump);

    expect(restoredProxy.getState().stagedState.products[productId]?.title).toBe('Forward Compatible Hat');
  });
});
