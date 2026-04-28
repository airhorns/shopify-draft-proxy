import Koa from 'koa';
import { createApp as createRuntimeApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { createDraftProxy, type DraftProxy } from '../../src/proxy-instance.js';
import { InMemoryStore, runWithStore } from '../../src/state/store.js';
import { runWithSyntheticIdentity, SyntheticIdentityRegistry } from '../../src/state/synthetic-identity.js';

let currentStore = new InMemoryStore();
let currentSyntheticIdentity = new SyntheticIdentityRegistry();
let runtimePrepared = false;

function prepareRuntime(): void {
  if (runtimePrepared) {
    return;
  }
  currentStore = new InMemoryStore();
  currentSyntheticIdentity = new SyntheticIdentityRegistry();
  runtimePrepared = true;
}

export const store = new Proxy({} as InMemoryStore, {
  get(_target, property) {
    prepareRuntime();
    const value = Reflect.get(currentStore, property);
    return typeof value === 'function' ? value.bind(currentStore) : value;
  },
  set(_target, property, value) {
    prepareRuntime();
    return Reflect.set(currentStore, property, value);
  },
}) as InMemoryStore;

export function resetSyntheticIdentity(): void {
  prepareRuntime();
  currentSyntheticIdentity.reset();
}

export function withRuntimeContext<T>(callback: () => T): T {
  prepareRuntime();
  return runWithStore(currentStore, () => runWithSyntheticIdentity(currentSyntheticIdentity, callback));
}

export function createApp(config: AppConfig, proxy?: DraftProxy): Koa {
  if (proxy) {
    return createRuntimeApp(config, proxy);
  }

  prepareRuntime();
  return createRuntimeApp(
    config,
    createDraftProxy(config, {
      store: currentStore,
      syntheticIdentity: currentSyntheticIdentity,
    }),
  );
}
