import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';

import { validateRecordedUpstreamCalls, type RecordedUpstreamCall } from './parity-cassette.js';
import {
  changedProtectedEvidencePaths,
  findProductsProvenanceFailures,
  findUnregisteredProtectedEvidenceChanges,
} from './protected-evidence-invariants.js';

const metafieldDefinitionsParitySpecDir = path.join('config', 'parity-specs', 'metafield-definitions');
const descriptorQueryPattern = /hand-synthesized|sha:|cassette-backed|recorded by scripts|local-runtime/iu;

const changed = changedProtectedEvidencePaths();
const failures = [...findUnregisteredProtectedEvidenceChanges(changed), ...findProductsProvenanceFailures()];

if (failures.length > 0) {
  process.stderr.write('Protected parity evidence invariant failures:\n');
  for (const failure of failures) process.stderr.write(`- ${failure.path}: ${failure.message}\n`);
  process.exit(1);
}

process.stdout.write('Protected parity evidence changes are registered and products provenance checks passed.\n');

function walkJsonFiles(directory: string): string[] {
  if (!existsSync(directory)) {
    return [];
  }

  return readdirSync(directory).flatMap((entry) => {
    const entryPath = path.join(directory, entry);
    if (statSync(entryPath).isDirectory()) {
      return walkJsonFiles(entryPath);
    }
    return entryPath.endsWith('.json') ? [entryPath] : [];
  });
}

function readJsonFile(filePath: string): unknown {
  return JSON.parse(readFileSync(filePath, 'utf8'));
}

function stringList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function findForbiddenMetafieldDefinitionsEvidence(): string[] {
  const failures: string[] = [];

  for (const specPath of walkJsonFiles(metafieldDefinitionsParitySpecDir)) {
    const spec = readJsonFile(specPath) as {
      comparisonMode?: unknown;
      liveCaptureFiles?: unknown;
      scenarioStatus?: unknown;
    };
    if (spec.scenarioStatus !== 'captured' || spec.comparisonMode !== 'captured-vs-proxy-request') {
      continue;
    }

    for (const liveCaptureFile of stringList(spec.liveCaptureFiles)) {
      if (liveCaptureFile.startsWith('fixtures/conformance/local-runtime/')) {
        failures.push(`${specPath}: liveCaptureFiles must not use local-runtime evidence (${liveCaptureFile})`);
        continue;
      }

      if (!existsSync(liveCaptureFile)) {
        continue;
      }

      const fixture = readJsonFile(liveCaptureFile) as { upstreamCalls?: unknown };
      const upstreamCalls = Array.isArray(fixture.upstreamCalls) ? fixture.upstreamCalls : [];
      upstreamCalls.forEach((call, index) => {
        const query = call && typeof call === 'object' ? (call as { query?: unknown }).query : undefined;
        if (typeof query === 'string' && descriptorQueryPattern.test(query)) {
          failures.push(`${liveCaptureFile}: upstreamCalls[${index}].query is descriptor provenance, not GraphQL`);
        }
      });
    }
  }

  return failures;
}

const metafieldDefinitionsFailures = findForbiddenMetafieldDefinitionsEvidence();
if (metafieldDefinitionsFailures.length > 0) {
  process.stderr.write(
    'metafield-definitions parity evidence must use live Shopify captures, not local-runtime or descriptor provenance.\n',
  );
  for (const failure of metafieldDefinitionsFailures) process.stderr.write(`- ${failure}\n`);
  process.exit(1);
}

function collectShippingFulfillmentFixtureFiles(): string[] {
  return walkJsonFiles('fixtures/conformance').filter((filePath) =>
    filePath.split(path.sep).includes('shipping-fulfillments'),
  );
}

function findForbiddenShippingFulfillmentEvidence(): string[] {
  const failures: string[] = [];
  const descriptorPattern =
    /^(?:sha:|hand-synthesized|cassette-backed|recorded by scripts\/)|hand-synthesized|local-runtime/u;

  for (const specPath of walkJsonFiles('config/parity-specs/shipping-fulfillments')) {
    const spec = readJsonFile(specPath) as { liveCaptureFiles?: unknown };
    const liveCaptureFiles = Array.isArray(spec.liveCaptureFiles) ? spec.liveCaptureFiles : [];
    for (const liveCaptureFile of liveCaptureFiles) {
      if (typeof liveCaptureFile === 'string' && liveCaptureFile.startsWith('fixtures/conformance/local-runtime/')) {
        failures.push(`${specPath}: liveCaptureFiles contains local-runtime fixture ${liveCaptureFile}`);
      }
    }
  }

  for (const fixturePath of collectShippingFulfillmentFixtureFiles()) {
    if (fixturePath.startsWith('fixtures/conformance/local-runtime/')) {
      failures.push(`${fixturePath}: local-runtime shipping-fulfillments fixtures cannot be parity evidence`);
      continue;
    }

    const fixture = readJsonFile(fixturePath) as { upstreamCalls?: unknown };
    const upstreamCalls = Array.isArray(fixture.upstreamCalls) ? fixture.upstreamCalls : [];
    upstreamCalls.forEach((call, index) => {
      const query = call && typeof call === 'object' ? (call as { query?: unknown }).query : undefined;
      if (typeof query === 'string' && descriptorPattern.test(query)) {
        failures.push(`${fixturePath}: upstreamCalls[${index}].query is a descriptor, not GraphQL`);
      }
    });
  }

  return failures;
}

const shippingFulfillmentEvidenceFailures = findForbiddenShippingFulfillmentEvidence();
if (shippingFulfillmentEvidenceFailures.length > 0) {
  process.stderr.write(
    'shipping-fulfillments parity evidence contains local-runtime fixtures or descriptor upstream calls.\n',
  );
  for (const failure of shippingFulfillmentEvidenceFailures) process.stderr.write(`- ${failure}\n`);
  process.exit(1);
}

const customerSpecDir = path.join(process.cwd(), 'config/parity-specs/customers');
const customerEvidenceViolations: string[] = [];

function readRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

if (existsSync(customerSpecDir)) {
  for (const fileName of readdirSync(customerSpecDir)
    .filter((file) => file.endsWith('.json'))
    .sort()) {
    const specPath = path.join(customerSpecDir, fileName);
    const spec = readRecord(readJsonFile(specPath));
    if (!spec) continue;
    const scenarioStatus = spec?.['scenarioStatus'];
    const comparisonMode = spec?.['comparisonMode'];
    if (scenarioStatus !== 'captured' || comparisonMode !== 'captured-vs-proxy-request') continue;

    const liveCaptureFiles = Array.isArray(spec['liveCaptureFiles']) ? spec['liveCaptureFiles'] : [];
    for (const captureFile of liveCaptureFiles) {
      if (typeof captureFile !== 'string') continue;
      if (captureFile.startsWith('fixtures/conformance/local-runtime/')) {
        customerEvidenceViolations.push(`${specPath}: liveCaptureFiles contains local-runtime fixture ${captureFile}`);
        continue;
      }
      if (!captureFile.includes('/customers/') || !existsSync(captureFile)) continue;
      const fixture = readRecord(readJsonFile(captureFile));
      const upstreamCalls = Array.isArray(fixture?.['upstreamCalls']) ? fixture['upstreamCalls'] : [];
      for (const [index, upstreamCall] of upstreamCalls.entries()) {
        const call = readRecord(upstreamCall);
        const query = call?.['query'];
        if (typeof query === 'string' && descriptorQueryPattern.test(query)) {
          customerEvidenceViolations.push(
            `${captureFile}: upstreamCalls[${index}].query is a provenance descriptor (${JSON.stringify(query)})`,
          );
        }
      }
    }
  }
}

if (customerEvidenceViolations.length > 0) {
  process.stderr.write('Customers parity evidence contains local-runtime captures or descriptor upstream queries.\n');
  for (const violation of customerEvidenceViolations) process.stderr.write(`- ${violation}\n`);
  process.exit(1);
}

process.stdout.write('Customers parity evidence uses live fixture paths and GraphQL upstream cassette queries.\n');

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

const marketsEvidenceErrors: string[] = [];
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
  const spec = readJsonFile(specPath) as Record<string, unknown>;
  const scenarioId = spec['scenarioId'];
  const isCheckedScenario = typeof scenarioId === 'string' && checkedMarketsScenarioIds.has(scenarioId);
  const liveCaptureFiles = Array.isArray(spec['liveCaptureFiles']) ? spec['liveCaptureFiles'] : [];
  const isCapturedParity =
    spec['scenarioStatus'] === 'captured' && spec['comparisonMode'] === 'captured-vs-proxy-request';
  for (const liveCaptureFile of liveCaptureFiles) {
    if (typeof liveCaptureFile !== 'string') continue;
    const isCheckedFixture = checkedMarketsFixtureSuffixes.some((suffix) => liveCaptureFile.endsWith(suffix));
    if (isCheckedScenario && isCapturedParity && liveCaptureFile.startsWith('fixtures/conformance/local-runtime/')) {
      marketsEvidenceErrors.push(`${specPath}: captured markets parity spec points at local-runtime evidence`);
    }
    if (isCheckedScenario || isCheckedFixture) referencedMarketsFixtures.add(liveCaptureFile);
  }
}

for (const fixturePath of [...referencedMarketsFixtures].sort()) {
  const fixture = readJsonFile(fixturePath) as { upstreamCalls?: unknown };
  if (!Array.isArray(fixture.upstreamCalls)) continue;
  const errors = validateRecordedUpstreamCalls(fixture.upstreamCalls as RecordedUpstreamCall[]);
  for (const error of errors) marketsEvidenceErrors.push(`${fixturePath}: ${error}`);
}

if (marketsEvidenceErrors.length > 0) {
  process.stderr.write('Markets parity evidence contains local-runtime references or descriptor upstream cassettes.\n');
  for (const error of marketsEvidenceErrors) process.stderr.write(`- ${error}\n`);
  process.exit(1);
}

process.stdout.write('Markets parity evidence uses GraphQL upstream cassette queries and live fixture paths.\n');
process.stdout.write(
  'shipping-fulfillments protected evidence has no local-runtime parity fixtures or descriptor upstream calls.\n',
);
