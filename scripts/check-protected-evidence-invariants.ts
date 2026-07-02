import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';

import { conformanceCaptureIndex } from './conformance-capture-index.js';
import { validateRecordedUpstreamCalls, type RecordedUpstreamCall } from './parity-cassette.js';

const protectedPaths = ['config/parity-specs', 'config/parity-requests', 'fixtures/conformance'];

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
    return {
      status,
      path: secondPath ?? firstPath,
    };
  });

const unregistered = changed.filter(
  ({ status, path: changedPath }) =>
    status !== 'D' &&
    existsSync(changedPath) &&
    !registeredFixtureOutputs.some((output) => fixtureOutputMatchesPath(output, changedPath)),
);

if (unregistered.length > 0) {
  process.stderr.write(
    'Protected parity specs, parity requests, or conformance fixtures changed without capture-index registration.\n',
  );
  for (const { status, path } of unregistered) process.stderr.write(`- ${status}\t${path}\n`);
  process.exit(1);
}

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

process.stdout.write('Protected parity evidence additions/modifications are registered in the capture index.\n');

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

for (const specPath of trackedFiles('config/parity-specs/markets').filter((path) => path.endsWith('.json'))) {
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
