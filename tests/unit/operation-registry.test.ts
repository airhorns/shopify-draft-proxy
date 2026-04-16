import { describe, expect, it } from 'vitest';
import {
  listImplementedOperationRegistryEntries,
  listOperationRegistryEntries,
} from '../../src/proxy/operation-registry.js';

describe('operation registry', () => {
  it('keeps implemented capability names unique', () => {
    const implementedNames = listImplementedOperationRegistryEntries().map((entry) => entry.name);
    expect(new Set(implementedNames).size).toBe(implementedNames.length);
  });

  it('requires implemented operations to declare runtime tests and conformance metadata', () => {
    for (const entry of listImplementedOperationRegistryEntries()) {
      expect(entry.runtimeTests.length).toBeGreaterThan(0);
      expect(['covered', 'declared-gap']).toContain(entry.conformance.status);
      if (entry.conformance.status === 'covered') {
        expect(entry.conformance.scenarioIds?.length ?? 0).toBeGreaterThan(0);
      }
      if (entry.conformance.status === 'declared-gap') {
        expect(entry.conformance.reason?.length ?? 0).toBeGreaterThan(0);
      }
    }
  });

  it('exposes both overlay-read and stage-locally implemented operations', () => {
    const executions = new Set(listOperationRegistryEntries().map((entry) => entry.execution));
    expect(executions.has('overlay-read')).toBe(true);
    expect(executions.has('stage-locally')).toBe(true);
  });
});
