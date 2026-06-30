import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { conformanceCaptureIndex } from './conformance-capture-index.js';
import { validateRecordedUpstreamCalls, type RecordedUpstreamCall } from './parity-cassette.js';

const protectedPaths = ['config/parity-specs', 'config/parity-requests', 'fixtures/conformance'];
const repoRoot = process.cwd();

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

function fixtureOutputMatchesPath(output: string, candidatePath: string): boolean {
  const pattern = escapeRegExp(output)
    .replaceAll('<store>', '[^/]+')
    .replaceAll('<api-version>', '[^/]+')
    .replaceAll('<domain-folder>', '[^/]+');
  return new RegExp(`^${pattern}$`, 'u').test(candidatePath);
}

const registeredFixtureOutputs = conformanceCaptureIndex.flatMap((entry) => entry.fixtureOutputs);

const changed = result.stdout
  .split('\n')
  .map((line) => line.trim())
  .filter(Boolean);

const unregistered = changed.filter(
  (changedPath) => !registeredFixtureOutputs.some((output) => fixtureOutputMatchesPath(output, changedPath)),
);

function readJson(relativePath: string): unknown {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), 'utf8')) as unknown;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
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

function giftCardParityEvidenceErrors(): string[] {
  const errors: string[] = [];
  const giftCardSpecPaths = listJsonFiles(path.join('config', 'parity-specs', 'gift-cards'));

  for (const specPath of giftCardSpecPaths) {
    const spec = readJson(specPath);
    if (!isRecord(spec)) {
      continue;
    }

    const scenarioId = typeof spec['scenarioId'] === 'string' ? spec['scenarioId'] : specPath;
    const liveCaptureFiles = Array.isArray(spec['liveCaptureFiles'])
      ? spec['liveCaptureFiles'].filter((candidate): candidate is string => typeof candidate === 'string')
      : [];

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
      if (!captureFile.startsWith('fixtures/conformance/') || !existsSync(path.join(repoRoot, captureFile))) {
        continue;
      }

      const fixture = readJson(captureFile);
      if (!isRecord(fixture) || !Array.isArray(fixture['upstreamCalls'])) {
        continue;
      }

      const upstreamErrors = validateRecordedUpstreamCalls(fixture['upstreamCalls'] as RecordedUpstreamCall[]);
      for (const upstreamError of upstreamErrors) {
        errors.push(`${captureFile}: ${upstreamError}`);
      }
    }
  }

  return errors;
}

const evidenceErrors = giftCardParityEvidenceErrors();

if (unregistered.length > 0 || evidenceErrors.length > 0) {
  if (unregistered.length > 0) {
    process.stderr.write(
      'Protected parity specs, parity requests, or conformance fixtures changed without capture-index registration.\n',
    );
    for (const changedPath of unregistered) process.stderr.write(`- ${changedPath}\n`);
  }

  if (evidenceErrors.length > 0) {
    process.stderr.write('Gift-card parity evidence contains disallowed local-runtime or descriptor cassette data.\n');
    for (const error of evidenceErrors) process.stderr.write(`- ${error}\n`);
  }

  process.exit(1);
}

process.stdout.write('Protected parity evidence changes are registered in the capture index.\n');
process.stdout.write('Gift-card parity evidence contains no local-runtime captures or descriptor cassette queries.\n');
