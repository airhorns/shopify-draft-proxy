import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { validateComparisonContract, type ParitySpec } from '../../scripts/conformance-parity-lib.js';
import {
  buildConformanceStatusDocument,
  listConformanceParitySpecPaths,
  loadConformanceScenarioOverrides,
  loadConformanceScenarios,
} from '../../scripts/conformance-scenario-registry.js';

type OperationRegistryEntry = {
  name: string;
  implemented?: boolean;
  runtimeTests?: string[];
};

const repoRoot = resolve(import.meta.dirname, '../..');
const allowedScenarioStatuses = new Set(['captured', 'planned']);

function readJson<T>(relativePath: string): T {
  return JSON.parse(readFileSync(resolve(repoRoot, relativePath), 'utf8')) as T;
}

function scenarioCoverageOperationNames(scenarios: ReturnType<typeof loadConformanceScenarios>): string[] {
  return scenarios.flatMap((scenario) => [...scenario.operationNames, ...scenario.runtimeBackedOperationNames]);
}

describe('conformance scenario discovery', () => {
  const paritySpecPaths = listConformanceParitySpecPaths(repoRoot);
  const scenarioOverrides = loadConformanceScenarioOverrides(repoRoot);
  const scenarios = loadConformanceScenarios(repoRoot);

  it('uses parity specs as the scenario convention instead of generated or central manifests', () => {
    expect(existsSync(resolve(repoRoot, 'config/conformance-scenarios.json'))).toBe(false);
    expect(existsSync(resolve(repoRoot, 'docs/generated'))).toBe(false);

    expect(paritySpecPaths.length).toBeGreaterThan(0);
    expect(scenarios.map((scenario) => scenario.paritySpecPath)).toEqual(paritySpecPaths);
  });

  it('keeps discovered scenario ids unique and structurally complete', () => {
    const scenarioIds = scenarios.map((scenario) => scenario.id);
    expect(new Set(scenarioIds).size).toBe(scenarioIds.length);

    for (const scenario of scenarios) {
      expect(scenario.id.length, `${scenario.paritySpecPath} should declare scenarioId`).toBeGreaterThan(0);
      expect(scenario.operationNames.length, `${scenario.id} should declare operationNames`).toBeGreaterThan(0);
      expect(allowedScenarioStatuses.has(scenario.status), `${scenario.id} has invalid status`).toBe(true);
      expect(scenario.assertionKinds.length, `${scenario.id} should declare assertionKinds`).toBeGreaterThan(0);
      if (scenario.status === 'captured') {
        expect(scenario.captureFiles.length, `${scenario.id} should reference capture files`).toBeGreaterThan(0);
      }
    }

    for (const scenarioId of scenarioOverrides.keys()) {
      expect(scenarioIds).toContain(scenarioId);
    }
  });

  it.each(scenarios.map((scenario) => [scenario.id, scenario] as const))(
    'keeps parity spec file references present on disk for %s',
    (_scenarioId, scenario) => {
      const paritySpec = readJson<ParitySpec>(scenario.paritySpecPath);
      expect(paritySpec.scenarioId).toBe(scenario.id);
      expect(paritySpec.operationNames).toEqual(scenario.operationNames);
      expect(paritySpec.runtimeBackedOperationNames ?? []).toEqual(scenario.runtimeBackedOperationNames);
      expect(paritySpec.scenarioStatus).toBe(scenario.status);
      expect(paritySpec.assertionKinds).toEqual(scenario.assertionKinds);
      expect(paritySpec.liveCaptureFiles).toEqual(scenario.captureFiles);

      for (const captureFile of paritySpec.liveCaptureFiles ?? []) {
        expect(existsSync(resolve(repoRoot, captureFile)), `${captureFile} should exist`).toBe(true);
      }

      if (paritySpec.proxyRequest?.documentPath) {
        expect(existsSync(resolve(repoRoot, paritySpec.proxyRequest.documentPath))).toBe(true);
      }
      if (paritySpec.proxyRequest?.variablesPath) {
        expect(existsSync(resolve(repoRoot, paritySpec.proxyRequest.variablesPath))).toBe(true);
      }

      if (scenario.status === 'captured') {
        expect(validateComparisonContract(paritySpec.comparison), `${scenario.id} comparison contract`).toEqual([]);
      } else if (paritySpec.comparison) {
        expect(validateComparisonContract(paritySpec.comparison)).not.toEqual([]);
      }
    },
  );

  it.each(
    scenarios.flatMap((scenario) =>
      [...scenario.operationNames, ...scenario.runtimeBackedOperationNames].map(
        (operationName) => [`${scenario.id} -> ${operationName}`, operationName] as const,
      ),
    ),
  )('keeps discovered scenario operation reachable from the operation registry: %s', (_label, operationName) => {
    const registry = readJson<OperationRegistryEntry[]>('config/operation-registry.json');
    expect(registry.some((entry) => entry.name === operationName)).toBe(true);
  });

  it('keeps every implemented operation covered by at least one discovered scenario', () => {
    const registry = readJson<OperationRegistryEntry[]>('config/operation-registry.json');
    const scenarioOperationNames = new Set(scenarioCoverageOperationNames(scenarios));

    for (const entry of registry.filter((candidate) => candidate.implemented)) {
      expect(entry.runtimeTests?.length ?? 0).toBeGreaterThan(0);
      expect(scenarioOperationNames.has(entry.name), `${entry.name} should have a parity spec`).toBe(true);
    }
  });

  it('builds conformance status from discovered parity specs', () => {
    const status = buildConformanceStatusDocument(repoRoot);

    expect(status.implementedOperations.length).toBeGreaterThan(0);
    expect(status.capturedScenarioIds).toContain('product-create-live-parity');
    expect(status.capturedScenarioIds).toContain('product-duplicate-live-parity');
    expect(status.strictComparisonScenarioIds).toContain('product-create-live-parity');
    expect(status.strictComparisonScenarioIds).toContain('customer-address-lifecycle-parity');
    expect(status.captureOnlyScenarioIds).toHaveLength(0);
    expect(status.captureOnlyScenarioIds).not.toContain('product-create-live-parity');
    expect(status.implementedOperations.every((entry) => entry.scenarioIds.length > 0)).toBe(true);
  });
});
