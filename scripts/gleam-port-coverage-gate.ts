import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { z } from 'zod';

import { listConformanceParitySpecPaths, loadConformanceScenarios } from './conformance-scenario-registry.js';
import {
  classifyParityScenarioState,
  type ParitySpec,
  validateParityScenarioInventoryEntry,
} from './conformance-parity-lib.js';
import { parseJsonFileWithSchema } from '../src/json-schemas.js';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const configPath = path.join(repoRoot, 'config', 'gleam-port-ci-gates.json');
const workflowPath = path.join(repoRoot, '.github', 'workflows', 'ci.yml');
const parityRunnerTestPath = path.join(repoRoot, 'gleam', 'test', 'parity_test.gleam');
const packageJsonPath = path.join(repoRoot, 'package.json');

const gateConfigSchema = z.strictObject({
  expectedGleamParityFailures: z.array(
    z.strictObject({
      specPath: z.string().min(1),
      reason: z.string().min(1),
      targets: z.array(z.enum(['erlang', 'javascript'])).optional(),
    }),
  ),
  requiredWorkflowCommands: z.array(z.string().min(1)),
  captureToolingChecks: z.array(z.string().min(1)),
});

type GateConfig = z.infer<typeof gateConfigSchema>;

type PackageJson = {
  scripts?: Record<string, string>;
};

function readText(relativePath: string): string {
  return readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readParitySpec(relativePath: string): ParitySpec {
  return JSON.parse(readText(relativePath)) as ParitySpec;
}

function sorted(values: Iterable<string>): string[] {
  return [...values].sort((left, right) => left.localeCompare(right));
}

function formatList(values: string[]): string {
  return values.length === 0 ? '(none)' : values.join(', ');
}

function expectedFailureKey(failure: GateConfig['expectedGleamParityFailures'][number]): string {
  const targets = failure.targets ? sorted(failure.targets).join('+') : '*';
  return `${failure.specPath} [${targets}]`;
}

function pushMissingPath(errors: string[], label: string, relativePath: string): void {
  if (!existsSync(path.join(repoRoot, relativePath))) {
    errors.push(`${label} does not exist: ${relativePath}`);
  }
}

function extractGleamParityRunnerSpecPaths(source: string): string[] {
  return sorted(
    [...source.matchAll(/check\(\s*(?:\n\s*)?"([^"]+)"/gu)].map((match) => {
      return match[1] ?? '';
    }),
  );
}

function checkParityInventory(config: GateConfig, errors: string[]): void {
  const paritySpecPaths = listConformanceParitySpecPaths(repoRoot);
  const paritySpecSet = new Set(paritySpecPaths);

  const expectedFailureKeys = config.expectedGleamParityFailures.map(expectedFailureKey);
  const uniqueExpectedFailureKeys = new Set(expectedFailureKeys);
  if (uniqueExpectedFailureKeys.size !== expectedFailureKeys.length) {
    const seen = new Set<string>();
    const duplicates = sorted(
      expectedFailureKeys.filter((key) => {
        if (seen.has(key)) {
          return true;
        }
        seen.add(key);
        return false;
      }),
    );
    errors.push(`Expected Gleam parity failure list has duplicate specs: ${formatList(duplicates)}.`);
  }

  for (const specPath of new Set(config.expectedGleamParityFailures.map((failure) => failure.specPath))) {
    pushMissingPath(errors, 'Expected Gleam parity failure spec', specPath);
    if (!paritySpecSet.has(specPath)) {
      errors.push(`Expected Gleam parity failure spec is not discovered by convention: ${specPath}`);
    }
  }

  checkGleamParityRunner(errors);

  for (const scenario of loadConformanceScenarios(repoRoot)) {
    const paritySpec = readParitySpec(scenario.paritySpecPath);
    const inventoryErrors = validateParityScenarioInventoryEntry(scenario, paritySpec);
    errors.push(...inventoryErrors);

    if (scenario.status !== 'captured') {
      errors.push(`Parity scenario is not captured/executable: ${scenario.id} (${scenario.paritySpecPath})`);
    }

    const state = classifyParityScenarioState(scenario, paritySpec);
    if (state !== 'ready-for-comparison') {
      errors.push(`Parity scenario is not ready for strict proxy comparison: ${scenario.id} (${state})`);
    }
  }
}

function checkGleamParityRunner(errors: string[]): void {
  const runnerSource = readFileSync(parityRunnerTestPath, 'utf8');
  const runnerSpecPaths = extractGleamParityRunnerSpecPaths(readFileSync(parityRunnerTestPath, 'utf8'));

  if (runnerSpecPaths.length > 0) {
    errors.push(
      `Gleam parity runner must discover specs dynamically instead of hardcoding an allowlist. Hardcoded specs: ${formatList(runnerSpecPaths)}.`,
    );
  }

  if (!runnerSource.includes('discover.discover(')) {
    errors.push('Gleam parity runner must discover parity specs through parity/discover.');
  }

  if (!runnerSource.includes('expectedGleamParityFailures')) {
    errors.push('Gleam parity runner must read expectedGleamParityFailures from config/gleam-port-ci-gates.json.');
  }
}

function checkWorkflowAndPackageScripts(config: GateConfig, errors: string[]): void {
  const workflow = readFileSync(workflowPath, 'utf8');
  for (const command of config.requiredWorkflowCommands) {
    if (!workflow.includes(command)) {
      errors.push(`CI workflow is missing required command: ${command}`);
    }
  }

  const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8')) as PackageJson;
  const scripts = packageJson.scripts ?? {};
  const requiredScripts = new Map<string, string[]>([
    ['gleam:registry:check', ['gleam/scripts/sync-operation-registry.sh']],
    ['gleam:port:coverage', ['./scripts/gleam-port-coverage-gate.ts']],
    ['conformance:capture:check', config.captureToolingChecks],
  ]);

  for (const [scriptName, requiredFragments] of requiredScripts) {
    const script = scripts[scriptName] ?? '';
    for (const requiredFragment of requiredFragments) {
      if (!script.includes(requiredFragment)) {
        errors.push(`package.json script ${scriptName} must include ${requiredFragment}.`);
      }
    }
  }

  for (const testPath of config.captureToolingChecks) {
    pushMissingPath(errors, 'TypeScript capture tooling check', testPath);
  }
}

function run(): void {
  const config = parseJsonFileWithSchema(configPath, gateConfigSchema);
  const errors: string[] = [];

  checkParityInventory(config, errors);
  checkWorkflowAndPackageScripts(config, errors);

  if (errors.length > 0) {
    process.stderr.write(`Gleam port CI gate failed:\n${errors.map((error) => `- ${error}`).join('\n')}\n`);
    process.exitCode = 1;
    return;
  }

  process.stdout.write(
    [
      'Gleam port CI gate passed:',
      `- ${listConformanceParitySpecPaths(repoRoot).length} parity specs discovered and all checked-in specs are strict executable comparisons`,
      `- ${config.expectedGleamParityFailures.length} expected Gleam parity failures are manifest-backed`,
      '- Gleam parity runner discovers the full parity corpus and does not hardcode a spec allowlist',
      '- CI workflow and TypeScript capture-tooling checks are wired',
    ].join('\n') + '\n',
  );
}

run();
