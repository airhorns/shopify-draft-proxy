import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  classifyParityScenarioState,
  executeParityScenario,
  type ParitySpec,
  validateParityScenarioOperationNames,
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

  it('validates declared mutation operations against executed mutation roots', () => {
    const validation = validateParityScenarioOperationNames({
      scenario: {
        id: 'operation-name-validation-example',
        status: 'captured',
        operationNames: ['fileCreate', 'fileUpdate', 'files'],
      },
      paritySpec: {},
      executedOperations: [
        {
          type: 'mutation',
          name: 'CreateFile',
          rootFields: ['fileCreate'],
        },
        {
          type: 'query',
          name: 'ReadFiles',
          rootFields: ['files'],
        },
        {
          type: 'mutation',
          name: 'StageUpload',
          rootFields: ['stagedUploadsCreate'],
        },
      ],
    });

    expect(validation.declaredMutationOperationNames).toEqual(['fileCreate', 'fileUpdate']);
    expect(validation.actualMutationOperationNames).toEqual(['fileCreate', 'stagedUploadsCreate']);
    expect(validation.missingMutationOperationNames).toEqual(['fileUpdate']);
    expect(validation.unexpectedMutationOperationNames).toEqual(['stagedUploadsCreate']);
    expect(validation.errors).toEqual([
      'Scenario operation-name-validation-example declares mutation operation(s) fileUpdate in operationNames but did not execute them. Actual executed mutation operation(s): fileCreate, stagedUploadsCreate.',
      'Scenario operation-name-validation-example executed mutation operation(s) stagedUploadsCreate but does not declare them in operationNames. Declared mutation operation(s): fileCreate, fileUpdate.',
    ]);
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
      expect(
        result.operationNameValidation.errors,
        `scenario ${scenario.id} declared mutation operationNames that did not match executed mutation roots`,
      ).toEqual([]);
      expect(result.ok).toBe(true);
    },
  );
});
