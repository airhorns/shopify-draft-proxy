import { spawnSync } from 'node:child_process';

import { conformanceCaptureIndex } from './conformance-capture-index.js';

const protectedPaths = ['config/parity-specs', 'config/parity-requests', 'fixtures/conformance'];
const registeredProtectedEvidenceRemovals = new Set([
  'config/parity-specs/payments/customer-payment-method-credit-card-create-validation.json',
  'config/parity-specs/payments/customer-payment-method-local-staging.json',
  'config/parity-specs/payments/customer-payment-method-remote-create-validation.json',
  'config/parity-specs/payments/customer-payment-method-shop-pay-guards.json',
  'config/parity-specs/payments/payment-reminder-send-shape.json',
  'config/parity-specs/payments/payment-terms-create-on-order.json',
  'config/parity-specs/payments/payment-terms-update-missing-local-runtime.json',
  'config/parity-specs/payments/payment_terms_delete_owner_cascade.json',
  'fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-credit-card-create-validation.json',
  'fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-local-staging.json',
  'fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-remote-create-validation.json',
  'fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-shop-pay-guards.json',
  'fixtures/conformance/local-runtime/2026-04/payments/payment-terms-create-on-order.json',
  'fixtures/conformance/local-runtime/2026-04/payments/payment-terms-delete-owner-cascade.json',
  'fixtures/conformance/local-runtime/2026-05/payments/payment-reminder-send-shape.json',
]);

const result = spawnSync('git', ['diff', '--name-status', 'origin/main', '--', ...protectedPaths], {
  encoding: 'utf8',
});

if (result.error) {
  throw result.error;
}

if (result.status !== 0) {
  process.stderr.write(result.stderr);
  process.exit(result.status ?? 1);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
}

function fixtureOutputMatchesPath(output: string, path: string): boolean {
  const pattern = escapeRegExp(output)
    .replaceAll('<store>', '[^/]+')
    .replaceAll('<api-version>', '[^/]+')
    .replaceAll('<domain-folder>', '[^/]+');
  return new RegExp(`^${pattern}$`, 'u').test(path);
}

const registeredFixtureOutputs = conformanceCaptureIndex.flatMap((entry) => entry.fixtureOutputs);

const changed = result.stdout
  .split('\n')
  .map((line) => line.trim())
  .filter(Boolean)
  .map((line) => {
    const [status, path] = line.split(/\t/u);
    if (!status || !path) {
      throw new Error(`Unexpected git diff --name-status line: ${line}`);
    }
    return { status, path };
  });

const unregistered = changed.filter(
  ({ status, path }) =>
    !(status === 'D' && registeredProtectedEvidenceRemovals.has(path)) &&
    !registeredFixtureOutputs.some((output) => fixtureOutputMatchesPath(output, path)),
);

if (unregistered.length > 0) {
  process.stderr.write(
    'Protected parity specs, parity requests, or conformance fixtures changed without capture-index registration.\n',
  );
  for (const { status, path } of unregistered) process.stderr.write(`- ${status}\t${path}\n`);
  process.exit(1);
}

process.stdout.write('Protected parity evidence changes are registered in the capture index.\n');
