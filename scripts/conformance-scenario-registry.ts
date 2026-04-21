import { existsSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  conformanceScenarioOverridesSchema,
  operationRegistrySchema,
  parseJsonFileWithSchema,
  paritySpecSchema,
  type ConformanceScenarioOverride,
  type OperationRegistryEntry,
  type ParitySpec,
} from '../src/json-schemas.js';

export const defaultRepoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

const paritySpecDirectory = path.join('config', 'parity-specs');
const overrideConfigPath = path.join('config', 'conformance-scenario-overrides.json');

export type ConformanceScenario = {
  id: string;
  operationNames: string[];
  status: string;
  assertionKinds: string[];
  captureFiles: string[];
  paritySpecPath: string;
  notes?: string;
};

export type ConformanceStatusDocument = {
  generatedAt: string;
  implementedOperations: Array<{
    name: string;
    type: string;
    execution: string;
    conformanceStatus: 'covered' | 'declared-gap';
    scenarioIds: string[];
    reason: string | null;
  }>;
  coveredOperationNames: string[];
  declaredGapOperationNames: string[];
  capturedScenarioIds: string[];
  plannedScenarioIds: string[];
  regrettableDivergences: Array<{
    scenarioId: string;
    paritySpecPath: string;
    expectedDifferenceIndex: number;
    path: string | null;
    reason: string | null;
    matcher: string | null;
    ignored: boolean;
  }>;
};

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

export function loadConformanceScenarioOverrides(repoRoot = defaultRepoRoot): Map<string, ConformanceScenarioOverride> {
  const absolutePath = path.join(repoRoot, overrideConfigPath);
  if (!existsSync(absolutePath)) {
    return new Map();
  }

  const parsed = parseJsonFileWithSchema(absolutePath, conformanceScenarioOverridesSchema);
  return new Map(Object.entries(parsed));
}

export function listConformanceParitySpecPaths(repoRoot = defaultRepoRoot): string[] {
  const absoluteDirectory = path.join(repoRoot, paritySpecDirectory);

  return readdirSync(absoluteDirectory)
    .filter((fileName) => fileName.endsWith('.json'))
    .sort((left, right) => left.localeCompare(right))
    .map((fileName) => path.join(paritySpecDirectory, fileName));
}

export function loadConformanceScenarios(repoRoot = defaultRepoRoot): ConformanceScenario[] {
  const overrides = loadConformanceScenarioOverrides(repoRoot);

  return listConformanceParitySpecPaths(repoRoot).map((paritySpecPath) => {
    const paritySpec = parseJsonFileWithSchema(path.join(repoRoot, paritySpecPath), paritySpecSchema);
    const scenarioId = typeof paritySpec.scenarioId === 'string' ? paritySpec.scenarioId : '';
    const override = overrides.get(scenarioId) ?? {};
    const notes = override.notes ?? paritySpec.notes;

    return {
      id: scenarioId,
      operationNames: override.operationNames ?? stringArray(paritySpec.operationNames),
      status: override.status ?? (typeof paritySpec.scenarioStatus === 'string' ? paritySpec.scenarioStatus : ''),
      assertionKinds: override.assertionKinds ?? stringArray(paritySpec.assertionKinds),
      captureFiles: override.captureFiles ?? stringArray(paritySpec.liveCaptureFiles),
      paritySpecPath,
      ...(notes ? { notes } : {}),
    };
  });
}

export function loadOperationRegistry(repoRoot = defaultRepoRoot): OperationRegistryEntry[] {
  return parseJsonFileWithSchema(path.join(repoRoot, 'config', 'operation-registry.json'), operationRegistrySchema);
}

export function groupScenariosByOperation(scenarios: ConformanceScenario[]): Map<string, ConformanceScenario[]> {
  const result = new Map<string, ConformanceScenario[]>();
  for (const scenario of scenarios) {
    for (const operationName of scenario.operationNames) {
      const scenariosForOperation = result.get(operationName) ?? [];
      scenariosForOperation.push(scenario);
      result.set(operationName, scenariosForOperation);
    }
  }

  return result;
}

function readParitySpec(repoRoot: string, scenario: ConformanceScenario): ParitySpec {
  return parseJsonFileWithSchema(path.join(repoRoot, scenario.paritySpecPath), paritySpecSchema);
}

function listRegrettableDivergences(repoRoot: string, scenarios: ConformanceScenario[]) {
  return scenarios.flatMap((scenario) => {
    if (scenario.status !== 'captured') {
      return [];
    }

    const paritySpec = readParitySpec(repoRoot, scenario);
    const expectedDifferences = Array.isArray(paritySpec.comparison?.expectedDifferences)
      ? paritySpec.comparison.expectedDifferences
      : [];

    return expectedDifferences.flatMap((difference, index) => {
      if (difference?.regrettable !== true) {
        return [];
      }

      return [
        {
          scenarioId: scenario.id,
          paritySpecPath: scenario.paritySpecPath,
          expectedDifferenceIndex: index,
          path: difference.path ?? null,
          reason: difference.reason ?? null,
          matcher: difference.matcher ?? null,
          ignored: difference.ignore === true,
        },
      ];
    });
  });
}

export function buildConformanceStatusDocument(repoRoot = defaultRepoRoot): ConformanceStatusDocument {
  const scenarios = loadConformanceScenarios(repoRoot);
  const scenariosByOperation = groupScenariosByOperation(scenarios);
  const implementedEntries = loadOperationRegistry(repoRoot).filter((entry) => entry.implemented);
  const coveredEntries = implementedEntries.filter((entry) => {
    return (scenariosByOperation.get(entry.name) ?? []).some((scenario) => scenario.status === 'captured');
  });
  const gapEntries = implementedEntries.filter((entry) => !coveredEntries.includes(entry));

  return {
    generatedAt: new Date().toISOString(),
    implementedOperations: implementedEntries.map((entry) => {
      const operationScenarios = scenariosByOperation.get(entry.name) ?? [];
      const isCovered = coveredEntries.includes(entry);

      return {
        name: entry.name,
        type: entry.type,
        execution: entry.execution,
        conformanceStatus: isCovered ? 'covered' : 'declared-gap',
        scenarioIds: operationScenarios.map((scenario) => scenario.id),
        reason: isCovered
          ? null
          : 'No captured conformance scenario has been promoted for this implemented operation yet.',
      };
    }),
    coveredOperationNames: coveredEntries.map((entry) => entry.name),
    declaredGapOperationNames: gapEntries.map((entry) => entry.name),
    capturedScenarioIds: scenarios.filter((scenario) => scenario.status === 'captured').map((scenario) => scenario.id),
    plannedScenarioIds: scenarios.filter((scenario) => scenario.status === 'planned').map((scenario) => scenario.id),
    regrettableDivergences: listRegrettableDivergences(repoRoot, scenarios),
  };
}
