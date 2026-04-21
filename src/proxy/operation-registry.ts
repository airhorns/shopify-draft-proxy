import operationRegistryJson from '../../config/operation-registry.json' with { type: 'json' };
import { operationRegistrySchema, type OperationRegistryEntry } from '../json-schemas.js';

export type CapabilityDomain = 'products' | 'media' | 'customers' | 'orders' | 'unknown';
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
