import Koa from 'koa';
import { createApp as createRuntimeApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import type { ProxyRuntimeContext } from '../../src/proxy/runtime-context.js';
import { createDraftProxy, type DraftProxy } from '../../src/proxy-instance.js';
import { InMemoryStore } from '../../src/state/store.js';
import { SyntheticIdentityRegistry } from '../../src/state/synthetic-identity.js';

let currentStore = new InMemoryStore();
let currentSyntheticIdentity = new SyntheticIdentityRegistry();
let currentProxy: DraftProxy | null = null;

export function resetTestRuntime(): void {
  currentStore = new InMemoryStore();
  currentSyntheticIdentity = new SyntheticIdentityRegistry();
  currentProxy = null;
}

function createTestProxy(config: AppConfig): DraftProxy {
  currentProxy ??= createDraftProxy(config, {
    store: currentStore,
    syntheticIdentity: currentSyntheticIdentity,
  });
  return currentProxy;
}

export const store = new Proxy({} as InMemoryStore, {
  get(_target, property) {
    const value = Reflect.get(currentStore, property);
    return typeof value === 'function' ? value.bind(currentStore) : value;
  },
  set(_target, property, value) {
    return Reflect.set(currentStore, property, value);
  },
}) as InMemoryStore;

export function resetSyntheticIdentity(): void {
  currentSyntheticIdentity.reset();
}

export function withRuntimeContext<T>(callback: (runtime: ProxyRuntimeContext) => T): T {
  return callback({ store: currentStore, syntheticIdentity: currentSyntheticIdentity });
}

export function createApp(config: AppConfig, proxy?: DraftProxy): Koa {
  if (proxy) {
    return createRuntimeApp(config, proxy);
  }

  return createRuntimeApp(config, createTestProxy(config));
}
