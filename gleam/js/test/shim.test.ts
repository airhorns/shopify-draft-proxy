// Vitest coverage for the Gleam-port shim. Verifies the public TS
// surface lights up end-to-end: create a proxy, hit /__meta/health,
// dispatch a saved-search mutation, dump and restore state.

import { describe, expect, it } from 'vitest';
import {
  createApp,
  createDraftProxy,
  DRAFT_PROXY_STATE_DUMP_SCHEMA,
  DraftProxy,
  loadConfig,
  type AppConfig,
} from '../src/index.js';

const baseConfig: AppConfig = {
  readMode: 'snapshot',
  port: 4000,
  shopifyAdminOrigin: 'https://shopify.com',
};

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
      runtime: { readMode: 'snapshot' },
      proxy: { port: 4000, shopifyAdminOrigin: 'https://shopify.com' },
      snapshot: { enabled: false, path: null },
    });
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

  it('accepts an initial state dump through DraftProxyOptions', () => {
    const proxy = createDraftProxy(baseConfig);
    const dump = proxy.dumpState('2026-04-29T12:00:00.000Z');

    const restored = createDraftProxy(baseConfig, { state: dump });
    expect(restored.getState()).toBeDefined();
  });

  it('processGraphQLRequest dispatches through the Admin GraphQL route', async () => {
    const proxy = createDraftProxy(baseConfig);
    const response = await proxy.processGraphQLRequest({
      query:
        'mutation { savedSearchCreate(input: { name: "Smoke", query: "tag:vip", resourceType: ORDER }) { savedSearch { id name } userErrors { field message } } }',
    });
    expect(response.status).toBe(200);
    expect(response.body).toMatchObject({
      data: {
        savedSearchCreate: {
          savedSearch: {
            id: expect.stringContaining('gid://shopify/SavedSearch/'),
            name: 'Smoke',
          },
          userErrors: [],
        },
      },
    });
  });

  it('commit exposes the empty-log commit envelope', async () => {
    const proxy = createDraftProxy(baseConfig);
    const result = await proxy.commit();
    expect(result).toEqual({
      stopIndex: null,
      attempts: [],
    });
  });
});

describe('public API stubs', () => {
  it('createApp throws not-implemented', () => {
    expect(() => createApp()).toThrow(/not implemented/);
  });

  it('loadConfig throws not-implemented', () => {
    expect(() => loadConfig()).toThrow(/not implemented/);
  });
});
