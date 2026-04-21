import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { parseJsonFileWithSchema, paritySpecSchema } from '../src/json-schemas.js';
import {
  classifyParityScenarioState,
  executeParityScenario,
  summarizeParityResults,
  validateComparisonContract,
} from './conformance-parity-lib.js';
import type { ParitySpec, Scenario } from './conformance-parity-lib.js';
import { loadConformanceScenarios } from './conformance-scenario-registry.js';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const scenarioRegistry = loadConformanceScenarios(repoRoot) as Array<
  Scenario & {
    operationNames: string[];
    assertionKinds: string[];
    captureFiles: string[];
    paritySpecPath: string;
  }
>;

const filterId = process.argv[2] ?? null;
const selectedScenarios = filterId ? scenarioRegistry.filter((scenario) => scenario.id === filterId) : scenarioRegistry;

if (selectedScenarios.length === 0) {
  // oxlint-disable-next-line no-console -- CLI error output is intentionally written to stderr.
  console.error(filterId ? `Unknown conformance scenario id: ${filterId}` : 'No conformance scenarios found.');
  process.exit(1);
}

const results = [];
for (const scenario of selectedScenarios) {
  const paritySpecPath = path.join(repoRoot, scenario.paritySpecPath);
  const paritySpec = parseJsonFileWithSchema(paritySpecPath, paritySpecSchema) as ParitySpec;

  const state = classifyParityScenarioState(scenario, paritySpec);
  const comparisonContractErrors = validateComparisonContract(paritySpec.comparison);
  const comparisonContract =
    comparisonContractErrors.length === 0 && paritySpec.comparison
      ? {
          status: 'valid',
          mode: paritySpec.comparison.mode,
          expectedDifferences: paritySpec.comparison.expectedDifferences?.length ?? 0,
        }
      : paritySpec.comparison
        ? {
            status: 'invalid',
            errors: comparisonContractErrors,
          }
        : {
            status: 'missing',
            errors: comparisonContractErrors,
          };
  const execution =
    state === 'ready-for-comparison' ? await executeParityScenario({ repoRoot, scenario, paritySpec }) : null;

  results.push({
    scenarioId: scenario.id,
    operations: scenario.operationNames,
    scenarioStatus: scenario.status,
    paritySpecPath: scenario.paritySpecPath,
    state,
    comparisonContract,
    assertionKinds: scenario.assertionKinds,
    captureFiles: scenario.captureFiles,
    ...(execution ? { execution } : {}),
  });
}

const summary = summarizeParityResults(results);
const ok = results.every((result) => !('execution' in result) || result.execution.ok);

// oxlint-disable-next-line no-console -- CLI parity result is intentionally written to stdout.
console.log(
  JSON.stringify(
    {
      ok,
      total: results.length,
      ...summary,
      executedComparisons: results.filter((result) => 'execution' in result).length,
      results,
    },
    null,
    2,
  ),
);

if (!ok) {
  process.exit(1);
}
