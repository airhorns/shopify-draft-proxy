/* oxlint-disable no-console -- aggregate capture runner reports delegated live capture status. */

import { spawnSync } from 'node:child_process';

const replacementCaptureIds = [
  'abandoned-checkout-empty-read',
  'draft-order-bulk-tag-case-preservation',
  'draft-order-complete-parity',
  'draft-order-complete-payment-terms-pending',
  'order-create-math-matrix',
  'order-edit-residual-calculated-edits',
  'order-cancel-error-messages',
  'order-delete-snapshot-staging',
  'return-approve-decline-state-preconditions',
  'return-reverse-logistics',
  'return-status-preconditions',
  'return-quantity-validation',
  'order-capture-validation',
  'order-payment-transaction-void',
  'order-create-mandate-payment-validation',
];

for (const captureId of replacementCaptureIds) {
  console.log(`Running replacement orders capture: ${captureId}`);
  const result = spawnSync('corepack', ['pnpm', 'conformance:capture', '--', '--run', captureId], {
    shell: process.platform === 'win32',
    stdio: 'inherit',
  });

  if (result.status !== 0) {
    process.exit(typeof result.status === 'number' ? result.status : 1);
  }
}
