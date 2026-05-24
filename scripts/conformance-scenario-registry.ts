import { execFileSync } from 'node:child_process';
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
} from './support/json-schemas.js';

export const defaultRepoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

const paritySpecDirectory = path.join('config', 'parity-specs');
const overrideConfigPath = path.join('config', 'conformance-scenario-overrides.json');
const registryCache = new Map<string, OperationRegistryEntry[]>();

export type ConformanceScenario = {
  id: string;
  operationNames: string[];
  status: string;
  assertionKinds: string[];
  captureFiles: string[];
  runtimeTestFiles: string[];
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
  strictComparisonScenarioIds: string[];
  runtimeFixtureScenarioIds: string[];
  captureOnlyScenarioIds: string[];
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

  function listJsonFiles(directory: string): string[] {
    return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        return listJsonFiles(entryPath);
      }

      return entry.isFile() && entry.name.endsWith('.json') ? [entryPath] : [];
    });
  }

  return listJsonFiles(absoluteDirectory)
    .map((absolutePath) => path.relative(absoluteDirectory, absolutePath))
    .sort((left, right) => left.localeCompare(right))
    .map((relativePath) => path.join(paritySpecDirectory, relativePath));
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
      runtimeTestFiles: stringArray(paritySpec.runtimeTestFiles),
      paritySpecPath,
      ...(notes ? { notes } : {}),
    };
  });
}

export function loadOperationRegistry(repoRoot = defaultRepoRoot): OperationRegistryEntry[] {
  const cacheKey = path.resolve(repoRoot);
  const cached = registryCache.get(cacheKey);
  if (cached) {
    return cloneRegistryEntries(cached);
  }

  let output: string;
  try {
    output = execFileSync('cargo', ['run', '--quiet', '--bin', 'operation-registry-json'], {
      cwd: cacheKey,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (error) {
    const stderr = stderrFromExecError(error);
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Failed to export operation registry from Rust: ${stderr ?? message}`);
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(output) as unknown;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Rust operation registry export produced invalid JSON: ${message}`);
  }

  const registry = operationRegistrySchema.parse(parsed);
  registryCache.set(cacheKey, registry);
  return cloneRegistryEntries(registry);
}

function cloneRegistryEntries(registryEntries: OperationRegistryEntry[]): OperationRegistryEntry[] {
  return registryEntries.map((entry) => ({
    ...entry,
    matchNames: [...entry.matchNames],
    runtimeTests: [...entry.runtimeTests],
  }));
}

function stderrFromExecError(error: unknown): string | null {
  if (typeof error !== 'object' || error === null || !('stderr' in error)) {
    return null;
  }

  const stderr = (error as { stderr?: unknown }).stderr;
  if (Buffer.isBuffer(stderr)) {
    const text = stderr.toString('utf8').trim();
    return text.length > 0 ? text : null;
  }

  if (typeof stderr === 'string') {
    const text = stderr.trim();
    return text.length > 0 ? text : null;
  }

  return null;
}

function addScenarioForOperation(
  result: Map<string, ConformanceScenario[]>,
  operationName: string,
  scenario: ConformanceScenario,
): void {
  const scenariosForOperation = result.get(operationName) ?? [];
  if (!scenariosForOperation.includes(scenario)) {
    scenariosForOperation.push(scenario);
  }
  result.set(operationName, scenariosForOperation);
}

function runtimeTestCoveredOperationNames(
  scenario: ConformanceScenario,
  registryEntries: OperationRegistryEntry[],
): string[] {
  const scenarioRuntimeTests = new Set(scenario.runtimeTestFiles);
  if (scenarioRuntimeTests.size === 0) {
    return [];
  }

  return registryEntries
    .filter((entry) => entry.runtimeTests.some((runtimeTestFile) => scenarioRuntimeTests.has(runtimeTestFile)))
    .map((entry) => entry.name);
}

export function groupScenariosByOperation(
  scenarios: ConformanceScenario[],
  registryEntries: OperationRegistryEntry[] = [],
): Map<string, ConformanceScenario[]> {
  const result = new Map<string, ConformanceScenario[]>();
  for (const scenario of scenarios) {
    for (const operationName of scenario.operationNames) {
      addScenarioForOperation(result, operationName, scenario);
    }
    for (const operationName of runtimeTestCoveredOperationNames(scenario, registryEntries)) {
      addScenarioForOperation(result, operationName, scenario);
    }
  }

  return result;
}

function readParitySpec(repoRoot: string, scenario: ConformanceScenario): ParitySpec {
  return parseJsonFileWithSchema(path.join(repoRoot, scenario.paritySpecPath), paritySpecSchema);
}

function isCaptureOnlyScenario(repoRoot: string, scenario: ConformanceScenario): boolean {
  if (scenario.status !== 'captured') {
    return false;
  }

  const paritySpec = readParitySpec(repoRoot, scenario);
  return paritySpec.comparisonMode === 'captured-fixture' && (paritySpec.runtimeTestFiles?.length ?? 0) === 0;
}

function isRuntimeFixtureScenario(repoRoot: string, scenario: ConformanceScenario): boolean {
  if (scenario.status !== 'captured') {
    return false;
  }

  const paritySpec = readParitySpec(repoRoot, scenario);
  return paritySpec.comparisonMode === 'captured-fixture' && (paritySpec.runtimeTestFiles?.length ?? 0) > 0;
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
  const capturedScenarios = scenarios.filter((scenario) => scenario.status === 'captured');
  const captureOnlyScenarios = capturedScenarios.filter((scenario) => isCaptureOnlyScenario(repoRoot, scenario));
  const runtimeFixtureScenarios = capturedScenarios.filter((scenario) => isRuntimeFixtureScenario(repoRoot, scenario));
  const strictComparisonScenarios = capturedScenarios.filter((scenario) => {
    return !captureOnlyScenarios.includes(scenario) && !runtimeFixtureScenarios.includes(scenario);
  });
  const implementedEntries = loadOperationRegistry(repoRoot).filter((entry) => entry.implemented);
  const scenariosByOperation = groupScenariosByOperation(scenarios, implementedEntries);
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
    capturedScenarioIds: capturedScenarios.map((scenario) => scenario.id),
    strictComparisonScenarioIds: strictComparisonScenarios.map((scenario) => scenario.id),
    runtimeFixtureScenarioIds: runtimeFixtureScenarios.map((scenario) => scenario.id),
    captureOnlyScenarioIds: captureOnlyScenarios.map((scenario) => scenario.id),
    plannedScenarioIds: scenarios.filter((scenario) => scenario.status === 'planned').map((scenario) => scenario.id),
    regrettableDivergences: listRegrettableDivergences(repoRoot, scenarios),
  };
}
