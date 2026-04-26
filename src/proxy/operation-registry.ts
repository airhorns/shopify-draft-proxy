import operationRegistryJson from '../../config/operation-registry.json' with { type: 'json' };
import { operationRegistrySchema, type OperationRegistryEntry } from '../json-schemas.js';

export type CapabilityDomain =
  | 'products'
  | 'media'
  | 'bulk-operations'
  | 'customers'
  | 'orders'
  | 'store-properties'
  | 'discounts'
  | 'payments'
  | 'marketing'
  | 'online-store'
  | 'privacy'
  | 'segments'
  | 'shipping-fulfillments'
  | 'gift-cards'
  | 'webhooks'
  | 'markets'
  | 'metafields'
  | 'metaobjects'
  | 'unknown';
export type CapabilityExecution = 'overlay-read' | 'stage-locally' | 'passthrough';
export type OperationType = 'query' | 'mutation';

const operationRegistry = operationRegistrySchema.parse(operationRegistryJson);

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
