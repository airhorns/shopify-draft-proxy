import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { conformanceCaptureIndex, retiredConformanceEvidencePaths } from './conformance-capture-index.js';
import { validateRecordedUpstreamCalls, type RecordedUpstreamCall } from './parity-cassette.js';

const protectedPaths = ['config/parity-specs', 'config/parity-requests', 'fixtures/conformance'];
const repoRoot = process.cwd();
const registeredProtectedEvidenceRemovals = new Set([
  'config/parity-specs/payments/customer-payment-method-credit-card-create-validation.json',
  'config/parity-specs/payments/customer-payment-method-local-staging.json',
  'config/parity-specs/payments/customer-payment-method-remote-create-validation.json',
  'config/parity-specs/payments/customer-payment-method-shop-pay-guards.json',
  'fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-credit-card-create-validation.json',
  'fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-local-staging.json',
  'fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-remote-create-validation.json',
  'fixtures/conformance/local-runtime/2026-04/payments/customer-payment-method-shop-pay-guards.json',
  'fixtures/conformance/local-runtime/2026-04/media/file-acknowledge-update-failed-local-runtime.json',
  'fixtures/conformance/local-runtime/2026-04/media/file-update-product-reference-local-runtime.json',
  'fixtures/conformance/local-runtime/2026-04/media/files-upload-local-runtime.json',
  'fixtures/conformance/local-runtime/2026-04/media/media-file-acknowledge-update-failed-semantics.json',
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

function fixtureOutputMatchesPath(output: string, candidatePath: string): boolean {
  if (
    candidatePath.startsWith('fixtures/conformance/local-runtime/') &&
    !output.startsWith('fixtures/conformance/local-runtime/')
  ) {
    return false;
  }

  const pattern = escapeRegExp(output)
    .replaceAll('<store>', '[^/]+')
    .replaceAll('<api-version>', '[^/]+')
    .replaceAll('<domain-folder>', '[^/]+');
  return new RegExp(`^${pattern}$`, 'u').test(candidatePath);
}

const registeredFixtureOutputs = [
  ...conformanceCaptureIndex.flatMap((entry) => entry.fixtureOutputs),
  ...retiredConformanceEvidencePaths,
];

type ChangedPath = {
  status: string;
  path: string;
};

const changed = result.stdout
  .split('\n')
  .map((line) => line.trim())
  .filter(Boolean)
  .map((line): ChangedPath => {
    const [status = '', firstPath = '', secondPath] = line.split('\t');
    const changedPath = secondPath ?? firstPath;
    if (!status || !changedPath) {
      throw new Error(`Unexpected git diff --name-status line: ${line}`);
    }
    return {
      status,
      path: changedPath,
    };
  })
  .filter((entry) => entry.path.length > 0);

const unregistered = changed.filter(({ status, path: changedPath }) => {
  if (status === 'D') {
    return (
      !registeredProtectedEvidenceRemovals.has(changedPath) &&
      !registeredFixtureOutputs.some((output) => fixtureOutputMatchesPath(output, changedPath))
    );
  }
  return (
    existsSync(changedPath) && !registeredFixtureOutputs.some((output) => fixtureOutputMatchesPath(output, changedPath))
  );
});

function readJson(relativePath: string): unknown {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), 'utf8')) as unknown;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((candidate): candidate is string => typeof candidate === 'string') : [];
}

function listJsonFiles(relativeDirectory: string): string[] {
  const absoluteDirectory = path.join(repoRoot, relativeDirectory);
  if (!existsSync(absoluteDirectory)) {
    return [];
  }

  return readdirSync(absoluteDirectory, { withFileTypes: true }).flatMap((entry) => {
    const relativePath = path.join(relativeDirectory, entry.name);
    if (entry.isDirectory()) {
      return listJsonFiles(relativePath);
    }

    return entry.isFile() && entry.name.endsWith('.json') ? [relativePath] : [];
  });
}

function trackedFiles(pathspec: string): string[] {
  const trackedResult = spawnSync('git', ['ls-files', '--', pathspec], { encoding: 'utf8' });
  if (trackedResult.error) throw trackedResult.error;
  if (trackedResult.status !== 0) {
    process.stderr.write(trackedResult.stderr);
    process.exit(trackedResult.status ?? 1);
  }
  return trackedResult.stdout
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);
}

function fixtureUpstreamErrors(fixturePath: string): string[] {
  const fixture = readJson(fixturePath);
  if (!isRecord(fixture) || !Array.isArray(fixture['upstreamCalls'])) {
    return [];
  }

  return validateRecordedUpstreamCalls(fixture['upstreamCalls'] as RecordedUpstreamCall[]).map(
    (error) => `${fixturePath}: ${error}`,
  );
}

function capturedParitySpecs(specDirectory: string): string[] {
  return listJsonFiles(specDirectory).filter((specPath) => {
    const spec = readJson(specPath);
    return (
      isRecord(spec) && spec['scenarioStatus'] === 'captured' && spec['comparisonMode'] === 'captured-vs-proxy-request'
    );
  });
}

function metafieldDefinitionsParityEvidenceErrors(): string[] {
  const errors: string[] = [];

  for (const specPath of capturedParitySpecs(path.join('config', 'parity-specs', 'metafield-definitions'))) {
    const spec = readJson(specPath);
    if (!isRecord(spec)) {
      continue;
    }

    for (const liveCaptureFile of stringArray(spec['liveCaptureFiles'])) {
      if (liveCaptureFile.startsWith('fixtures/conformance/local-runtime/')) {
        errors.push(`${specPath}: liveCaptureFiles must not use local-runtime evidence (${liveCaptureFile})`);
        continue;
      }

      if (existsSync(path.join(repoRoot, liveCaptureFile))) {
        errors.push(...fixtureUpstreamErrors(liveCaptureFile));
      }
    }
  }

  return errors;
}

function customerParityEvidenceErrors(): string[] {
  const errors: string[] = [];

  for (const specPath of capturedParitySpecs(path.join('config', 'parity-specs', 'customers'))) {
    const spec = readJson(specPath);
    if (!isRecord(spec)) {
      continue;
    }

    for (const liveCaptureFile of stringArray(spec['liveCaptureFiles'])) {
      if (liveCaptureFile.startsWith('fixtures/conformance/local-runtime/')) {
        errors.push(`${specPath}: liveCaptureFiles contains local-runtime fixture ${liveCaptureFile}`);
        continue;
      }

      if (liveCaptureFile.includes('/customers/') && existsSync(path.join(repoRoot, liveCaptureFile))) {
        errors.push(...fixtureUpstreamErrors(liveCaptureFile));
      }
    }
  }

  return errors;
}

function giftCardParityEvidenceErrors(): string[] {
  const errors: string[] = [];

  for (const specPath of listJsonFiles(path.join('config', 'parity-specs', 'gift-cards'))) {
    const spec = readJson(specPath);
    if (!isRecord(spec)) {
      continue;
    }

    const scenarioId = typeof spec['scenarioId'] === 'string' ? spec['scenarioId'] : specPath;
    const liveCaptureFiles = stringArray(spec['liveCaptureFiles']);

    if (spec['scenarioStatus'] === 'captured') {
      for (const captureFile of liveCaptureFiles) {
        if (captureFile.startsWith('fixtures/conformance/local-runtime/')) {
          errors.push(
            `${specPath}: captured gift-card scenario ${scenarioId} must not use local-runtime parity evidence (${captureFile}).`,
          );
        }
      }
    }

    for (const captureFile of liveCaptureFiles) {
      if (captureFile.startsWith('fixtures/conformance/') && existsSync(path.join(repoRoot, captureFile))) {
        errors.push(...fixtureUpstreamErrors(captureFile));
      }
    }
  }

  return errors;
}

function adminPlatformParityEvidenceErrors(): string[] {
  const errors: string[] = [];

  for (const specPath of capturedParitySpecs(path.join('config', 'parity-specs', 'admin-platform'))) {
    const spec = readJson(specPath);
    if (!isRecord(spec)) {
      continue;
    }

    const scenarioId = typeof spec['scenarioId'] === 'string' ? spec['scenarioId'] : specPath;

    for (const liveCaptureFile of stringArray(spec['liveCaptureFiles'])) {
      if (liveCaptureFile.startsWith('fixtures/conformance/local-runtime/')) {
        errors.push(`${specPath}: captured admin-platform scenario ${scenarioId} uses local-runtime fixture evidence`);
        continue;
      }

      if (!existsSync(path.join(repoRoot, liveCaptureFile))) {
        errors.push(
          `${specPath}: captured admin-platform scenario ${scenarioId} references missing fixture ${liveCaptureFile}`,
        );
        continue;
      }

      errors.push(...fixtureUpstreamErrors(liveCaptureFile));
    }
  }

  return errors;
}

function bulkOperationsParityEvidenceErrors(): string[] {
  const errors: string[] = [];

  for (const specPath of capturedParitySpecs(path.join('config', 'parity-specs', 'bulk-operations'))) {
    const spec = readJson(specPath);
    if (!isRecord(spec)) {
      continue;
    }

    const liveCaptureFiles = spec['liveCaptureFiles'];
    if (!Array.isArray(liveCaptureFiles)) {
      continue;
    }

    for (const liveCaptureFile of liveCaptureFiles) {
      if (typeof liveCaptureFile !== 'string') {
        errors.push(`${specPath}: liveCaptureFiles entry is not a string`);
        continue;
      }

      if (liveCaptureFile.startsWith('fixtures/conformance/local-runtime/')) {
        errors.push(`${specPath}: bulk-operations captured parity spec references local-runtime evidence`);
        continue;
      }

      if (!liveCaptureFile.includes('/bulk-operations/')) {
        continue;
      }

      if (!existsSync(path.join(repoRoot, liveCaptureFile))) {
        errors.push(`${specPath}: referenced bulk-operations fixture does not exist: ${liveCaptureFile}`);
        continue;
      }

      errors.push(...fixtureUpstreamErrors(liveCaptureFile));
    }
  }

  return errors;
}

function shippingFulfillmentParityEvidenceErrors(): string[] {
  const errors: string[] = [];

  for (const specPath of listJsonFiles(path.join('config', 'parity-specs', 'shipping-fulfillments'))) {
    const spec = readJson(specPath);
    if (!isRecord(spec)) {
      continue;
    }

    for (const liveCaptureFile of stringArray(spec['liveCaptureFiles'])) {
      if (liveCaptureFile.startsWith('fixtures/conformance/local-runtime/')) {
        errors.push(`${specPath}: liveCaptureFiles contains local-runtime fixture ${liveCaptureFile}`);
      }
    }
  }

  for (const fixturePath of listJsonFiles('fixtures/conformance').filter((filePath) =>
    filePath.split(path.sep).includes('shipping-fulfillments'),
  )) {
    if (fixturePath.startsWith('fixtures/conformance/local-runtime/')) {
      errors.push(`${fixturePath}: local-runtime shipping-fulfillments fixtures cannot be parity evidence`);
      continue;
    }

    errors.push(...fixtureUpstreamErrors(fixturePath));
  }

  return errors;
}

function marketsParityEvidenceErrors(): string[] {
  const errors: string[] = [];
  const referencedMarketsFixtures = new Set<string>();
  const checkedMarketsScenarioIds = new Set([
    'bundled-price-list-web-presence-create',
    'web-presence-create-case-insensitive-locale',
    'web-presence-create-french-default-locale',
    'web-presence-delete-primary-blocked',
    'web-presence-create-invalid-default-locale',
    'web-presence-root-urls-multi-locale',
    'market-localization-metafield-default-validation',
  ]);
  const checkedMarketsFixtureSuffixes = [
    '/markets/bundled-price-list-web-presence.json',
    '/markets/market-web-presence-lifecycle-parity.json',
    '/markets/market-localization-metafield-lifecycle-parity.json',
  ];

  for (const specPath of trackedFiles('config/parity-specs/markets').filter((filePath) => filePath.endsWith('.json'))) {
    const spec = readJson(specPath);
    if (!isRecord(spec)) {
      continue;
    }

    const scenarioId = spec['scenarioId'];
    const isCheckedScenario = typeof scenarioId === 'string' && checkedMarketsScenarioIds.has(scenarioId);
    const isCapturedParity =
      spec['scenarioStatus'] === 'captured' && spec['comparisonMode'] === 'captured-vs-proxy-request';

    for (const liveCaptureFile of stringArray(spec['liveCaptureFiles'])) {
      const isCheckedFixture = checkedMarketsFixtureSuffixes.some((suffix) => liveCaptureFile.endsWith(suffix));
      if (isCheckedScenario && isCapturedParity && liveCaptureFile.startsWith('fixtures/conformance/local-runtime/')) {
        errors.push(`${specPath}: captured markets parity spec points at local-runtime evidence`);
      }
      if (isCheckedScenario || isCheckedFixture) {
        referencedMarketsFixtures.add(liveCaptureFile);
      }
    }
  }

  for (const fixturePath of [...referencedMarketsFixtures].sort()) {
    if (existsSync(path.join(repoRoot, fixturePath))) {
      errors.push(...fixtureUpstreamErrors(fixturePath));
    }
  }

  return errors;
}

const metafieldDefinitionsErrors = metafieldDefinitionsParityEvidenceErrors();
const customerErrors = customerParityEvidenceErrors();
const giftCardErrors = giftCardParityEvidenceErrors();
const adminPlatformErrors = adminPlatformParityEvidenceErrors();
const bulkOperationsErrors = bulkOperationsParityEvidenceErrors();
const shippingFulfillmentErrors = shippingFulfillmentParityEvidenceErrors();
const marketsErrors = marketsParityEvidenceErrors();

if (unregistered.length > 0) {
  process.stderr.write(
    'Protected parity specs, parity requests, or conformance fixtures changed without capture-index registration.\n',
  );
  for (const { status, path } of unregistered) process.stderr.write(`- ${status}\t${path}\n`);
}

if (metafieldDefinitionsErrors.length > 0) {
  process.stderr.write(
    'metafield-definitions parity evidence must use live Shopify captures, not local-runtime or descriptor provenance.\n',
  );
  for (const error of metafieldDefinitionsErrors) process.stderr.write(`- ${error}\n`);
}

if (customerErrors.length > 0) {
  process.stderr.write('Customers parity evidence contains local-runtime captures or descriptor upstream queries.\n');
  for (const error of customerErrors) process.stderr.write(`- ${error}\n`);
}

if (giftCardErrors.length > 0) {
  process.stderr.write('Gift-card parity evidence contains disallowed local-runtime or descriptor cassette data.\n');
  for (const error of giftCardErrors) process.stderr.write(`- ${error}\n`);
}

if (adminPlatformErrors.length > 0) {
  process.stderr.write(
    'Admin-platform captured parity evidence contains synthetic/local-runtime provenance signals.\n',
  );
  for (const error of adminPlatformErrors) process.stderr.write(`- ${error}\n`);
}

if (bulkOperationsErrors.length > 0) {
  process.stderr.write(
    'Bulk-operations captured parity evidence contains local-runtime references or non-GraphQL upstream cassette queries.\n',
  );
  for (const error of bulkOperationsErrors) process.stderr.write(`- ${error}\n`);
}

if (shippingFulfillmentErrors.length > 0) {
  process.stderr.write(
    'shipping-fulfillments parity evidence contains local-runtime fixtures or descriptor upstream calls.\n',
  );
  for (const error of shippingFulfillmentErrors) process.stderr.write(`- ${error}\n`);
}

if (marketsErrors.length > 0) {
  process.stderr.write('Markets parity evidence contains local-runtime references or descriptor upstream cassettes.\n');
  for (const error of marketsErrors) process.stderr.write(`- ${error}\n`);
}

if (
  unregistered.length > 0 ||
  metafieldDefinitionsErrors.length > 0 ||
  customerErrors.length > 0 ||
  giftCardErrors.length > 0 ||
  adminPlatformErrors.length > 0 ||
  bulkOperationsErrors.length > 0 ||
  shippingFulfillmentErrors.length > 0 ||
  marketsErrors.length > 0
) {
  process.exit(1);
}

process.stdout.write('Protected parity evidence changes are registered in the capture index.\n');
process.stdout.write('metafield-definitions parity evidence uses live fixture paths and GraphQL upstream queries.\n');
process.stdout.write('Customers parity evidence uses live fixture paths and GraphQL upstream cassette queries.\n');
process.stdout.write('Gift-card parity evidence contains no local-runtime captures or descriptor cassette queries.\n');
process.stdout.write('Admin-platform captured parity evidence has no synthetic/local-runtime provenance signals.\n');
process.stdout.write('Bulk-operations captured parity evidence has no local-runtime or descriptor upstream signals.\n');
process.stdout.write(
  'shipping-fulfillments protected evidence has no local-runtime parity fixtures or descriptor upstream calls.\n',
);
process.stdout.write('Markets parity evidence uses GraphQL upstream cassette queries and live fixture paths.\n');
