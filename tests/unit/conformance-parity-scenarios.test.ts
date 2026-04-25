import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  classifyParityScenarioState,
  executeParityScenario,
  type ParitySpec,
  validateParityScenarioInventoryEntry,
} from '../../scripts/conformance-parity-lib.js';
import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

const repoRoot = path.resolve(import.meta.dirname, '../..');

function readParitySpec(relativePath: string): ParitySpec {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), 'utf8')) as ParitySpec;
}

const discoveredScenarios = loadConformanceScenarios(repoRoot).map((scenario) => ({
  ...scenario,
  paritySpec: readParitySpec(scenario.paritySpecPath),
}));

const readyScenarios = discoveredScenarios.filter(
  (scenario) => classifyParityScenarioState(scenario, scenario.paritySpec) === 'ready-for-comparison',
);

describe('conformance parity scenarios (convention-driven suite)', () => {
  it('rejects checked-in captured scenarios without executable enforcement', () => {
    const errors = discoveredScenarios.flatMap((scenario) =>
      validateParityScenarioInventoryEntry(scenario, scenario.paritySpec),
    );

    expect(errors).toEqual([]);
  });

  it('discovers at least one ready-for-comparison scenario by convention', () => {
    expect(readyScenarios.length).toBeGreaterThan(0);
  });

  it.each(readyScenarios.map((scenario) => [scenario.id, scenario] as const))(
    'executes ready-for-comparison scenario %s against the local proxy harness',
    async (_id, scenario) => {
      const result = await executeParityScenario({
        repoRoot,
        scenario,
        paritySpec: scenario.paritySpec,
      });

      expect(result.primaryProxyStatus).toBe(200);
      expect(
        result.comparisons.filter((comparison) => !comparison.ok),
        `scenario ${scenario.id} had failing comparisons`,
      ).toEqual([]);
      expect(result.ok).toBe(true);
    },
  );
});
