import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const operationRegistryPath = path.join(repoRoot, 'config', 'operation-registry.json');
const scenarioRegistryPath = path.join(repoRoot, 'config', 'conformance-scenarios.json');
const worklistPath = path.join(repoRoot, 'docs', 'shopify-admin-worklist.md');
const reportPath = path.join(repoRoot, 'docs', 'generated', 'conformance-coverage.md');
const statusJsonPath = path.join(repoRoot, 'docs', 'generated', 'conformance-status.json');
const statusMarkdownPath = path.join(repoRoot, 'docs', 'generated', 'worklist-conformance-status.md');

const operationRegistry = JSON.parse(readFileSync(operationRegistryPath, 'utf8'));
const scenarioRegistry = JSON.parse(readFileSync(scenarioRegistryPath, 'utf8'));
const worklist = readFileSync(worklistPath, 'utf8');

const errors = [];

function assert(condition, message) {
  if (!condition) {
    errors.push(message);
  }
}

function relativeExists(relativePath) {
  return existsSync(path.join(repoRoot, relativePath));
}

const allowedExecution = new Set(['overlay-read', 'stage-locally', 'passthrough']);
const allowedDomain = new Set(['products', 'media', 'unknown']);
const allowedType = new Set(['query', 'mutation']);
const allowedConformanceStatus = new Set(['covered', 'declared-gap']);
const allowedScenarioStatus = new Set(['captured', 'planned']);
const operationNames = new Set();
const matchNames = new Map();
const scenarioIds = new Map();

for (const scenario of scenarioRegistry) {
  assert(typeof scenario.id === 'string' && scenario.id.length > 0, 'Every conformance scenario must have a non-empty id.');
  assert(!scenarioIds.has(scenario.id), `Duplicate conformance scenario id: ${scenario.id}`);
  scenarioIds.set(scenario.id, scenario);
  assert(allowedScenarioStatus.has(scenario.status), `Scenario ${scenario.id} has invalid status ${scenario.status}.`);
  assert(Array.isArray(scenario.operationNames) && scenario.operationNames.length > 0, `Scenario ${scenario.id} must declare operationNames.`);
  assert(Array.isArray(scenario.assertionKinds) && scenario.assertionKinds.length > 0, `Scenario ${scenario.id} must declare assertionKinds.`);
  assert(Array.isArray(scenario.captureFiles), `Scenario ${scenario.id} must declare captureFiles.`);
  assert(typeof scenario.paritySpecPath === 'string' && scenario.paritySpecPath.length > 0, `Scenario ${scenario.id} must declare paritySpecPath.`);
  assert(relativeExists(scenario.paritySpecPath), `Scenario ${scenario.id} references missing parity spec: ${scenario.paritySpecPath}`);
  if (scenario.status === 'captured') {
    assert(scenario.captureFiles.length > 0, `Captured scenario ${scenario.id} must reference at least one capture file.`);
    for (const captureFile of scenario.captureFiles) {
      assert(relativeExists(captureFile), `Scenario ${scenario.id} references missing capture file: ${captureFile}`);
    }
  }
}

for (const entry of operationRegistry) {
  assert(typeof entry.name === 'string' && entry.name.length > 0, 'Every operation registry entry must have a non-empty name.');
  assert(!operationNames.has(entry.name), `Duplicate operation registry entry: ${entry.name}`);
  operationNames.add(entry.name);
  assert(allowedType.has(entry.type), `Operation ${entry.name} has invalid type ${entry.type}.`);
  assert(allowedDomain.has(entry.domain), `Operation ${entry.name} has invalid domain ${entry.domain}.`);
  assert(allowedExecution.has(entry.execution), `Operation ${entry.name} has invalid execution ${entry.execution}.`);
  assert(Array.isArray(entry.matchNames) && entry.matchNames.length > 0, `Operation ${entry.name} must declare matchNames.`);
  for (const matchName of entry.matchNames) {
    const previous = matchNames.get(matchName);
    assert(!previous, `Match name ${matchName} is declared by both ${previous} and ${entry.name}.`);
    matchNames.set(matchName, entry.name);
  }

  if (!entry.implemented) {
    continue;
  }

  assert(Array.isArray(entry.runtimeTests) && entry.runtimeTests.length > 0, `Implemented operation ${entry.name} must declare runtime test files.`);
  for (const testPath of entry.runtimeTests) {
    assert(relativeExists(testPath), `Implemented operation ${entry.name} references missing runtime test file: ${testPath}`);
  }

  assert(entry.conformance && allowedConformanceStatus.has(entry.conformance.status), `Implemented operation ${entry.name} must declare a valid conformance status.`);
  assert(Array.isArray(entry.conformance.scenarioIds) && entry.conformance.scenarioIds.length > 0, `Implemented operation ${entry.name} must reference at least one conformance scenario.`);

  for (const scenarioId of entry.conformance.scenarioIds ?? []) {
    const scenario = scenarioIds.get(scenarioId);
    assert(!!scenario, `Operation ${entry.name} references missing conformance scenario ${scenarioId}.`);
    if (scenario) {
      assert(scenario.operationNames.includes(entry.name), `Scenario ${scenarioId} must list operation ${entry.name}.`);
      if (entry.conformance.status === 'covered') {
        assert(scenario.status === 'captured', `Covered operation ${entry.name} must reference captured scenario ${scenarioId}.`);
      }
    }
  }

  if (entry.conformance.status === 'declared-gap') {
    assert(typeof entry.conformance.reason === 'string' && entry.conformance.reason.length > 0, `Declared-gap operation ${entry.name} must include a reason.`);
  }
}

for (const scenario of scenarioRegistry) {
  for (const operationName of scenario.operationNames) {
    assert(operationNames.has(operationName), `Scenario ${scenario.id} references unknown operation ${operationName}.`);
  }
}

const implementedWorklistOperations = new Set(
  worklist
    .split('\n')
    .filter((line) => line.includes('[x]'))
    .flatMap((line) => Array.from(line.matchAll(/`([^`]+)`/g), (match) => match[1])),
);

for (const entry of operationRegistry.filter((candidate) => candidate.implemented)) {
  assert(implementedWorklistOperations.has(entry.name), `Implemented operation ${entry.name} must appear as [x] in docs/shopify-admin-worklist.md.`);
}

if (errors.length > 0) {
  console.error('Conformance coverage check failed:\n');
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

const implementedEntries = operationRegistry.filter((entry) => entry.implemented);
const coveredEntries = implementedEntries.filter((entry) => entry.conformance.status === 'covered');
const gapEntries = implementedEntries.filter((entry) => entry.conformance.status === 'declared-gap');
const capturedScenarios = scenarioRegistry.filter((scenario) => scenario.status === 'captured');
const plannedScenarios = scenarioRegistry.filter((scenario) => scenario.status === 'planned');
const statusJson = {
  generatedAt: new Date().toISOString(),
  implementedOperations: implementedEntries.map((entry) => ({
    name: entry.name,
    type: entry.type,
    execution: entry.execution,
    conformanceStatus: entry.conformance.status,
    scenarioIds: entry.conformance.scenarioIds,
    reason: entry.conformance.reason ?? null,
  })),
  coveredOperationNames: coveredEntries.map((entry) => entry.name),
  declaredGapOperationNames: gapEntries.map((entry) => entry.name),
  capturedScenarioIds: capturedScenarios.map((scenario) => scenario.id),
  plannedScenarioIds: plannedScenarios.map((scenario) => scenario.id),
};

const coverageReport = [
  '# Conformance Coverage Report',
  '',
  'Generated by `corepack pnpm conformance:check`.',
  '',
  `- Implemented operations: ${implementedEntries.length}`,
  `- Covered operations: ${coveredEntries.length}`,
  `- Declared gaps: ${gapEntries.length}`,
  `- Captured scenarios: ${capturedScenarios.length}`,
  `- Planned scenarios: ${plannedScenarios.length}`,
  '',
  '## Covered operations',
  '',
  ...coveredEntries.map((entry) => {
    const scenarioIdsList = (entry.conformance.scenarioIds ?? []).map((scenarioId) => `\`${scenarioId}\``).join(', ');
    return `- \`${entry.name}\` → ${scenarioIdsList}`;
  }),
  '',
  '## Declared gaps',
  '',
  ...gapEntries.map((entry) => {
    const scenarioIdsList = (entry.conformance.scenarioIds ?? []).map((scenarioId) => `\`${scenarioId}\``).join(', ');
    return `- \`${entry.name}\` → ${scenarioIdsList} — ${entry.conformance.reason}`;
  }),
  '',
  '## Captured scenarios',
  '',
  ...capturedScenarios.map((scenario) => `- \`${scenario.id}\` → \`${scenario.paritySpecPath}\` + ${scenario.captureFiles.map((file) => `\`${file}\``).join(', ')}`),
  '',
  '## Planned scenarios',
  '',
  ...plannedScenarios.map((scenario) => `- \`${scenario.id}\` → \`${scenario.paritySpecPath}\``),
  '',
];

const worklistStatusReport = [
  '# Generated Worklist Conformance Status',
  '',
  'This file is machine-generated by `corepack pnpm conformance:check`.',
  'Use it as the source of truth for which implemented root operations are structurally conformance-covered versus explicitly blocked.',
  '',
  '## Generated `[c]`-eligible operations',
  '',
  ...coveredEntries.map((entry) => `- [c] \`${entry.name}\``),
  '',
  '## Implemented operations with declared conformance gaps',
  '',
  ...gapEntries.map((entry) => `- [x] \`${entry.name}\` — ${entry.conformance.reason}`),
  '',
];

mkdirSync(path.dirname(reportPath), { recursive: true });
writeFileSync(reportPath, coverageReport.join('\n'));
writeFileSync(statusJsonPath, JSON.stringify(statusJson, null, 2) + '\n');
writeFileSync(statusMarkdownPath, worklistStatusReport.join('\n'));

console.log(`conformance coverage ok (${coveredEntries.length} covered / ${gapEntries.length} declared gaps)`);
console.log(`reports written to ${path.relative(repoRoot, reportPath)}, ${path.relative(repoRoot, statusJsonPath)}, and ${path.relative(repoRoot, statusMarkdownPath)}`);
