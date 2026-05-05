// Vitest coverage for the Gleam-port shim. Verifies the public TS
// surface lights up end-to-end: create a proxy, hit /__meta/health,
// dispatch a saved-search mutation, dump and restore state.

import { beforeAll, describe, expect, it } from 'vitest';
import {
  createApp,
  createDraftProxy,
  DRAFT_PROXY_STATE_DUMP_SCHEMA,
  DraftProxy,
  loadConfig,
  type AppConfig,
} from '../src/index.js';
import { ensureGleamJavaScriptBuild } from './support/gleam-build.js';

const baseConfig: AppConfig = {
  readMode: 'snapshot',
  port: 4000,
  shopifyAdminOrigin: 'https://shopify.com',
};

beforeAll(() => {
  ensureGleamJavaScriptBuild();
});

describe('createDraftProxy', () => {
  it('produces a DraftProxy instance', () => {
    const proxy = createDraftProxy(baseConfig);
    expect(proxy).toBeInstanceOf(DraftProxy);
  });

  it('answers /__meta/health with the documented envelope', async () => {
    const proxy = createDraftProxy(baseConfig);
    const response = await proxy.processRequest({
      method: 'GET',
      path: '/__meta/health',
    });
    expect(response.status).toBe(200);
    expect(response.body).toMatchObject({
      ok: true,
      message: expect.stringContaining('shopify-draft-proxy'),
    });
  });

  it('exposes a config snapshot matching the shape TS callers expect', () => {
    const proxy = createDraftProxy(baseConfig);
    const snapshot = proxy.getConfig();
    expect(snapshot).toMatchObject({
      runtime: { readMode: 'snapshot', unsupportedMutationMode: 'passthrough' },
      proxy: { port: 4000, shopifyAdminOrigin: 'https://shopify.com' },
      snapshot: { enabled: false, path: null },
    });
  });

  it('keeps the legacy passthrough read mode string in config snapshots', () => {
    const proxy = createDraftProxy({
      ...baseConfig,
      readMode: 'passthrough',
    });
    expect(proxy.getConfig().runtime.readMode).toBe('passthrough');
  });

  it('dumps and restores state across instances', () => {
    const proxy = createDraftProxy(baseConfig);
    const dump = proxy.dumpState('2026-04-29T12:00:00.000Z');
    expect(dump.schema).toBe(DRAFT_PROXY_STATE_DUMP_SCHEMA);
    expect(dump.createdAt).toBe('2026-04-29T12:00:00.000Z');

    const fresh = createDraftProxy(baseConfig);
    fresh.restoreState(dump);
    // After restore, getState should serialize without throwing.
    expect(fresh.getState()).toBeDefined();
  });

  it('stages saved-search mutations through the Gleam-backed shim', async () => {
    const proxy = createDraftProxy(baseConfig);

    const createResponse = await proxy.processRequest({
      method: 'POST',
      path: '/admin/api/2026-04/graphql.json',
      body: {
        query:
          'mutation { savedSearchCreate(input: { name: "Promo products", query: "tag:promo", resourceType: PRODUCT }) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } } }',
      },
    });

    expect(createResponse.status).toBe(200);
    expect(createResponse.body).toMatchObject({
      data: {
        savedSearchCreate: {
          savedSearch: {
            id: 'gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic',
            name: 'Promo products',
            query: 'tag:promo',
            resourceType: 'PRODUCT',
            filters: [{ key: 'tag', value: 'promo' }],
          },
          userErrors: [],
        },
      },
    });

    const readResponse = await proxy.processRequest({
      method: 'POST',
      path: '/admin/api/2026-04/graphql.json',
      body: {
        query: '{ productSavedSearches(first: 1) { nodes { id name resourceType } } }',
      },
    });

    expect(readResponse.status).toBe(200);
    expect(readResponse.body).toMatchObject({
      data: {
        productSavedSearches: {
          nodes: [
            {
              id: 'gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic',
              name: 'Promo products',
              resourceType: 'PRODUCT',
            },
          ],
        },
      },
    });
  });
});

describe('public API server helpers', () => {
  it('createApp returns a Node HTTP adapter over a DraftProxy instance', () => {
    const proxy = createDraftProxy(baseConfig);
    const app = createApp(baseConfig, proxy);
    expect(app.proxy).toBe(proxy);
    expect(typeof app.callback()).toBe('function');
  });

  it('loadConfig mirrors the legacy package configuration parser', () => {
    expect(
      loadConfig({
        PORT: '4111',
        SHOPIFY_ADMIN_ORIGIN: 'https://example.myshopify.com',
        SHOPIFY_DRAFT_PROXY_READ_MODE: 'live-hybrid',
        SHOPIFY_DRAFT_PROXY_UNSUPPORTED_MUTATION_MODE: 'reject',
      }),
    ).toEqual({
      port: 4111,
      shopifyAdminOrigin: 'https://example.myshopify.com',
      readMode: 'live-hybrid',
      unsupportedMutationMode: 'reject',
    });
  });
});
