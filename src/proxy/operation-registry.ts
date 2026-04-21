import operationRegistryJson from '../../config/operation-registry.json' with { type: 'json' };

export type CapabilityDomain = 'products' | 'media' | 'customers' | 'orders' | 'unknown';
export type CapabilityExecution = 'overlay-read' | 'stage-locally' | 'passthrough';
export type OperationType = 'query' | 'mutation';

export interface OperationRegistryEntry {
  name: string;
  type: OperationType;
  domain: CapabilityDomain;
  execution: CapabilityExecution;
  implemented: boolean;
  matchNames: string[];
  runtimeTests: string[];
}

const operationRegistry = operationRegistryJson as OperationRegistryEntry[];

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
