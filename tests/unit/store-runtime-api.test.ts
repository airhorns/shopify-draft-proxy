import { describe, expect, it } from 'vitest';
import { InMemoryStore } from '../../src/state/store.js';
import { SyntheticIdentityRegistry } from '../../src/state/synthetic-identity.js';

describe('InMemoryStore runtime API', () => {
  it('exposes meta log and state through high-level methods', () => {
    const store = new InMemoryStore();
    const identity = new SyntheticIdentityRegistry();

    store.recordMutationLogEntry({
      id: identity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: identity.makeSyntheticTimestamp(),
      operationName: 'productCreate',
      path: '/admin/api/2025-01/graphql.json',
      query: 'mutation { productCreate(product: { title: "Hat" }) { product { id } } }',
      variables: {},
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'productCreate',
        rootFields: ['productCreate'],
        primaryRootField: 'productCreate',
        capability: {
          operationName: 'productCreate',
          domain: 'products',
          execution: 'stage-locally',
        },
      },
    });

    expect(store.getMetaLog().entries).toHaveLength(1);
    expect(store.getMetaState()).toEqual(store.getState());
  });

  it('resets store state and synthetic identity together', () => {
    const store = new InMemoryStore();
    const identity = new SyntheticIdentityRegistry();

    const firstId = identity.makeSyntheticGid('Product');
    store.recordMutationLogEntry({
      id: identity.makeSyntheticGid('MutationLogEntry'),
      receivedAt: identity.makeSyntheticTimestamp(),
      operationName: 'productCreate',
      path: '/admin/api/2025-01/graphql.json',
      query: 'mutation { productCreate(product: { title: "Hat" }) { product { id } } }',
      variables: {},
      status: 'staged',
      interpreted: {
        operationType: 'mutation',
        operationName: 'productCreate',
        rootFields: ['productCreate'],
        primaryRootField: 'productCreate',
        capability: {
          operationName: 'productCreate',
          domain: 'products',
          execution: 'stage-locally',
        },
      },
    });

    expect(store.resetRuntimeState(identity)).toEqual({
      ok: true,
      message: 'state reset',
    });
    expect(store.getMetaLog().entries).toEqual([]);
    expect(identity.makeSyntheticGid('Product')).toBe(firstId);
  });

  it('stages upload content and returns Shopify-like staged upload metadata', () => {
    const store = new InMemoryStore();

    expect(store.stageStagedUpload('gid://shopify/Product/1', 'image.png', 'binary-content')).toEqual({
      ok: true,
      key: 'shopify-draft-proxy/gid://shopify/Product/1/image.png',
    });
    expect(store.getStagedUploadContent('shopify-draft-proxy/gid://shopify/Product/1/image.png')).toBe(
      'binary-content',
    );
  });
});
