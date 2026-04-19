import { readFileSync } from 'node:fs';
import path from 'node:path';

import { classifyParityScenarioState, summarizeParityResults, validateComparisonContract } from './conformance-parity-lib.mjs';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const scenarioRegistry = JSON.parse(readFileSync(path.join(repoRoot, 'config', 'conformance-scenarios.json'), 'utf8'));

const filterId = process.argv[2] ?? null;
const selectedScenarios = filterId
  ? scenarioRegistry.filter((scenario) => scenario.id === filterId)
  : scenarioRegistry;

if (selectedScenarios.length === 0) {
  console.error(filterId ? `Unknown conformance scenario id: ${filterId}` : 'No conformance scenarios found.');
  process.exit(1);
}

const results = [];
for (const scenario of selectedScenarios) {
  const paritySpecPath = path.join(repoRoot, scenario.paritySpecPath);
  const paritySpec = JSON.parse(readFileSync(paritySpecPath, 'utf8'));

  const state = classifyParityScenarioState(scenario, paritySpec);
  const comparisonContractErrors = validateComparisonContract(paritySpec?.comparison);
  const comparisonContract =
    comparisonContractErrors.length === 0
      ? {
          status: 'valid',
          mode: paritySpec.comparison.mode,
          allowedDifferences: paritySpec.comparison.allowedDifferences.length,
        }
      : paritySpec?.comparison
        ? {
            status: 'invalid',
            errors: comparisonContractErrors,
          }
        : {
            status: 'missing',
            errors: comparisonContractErrors,
          };

  results.push({
    scenarioId: scenario.id,
    operations: scenario.operationNames,
    scenarioStatus: scenario.status,
    paritySpecPath: scenario.paritySpecPath,
    state,
    comparisonContract,
    assertionKinds: scenario.assertionKinds,
    captureFiles: scenario.captureFiles,
  });
}

const summary = summarizeParityResults(results);

console.log(JSON.stringify({
  ok: true,
  total: results.length,
  ...summary,
  results,
}, null, 2));
