import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

import { conformanceCaptureIndex } from './conformance-capture-index.js';
import { validateRecordedUpstreamCalls, type RecordedUpstreamCall } from './parity-cassette.js';

const protectedPaths = ['config/parity-specs', 'config/parity-requests', 'fixtures/conformance'];
const bulkOperationsSpecRoot = path.join('config', 'parity-specs', 'bulk-operations');

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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readJsonFile(filePath: string): unknown {
  return JSON.parse(readFileSync(filePath, 'utf8')) as unknown;
}

function collectJsonFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      return collectJsonFiles(entryPath);
    }
    return entry.isFile() && entry.name.endsWith('.json') ? [entryPath] : [];
  });
}

function shouldAuditBulkOperationsSpec(spec: Record<string, unknown>): boolean {
  return spec['scenarioStatus'] === 'captured' || spec['comparisonMode'] === 'captured-vs-proxy-request';
}

function auditBulkOperationsParityEvidence(): string[] {
  if (!existsSync(bulkOperationsSpecRoot)) {
    return [];
  }

  const errors: string[] = [];
  for (const specPath of collectJsonFiles(bulkOperationsSpecRoot)) {
    const spec = readJsonFile(specPath);
    if (!isRecord(spec) || !shouldAuditBulkOperationsSpec(spec)) {
      continue;
    }

    const liveCaptureFiles = spec['liveCaptureFiles'];
    if (!Array.isArray(liveCaptureFiles)) {
      continue;
    }

    for (const capturePath of liveCaptureFiles) {
      if (typeof capturePath !== 'string') {
        errors.push(`${specPath}: liveCaptureFiles entry is not a string`);
        continue;
      }
      if (capturePath.includes('fixtures/conformance/local-runtime/')) {
        errors.push(
          `${specPath}: bulk-operations captured parity spec references local-runtime evidence: ${capturePath}`,
        );
        continue;
      }
      if (!capturePath.includes('/bulk-operations/')) {
        continue;
      }
      if (!existsSync(capturePath)) {
        errors.push(`${specPath}: referenced bulk-operations fixture does not exist: ${capturePath}`);
        continue;
      }

      const fixture = readJsonFile(capturePath);
      if (!isRecord(fixture)) {
        errors.push(`${capturePath}: expected fixture JSON object`);
        continue;
      }
      const upstreamCalls = fixture['upstreamCalls'];
      if (upstreamCalls === undefined) {
        continue;
      }
      if (!Array.isArray(upstreamCalls)) {
        errors.push(`${capturePath}: upstreamCalls is not an array`);
        continue;
      }
      for (const error of validateRecordedUpstreamCalls(upstreamCalls as RecordedUpstreamCall[])) {
        errors.push(`${capturePath}: ${error}`);
      }
    }
  }

  return errors;
}

const bulkOperationsEvidenceErrors = auditBulkOperationsParityEvidence();
if (bulkOperationsEvidenceErrors.length > 0) {
  process.stderr.write(
    'Bulk-operations captured parity evidence contains local-runtime references or non-GraphQL upstream cassette queries.\n',
  );
  for (const error of bulkOperationsEvidenceErrors) process.stderr.write(`- ${error}\n`);
  process.exit(1);
}

process.stdout.write('Protected parity evidence changes are registered in the capture index.\n');
process.stdout.write('Bulk-operations captured parity evidence has no local-runtime or descriptor upstream signals.\n');
