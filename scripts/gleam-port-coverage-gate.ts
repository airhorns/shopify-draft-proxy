import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { z } from 'zod';

import {
  listConformanceParitySpecPaths,
  loadConformanceScenarios,
  loadOperationRegistry,
} from './conformance-scenario-registry.js';
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
  expectedParitySpecCount: z.number().int().nonnegative(),
  gleamParityRunnerSpecPaths: z.array(z.string().min(1)),
  requiredWorkflowCommands: z.array(z.string().min(1)),
  captureToolingChecks: z.array(z.string().min(1)),
  domainGates: z.array(
    z.strictObject({
      domain: z.string().min(1),
      integrationTests: z.array(z.string().min(1)),
    }),
  ),
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

function pushMissingPath(errors: string[], label: string, relativePath: string): void {
  if (!existsSync(path.join(repoRoot, relativePath))) {
    errors.push(`${label} does not exist: ${relativePath}`);
  }
}

function compareSets(errors: string[], label: string, actual: Iterable<string>, expected: Iterable<string>): void {
  const actualSet = new Set(actual);
  const expectedSet = new Set(expected);
  const missing = sorted([...expectedSet].filter((value) => !actualSet.has(value)));
  const extra = sorted([...actualSet].filter((value) => !expectedSet.has(value)));

  if (missing.length > 0 || extra.length > 0) {
    errors.push(`${label} mismatch. Missing: ${formatList(missing)}. Extra: ${formatList(extra)}.`);
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
  if (paritySpecPaths.length !== config.expectedParitySpecCount) {
    errors.push(
      `Parity spec discovery count changed to ${paritySpecPaths.length}; expected ${config.expectedParitySpecCount}. Update config/gleam-port-ci-gates.json when intentionally adding or removing specs.`,
    );
  }

  const paritySpecSet = new Set(paritySpecPaths);
  for (const specPath of config.gleamParityRunnerSpecPaths) {
    pushMissingPath(errors, 'Configured Gleam parity runner spec', specPath);
    if (!paritySpecSet.has(specPath)) {
      errors.push(`Configured Gleam parity runner spec is not discovered by convention: ${specPath}`);
    }
  }

  const runnerSpecPaths = extractGleamParityRunnerSpecPaths(readFileSync(parityRunnerTestPath, 'utf8'));
  compareSets(errors, 'Gleam parity runner spec list', runnerSpecPaths, config.gleamParityRunnerSpecPaths);

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

function checkDomainGates(config: GateConfig, errors: string[]): void {
  const implementedRegistryEntries = loadOperationRegistry(repoRoot).filter((entry) => entry.implemented);
  const expectedDomains = sorted(new Set(implementedRegistryEntries.map((entry) => entry.domain)));
  const gatesByDomain = new Map(config.domainGates.map((gate) => [gate.domain, gate]));

  compareSets(errors, 'Gleam domain gate domains', gatesByDomain.keys(), expectedDomains);

  for (const gate of config.domainGates) {
    const domainParityDirectory = path.join('config', 'parity-specs', gate.domain);
    pushMissingPath(errors, `Parity spec directory for domain ${gate.domain}`, domainParityDirectory);

    const domainParitySpecs = listConformanceParitySpecPaths(repoRoot).filter((specPath) => {
      return specPath.startsWith(`${domainParityDirectory}/`);
    });
    if (domainParitySpecs.length === 0) {
      errors.push(`Domain ${gate.domain} has no discovered parity specs under ${domainParityDirectory}.`);
    }

    for (const integrationTest of gate.integrationTests) {
      pushMissingPath(errors, `Integration-test port mapping for domain ${gate.domain}`, integrationTest);
    }
  }

  for (const domain of expectedDomains) {
    const expectedIntegrationTests = sorted(
      new Set(
        implementedRegistryEntries
          .filter((entry) => entry.domain === domain)
          .flatMap((entry) => entry.runtimeTests)
          .filter((testPath) => testPath.startsWith('tests/integration/')),
      ),
    );
    const gate = gatesByDomain.get(domain);
    if (!gate) {
      continue;
    }
    compareSets(
      errors,
      `Integration-test port mappings for ${domain}`,
      gate.integrationTests,
      expectedIntegrationTests,
    );
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
  checkDomainGates(config, errors);
  checkWorkflowAndPackageScripts(config, errors);

  if (errors.length > 0) {
    process.stderr.write(`Gleam port CI gate failed:\n${errors.map((error) => `- ${error}`).join('\n')}\n`);
    process.exitCode = 1;
    return;
  }

  process.stdout.write(
    [
      'Gleam port CI gate passed:',
      `- ${config.expectedParitySpecCount} parity specs discovered and all checked-in specs are strict executable comparisons`,
      `- ${config.gleamParityRunnerSpecPaths.length} Gleam parity runner specs are manifest-backed`,
      `- ${config.domainGates.length} implemented registry domains have parity/spec and integration-test gates`,
      '- CI workflow and TypeScript capture-tooling checks are wired',
    ].join('\n') + '\n',
  );
}

run();
