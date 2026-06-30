import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

import { conformanceCaptureIndex } from './conformance-capture-index.js';

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

const descriptorQueryPattern = /^(?:hand-synthesized|sha:|cassette-backed|recorded by scripts\/|local-runtime)/u;
const customerSpecDir = path.join(process.cwd(), 'config/parity-specs/customers');
const customerEvidenceViolations: string[] = [];

function readJson(filePath: string): unknown {
  return JSON.parse(readFileSync(filePath, 'utf8')) as unknown;
}

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
    const spec = readRecord(readJson(specPath));
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
      const fixture = readRecord(readJson(captureFile));
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

process.stdout.write('Protected parity evidence changes are registered in the capture index.\n');
