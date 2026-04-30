import { describe, expect, it } from 'vitest';
import { InMemoryStore } from '../../src/state/store.js';
import { SyntheticIdentityRegistry } from '../../src/state/synthetic-identity.js';

type StoreInternals = Record<string, unknown>;

function sentinelForStoreField(key: string, currentValue: unknown): unknown {
  if (currentValue instanceof Map) {
    const entry: [string, unknown] = [key, key === 'laggedTagSearchProductIds' ? 7 : `${key}:value`];
    return new Map([entry]);
  }

  if (currentValue instanceof Set) {
    return new Set([`${key}:value`]);
  }

  if (Array.isArray(currentValue)) {
    return [`${key}:value`];
  }

  if (currentValue === null || typeof currentValue === 'object') {
    return {
      [`${key}:id`]: {
        __sentinel: key,
      },
    };
  }

  if (typeof currentValue === 'number') {
    return 7;
  }

  if (typeof currentValue === 'string') {
    return `${key}:value`;
  }

  if (typeof currentValue === 'boolean') {
    return !currentValue;
  }

  return {
    __sentinel: key,
  };
}

function seedEveryEnumerableStoreField(store: InMemoryStore): void {
  const internals = store as unknown as StoreInternals;
  for (const [key, value] of Object.entries(internals)) {
    internals[key] = sentinelForStoreField(key, value);
  }
}

function normalizeForComparison(value: unknown): unknown {
  if (value instanceof Map) {
    return {
      __type: 'Map',
      entries: [...value.entries()]
        .map(([entryKey, entryValue]) => [normalizeForComparison(entryKey), normalizeForComparison(entryValue)])
        .sort(([left], [right]) => String(left).localeCompare(String(right))),
    };
  }

  if (value instanceof Set) {
    return {
      __type: 'Set',
      values: [...value.values()]
        .map(normalizeForComparison)
        .sort((left, right) => String(left).localeCompare(String(right))),
    };
  }

  if (Array.isArray(value)) {
    return value.map(normalizeForComparison);
  }

  if (value && typeof value === 'object') {
    const record = value as Record<string, unknown>;
    return Object.fromEntries(
      Object.keys(record)
        .sort()
        .map((key) => [key, normalizeForComparison(record[key])]),
    );
  }

  return value;
}

function normalizeStoreInternals(store: InMemoryStore): Record<string, unknown> {
  const internals = store as unknown as StoreInternals;
  return Object.fromEntries(
    Object.keys(internals)
      .sort()
      .map((key) => [key, normalizeForComparison(internals[key])]),
  );
}

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

  it('dump/restore preserves every enumerable in-memory store state field', () => {
    const store = new InMemoryStore();
    seedEveryEnumerableStoreField(store);

    expect(Object.keys(store.dumpRuntimeState().fields).sort()).toEqual(
      Object.keys(store as unknown as StoreInternals).sort(),
    );

    const restored = new InMemoryStore();
    restored.restoreRuntimeState(
      JSON.parse(JSON.stringify(store.dumpRuntimeState())) as ReturnType<InMemoryStore['dumpRuntimeState']>,
    );

    expect(normalizeStoreInternals(restored)).toEqual(normalizeStoreInternals(store));
  });

  it('rejects dumps that omit a current in-memory store state field', () => {
    const store = new InMemoryStore();
    seedEveryEnumerableStoreField(store);

    const dump = JSON.parse(JSON.stringify(store.dumpRuntimeState())) as ReturnType<InMemoryStore['dumpRuntimeState']>;
    const omittedKey = Object.keys(store as unknown as StoreInternals)[0];
    if (!omittedKey) {
      throw new Error('Expected InMemoryStore to have enumerable state fields');
    }
    delete dump.fields[omittedKey];

    const restored = new InMemoryStore();

    expect(() => restored.restoreRuntimeState(dump)).toThrow(
      `In-memory store state dump is missing required fields: ${omittedKey}`,
    );
  });

  it('ignores unknown in-memory store state fields from newer dump writers', () => {
    const store = new InMemoryStore();
    seedEveryEnumerableStoreField(store);

    const dump = JSON.parse(JSON.stringify(store.dumpRuntimeState())) as ReturnType<InMemoryStore['dumpRuntimeState']>;
    dump.fields['__futureStoreField'] = {
      kind: 'plain',
      value: {
        ignored: true,
      },
    };

    const restored = new InMemoryStore();
    restored.restoreRuntimeState(dump);

    expect(normalizeStoreInternals(restored)).toEqual(normalizeStoreInternals(store));
  });
});
