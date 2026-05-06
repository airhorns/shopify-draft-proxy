/* oxlint-disable no-console -- CLI scripts intentionally write capture status to stdio. */
import { spawnSync } from 'node:child_process';

const scenarioIds = [
  'location-activate-missing-idempotency-validation',
  'location-deactivate-missing-idempotency-validation',
  'location-delete-active-location-validation',
  'location-add-blank-name-validation',
  'location-edit-unknown-id-validation',
];

for (const scenarioId of scenarioIds) {
  console.log(`Recording ${scenarioId} against Admin API 2026-04`);
  const result = spawnSync('tsx', ['./scripts/parity-record.mts', scenarioId], {
    env: {
      ...process.env,
      SHOPIFY_CONFORMANCE_API_VERSION: '2026-04',
    },
    stdio: 'inherit',
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
