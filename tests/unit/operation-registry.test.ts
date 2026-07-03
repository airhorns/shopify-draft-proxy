import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadOperationRegistry } from '../../scripts/conformance-scenario-registry.js';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const operationRegistryEntries = loadOperationRegistry(repoRoot);

function listOperationRegistryEntries() {
  return operationRegistryEntries.map((entry) => ({
    ...entry,
    matchNames: [...entry.matchNames],
    runtimeTests: [...entry.runtimeTests],
  }));
}

function listImplementedOperationRegistryEntries() {
  return listOperationRegistryEntries().filter((entry) => entry.implemented);
}

function listRuntimeTestedOperationRegistryEntries() {
  return listOperationRegistryEntries().filter((entry) => entry.runtimeTests.length > 0);
}

describe('operation registry', () => {
  it('keeps implemented capability names unique', () => {
    const implementedNames = listImplementedOperationRegistryEntries().map((entry) => entry.name);
    expect(new Set(implementedNames).size).toBe(implementedNames.length);
  });

  it('treats every runtime-tested operation as implemented', () => {
    // `implemented` spans the full locally-handled surface, so it is a superset of the
    // runtime-tested (uniform table-dispatch) operations.
    for (const entry of listRuntimeTestedOperationRegistryEntries()) {
      expect(entry.implemented, `${entry.name} declares runtime tests so it must be implemented`).toBe(true);
    }
  });

  it('requires runtime-tested operations to declare runtime tests without conformance metadata', () => {
    for (const entry of listRuntimeTestedOperationRegistryEntries()) {
      expect(entry.runtimeTests.length).toBeGreaterThan(0);
      expect('conformance' in entry).toBe(false);
    }
  });

  it('keeps runtime test references executable on disk', () => {
    for (const entry of listRuntimeTestedOperationRegistryEntries()) {
      for (const runtimeTest of entry.runtimeTests) {
        expect(
          existsSync(resolve(repoRoot, runtimeTest)),
          `${entry.name} runtime test should exist: ${runtimeTest}`,
        ).toBe(true);
      }
    }
  });

  it('exposes both overlay-read and stage-locally implemented operations', () => {
    const executions = new Set(listOperationRegistryEntries().map((entry) => entry.execution));
    expect(executions.has('overlay-read')).toBe(true);
    expect(executions.has('stage-locally')).toBe(true);
  });

  it('loads the Rust operation registry as the source of truth', () => {
    expect(listOperationRegistryEntries().length).toBeGreaterThan(0);
    expect(listOperationRegistryEntries().some((entry) => entry.name === 'productCreate')).toBe(true);
  });
});
