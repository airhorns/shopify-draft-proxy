import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

import { conformanceCaptureIndex } from './conformance-capture-index.js';
import { validateRecordedUpstreamCalls, type RecordedUpstreamCall } from './parity-cassette.js';

const protectedPaths = ['config/parity-specs', 'config/parity-requests', 'fixtures/conformance'];

const result = spawnSync('git', ['diff', '--name-only', 'origin/main', '--', ...protectedPaths], {
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
  .filter(Boolean);

const unregistered = changed.filter(
  (path) => !registeredFixtureOutputs.some((output) => fixtureOutputMatchesPath(output, path)),
);

if (unregistered.length > 0) {
  process.stderr.write(
    'Protected parity specs, parity requests, or conformance fixtures changed without capture-index registration.\n',
  );
  for (const path of unregistered) process.stderr.write(`- ${path}\n`);
  process.exit(1);
}

process.stdout.write('Protected parity evidence changes are registered in the capture index.\n');

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

function readJsonFile(path: string): Record<string, unknown> {
  return JSON.parse(readFileSync(path, 'utf8')) as Record<string, unknown>;
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
  const spec = readJsonFile(specPath);
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
  const fixture = readJsonFile(fixturePath);
  if (!Array.isArray(fixture['upstreamCalls'])) continue;
  const errors = validateRecordedUpstreamCalls(fixture['upstreamCalls'] as RecordedUpstreamCall[]);
  for (const error of errors) marketsEvidenceErrors.push(`${fixturePath}: ${error}`);
}

if (marketsEvidenceErrors.length > 0) {
  process.stderr.write('Markets parity evidence contains local-runtime references or descriptor upstream cassettes.\n');
  for (const error of marketsEvidenceErrors) process.stderr.write(`- ${error}\n`);
  process.exit(1);
}

process.stdout.write('Markets parity evidence uses GraphQL upstream cassette queries and live fixture paths.\n');
