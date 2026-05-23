import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { operationRegistrySchema, type OperationRegistryEntry } from './json-schemas.js';

export type CapabilityDomain =
  | 'products'
  | 'admin-platform'
  | 'b2b'
  | 'apps'
  | 'media'
  | 'bulk-operations'
  | 'customers'
  | 'orders'
  | 'store-properties'
  | 'discounts'
  | 'events'
  | 'functions'
  | 'payments'
  | 'marketing'
  | 'online-store'
  | 'saved-searches'
  | 'privacy'
  | 'segments'
  | 'shipping-fulfillments'
  | 'gift-cards'
  | 'webhooks'
  | 'localization'
  | 'markets'
  | 'metafields'
  | 'metaobjects'
  | 'unknown';
export type CapabilityExecution = 'overlay-read' | 'stage-locally' | 'passthrough';
export type OperationType = 'query' | 'mutation';

const defaultRepoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const registrySourceRelativePath = 'config/operation-registry.json';

export function loadOperationRegistryFromSource(repoRoot = defaultRepoRoot): OperationRegistryEntry[] {
  return operationRegistrySchema.parse(JSON.parse(readFileSync(resolve(repoRoot, registrySourceRelativePath), 'utf8')));
}

const operationRegistry = loadOperationRegistryFromSource();

export function listOperationRegistryEntries(): OperationRegistryEntry[] {
  return operationRegistry.map((entry) => ({
    ...entry,
    matchNames: [...entry.matchNames],
    runtimeTests: [...entry.runtimeTests],
  }));
}

export function listImplementedOperationRegistryEntries(): OperationRegistryEntry[] {
  return operationRegistry.filter((entry) => entry.implemented);
}

export function findOperationRegistryEntry(
  type: OperationType,
  names: Array<string | null | undefined>,
): OperationRegistryEntry | null {
  const candidates = names.filter((name): name is string => typeof name === 'string' && name.length > 0);
  for (const candidate of candidates) {
    const entry = operationRegistry.find(
      (registryEntry) => registryEntry.type === type && registryEntry.matchNames.includes(candidate),
    );
    if (entry) {
      return {
        ...entry,
        matchNames: [...entry.matchNames],
        runtimeTests: [...entry.runtimeTests],
      };
    }
  }
  return null;
}
