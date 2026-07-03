import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { validateRecordedUpstreamCalls, type RecordedUpstreamCall } from './parity-cassette.js';
import {
  changedProtectedEvidencePaths,
  findProductsProvenanceFailures,
  findUnregisteredProtectedEvidenceChanges,
} from './protected-evidence-invariants.js';

const repoRoot = process.cwd();
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

function originMainText(relativePath: string): string | null {
  const showResult = spawnSync('git', ['show', `origin/main:${relativePath}`], { encoding: 'utf8' });
  if (showResult.status !== 0) return null;
  return showResult.stdout;
}

function realShopifyConformanceFixturePath(relativePath: string): boolean {
  const parts = relativePath.split(path.sep).join('/').split('/');
  return parts[0] === 'fixtures' && parts[1] === 'conformance' && parts[2]?.endsWith('.myshopify.com') === true;
}

function nonShopifyStandInCaptureRefs(spec: unknown): string[] {
  if (!isRecord(spec)) return [];
  return stringArray(spec['liveCaptureFiles']).filter(
    (captureFile) => captureFile.startsWith('fixtures/conformance/') && !realShopifyConformanceFixturePath(captureFile),
  );
}

function newNonShopifyStandInEvidenceErrors(changedPaths: string[]): string[] {
  const errors: string[] = [];

  for (const changedPath of changedPaths) {
    if (
      changedPath.startsWith('fixtures/conformance/') &&
      !realShopifyConformanceFixturePath(changedPath) &&
      originMainText(changedPath) === null
    ) {
      errors.push(
        `${changedPath}: new conformance fixtures must be real Shopify captures; use runtime tests for proxy-only output`,
      );
    }

    if (!changedPath.startsWith('config/parity-specs/') || !changedPath.endsWith('.json') || !existsSync(changedPath)) {
      continue;
    }

    const currentRefs = new Set(nonShopifyStandInCaptureRefs(readJson(changedPath)));
    if (currentRefs.size === 0) continue;

    const baseText = originMainText(changedPath);
    const baseRefs = new Set<string>();
    if (baseText !== null) {
      try {
        for (const captureFile of nonShopifyStandInCaptureRefs(JSON.parse(baseText) as unknown)) {
          baseRefs.add(captureFile);
        }
      } catch {
        // If the base file is unparsable, be conservative and treat all current
        // non-Shopify capture references as newly introduced.
      }
    }

    for (const captureFile of currentRefs) {
      if (!baseRefs.has(captureFile)) {
        errors.push(
          `${changedPath}: new liveCaptureFiles entry is not a real Shopify capture path (${captureFile}); ` +
            'new parity/conformance evidence must be live Shopify capture-backed',
        );
      }
    }
  }

  return errors;
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

const changed = changedProtectedEvidencePaths();
const protectedEvidenceErrors = [
  ...findUnregisteredProtectedEvidenceChanges(changed),
  ...findProductsProvenanceFailures(),
];
const newNonShopifyStandInErrors = newNonShopifyStandInEvidenceErrors(changed);
const metafieldDefinitionsErrors = metafieldDefinitionsParityEvidenceErrors();
const customerErrors = customerParityEvidenceErrors();
const giftCardErrors = giftCardParityEvidenceErrors();
const adminPlatformErrors = adminPlatformParityEvidenceErrors();
const bulkOperationsErrors = bulkOperationsParityEvidenceErrors();
const shippingFulfillmentErrors = shippingFulfillmentParityEvidenceErrors();
const marketsErrors = marketsParityEvidenceErrors();

if (protectedEvidenceErrors.length > 0) {
  process.stderr.write('Protected parity evidence invariant failures.\n');
  for (const { path: failurePath, message } of protectedEvidenceErrors) {
    process.stderr.write(`- ${failurePath}: ${message}\n`);
  }
}

if (newNonShopifyStandInErrors.length > 0) {
  process.stderr.write(
    'New parity/conformance evidence must be real Shopify capture-backed, not proxy-generated or synthetic. Use runtime tests for proxy-only output.\n',
  );
  for (const error of newNonShopifyStandInErrors) process.stderr.write(`- ${error}\n`);
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
  protectedEvidenceErrors.length > 0 ||
  newNonShopifyStandInErrors.length > 0 ||
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

process.stdout.write('Protected parity evidence changes are registered and products provenance checks passed.\n');
process.stdout.write('No new proxy-generated or synthetic parity evidence was introduced.\n');
process.stdout.write('metafield-definitions parity evidence uses live fixture paths and GraphQL upstream queries.\n');
process.stdout.write('Customers parity evidence uses live fixture paths and GraphQL upstream cassette queries.\n');
process.stdout.write('Gift-card parity evidence contains no local-runtime captures or descriptor cassette queries.\n');
process.stdout.write('Admin-platform captured parity evidence has no synthetic/local-runtime provenance signals.\n');
process.stdout.write('Bulk-operations captured parity evidence has no local-runtime or descriptor upstream signals.\n');
process.stdout.write(
  'shipping-fulfillments protected evidence has no local-runtime parity fixtures or descriptor upstream calls.\n',
);
process.stdout.write('Markets parity evidence uses GraphQL upstream cassette queries and live fixture paths.\n');
