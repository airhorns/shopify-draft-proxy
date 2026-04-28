import type { SyntheticIdentityRegistry } from '../state/synthetic-identity.js';
import type { InMemoryStore } from '../state/store.js';

export interface ProxyRuntimeContext {
  store: InMemoryStore;
  syntheticIdentity: SyntheticIdentityRegistry;
}
