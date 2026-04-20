import operationRegistryJson from '../../config/operation-registry.json' with { type: 'json' };

export type CapabilityDomain = 'products' | 'media' | 'unknown';
export type CapabilityExecution = 'overlay-read' | 'stage-locally' | 'passthrough';
export type ConformanceStatus = 'covered' | 'declared-gap';
export type OperationType = 'query' | 'mutation';

export interface OperationRegistryEntry {
  name: string;
  type: OperationType;
  domain: CapabilityDomain;
  execution: CapabilityExecution;
  implemented: boolean;
  matchNames: string[];
  runtimeTests: string[];
  conformance: {
    status: ConformanceStatus;
    scenarioIds: string[];
    reason?: string;
  };
}

const operationRegistry = operationRegistryJson as OperationRegistryEntry[];

export function listOperationRegistryEntries(): OperationRegistryEntry[] {
  return operationRegistry.map((entry) => ({
    ...entry,
    matchNames: [...entry.matchNames],
    runtimeTests: [...entry.runtimeTests],
    conformance: {
      ...entry.conformance,
      ...(entry.conformance.scenarioIds ? { scenarioIds: [...entry.conformance.scenarioIds] } : {}),
    },
  }));
}

export function listImplementedOperationRegistryEntries(): OperationRegistryEntry[] {
  return operationRegistry.filter((entry) => entry.implemented);
}
