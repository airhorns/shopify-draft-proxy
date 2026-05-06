/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import { spawnSync } from 'node:child_process';

type ScenarioRun = {
  scenarioId: string;
  apiVersion: string;
};

type ScriptRun = {
  scriptPath: string;
  apiVersion: string;
};

const scenarioRuns: ScenarioRun[] = [
  { scenarioId: 'delivery-settings-read', apiVersion: '2025-01' },
  { scenarioId: 'carrier-service-lifecycle', apiVersion: '2026-04' },
  { scenarioId: 'fulfillment-service-lifecycle', apiVersion: '2026-04' },
  { scenarioId: 'fulfillment-top-level-reads', apiVersion: '2026-04' },
];

const scriptRuns: ScriptRun[] = [
  {
    scriptPath: './scripts/capture-fulfillment-order-request-lifecycle-conformance.ts',
    apiVersion: '2026-04',
  },
];

for (const run of scenarioRuns) {
  console.log(`Recording ${run.scenarioId} against Admin API ${run.apiVersion}`);
  const result = spawnSync('tsx', ['./scripts/parity-record.mts', run.scenarioId], {
    env: {
      ...process.env,
      SHOPIFY_CONFORMANCE_API_VERSION: run.apiVersion,
    },
    stdio: 'inherit',
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

for (const run of scriptRuns) {
  console.log(`Running ${run.scriptPath} against Admin API ${run.apiVersion}`);
  const result = spawnSync('tsx', [run.scriptPath], {
    env: {
      ...process.env,
      SHOPIFY_CONFORMANCE_API_VERSION: run.apiVersion,
    },
    stdio: 'inherit',
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
