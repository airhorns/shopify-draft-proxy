import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { conformanceCaptureIndex } from './conformance-capture-index.js';

const protectedPaths = ['config/parity-specs', 'config/parity-requests', 'fixtures/conformance'];
const metafieldDefinitionsParitySpecDir = path.join('config', 'parity-specs', 'metafield-definitions');
const descriptorQueryPattern = /hand-synthesized|sha:|cassette-backed|recorded by scripts|local-runtime/iu;

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
    const [status, ...pathParts] = line.split('\t');
    return { status, path: pathParts.at(-1) ?? '' };
  });

const unregistered = changed.filter(
  (change) =>
    change.status !== 'D' && !registeredFixtureOutputs.some((output) => fixtureOutputMatchesPath(output, change.path)),
);

if (unregistered.length > 0) {
  process.stderr.write(
    'Protected parity specs, parity requests, or conformance fixtures changed without capture-index registration.\n',
  );
  for (const change of unregistered) process.stderr.write(`- ${change.path}\n`);
  process.exit(1);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readJsonObject(filePath: string): Record<string, unknown> {
  const parsed = JSON.parse(readFileSync(filePath, 'utf8')) as unknown;
  if (!isRecord(parsed)) {
    throw new Error(`Expected ${filePath} to contain a JSON object.`);
  }
  return parsed;
}

function stringList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function listMetafieldDefinitionsParitySpecPaths(): string[] {
  if (!existsSync(metafieldDefinitionsParitySpecDir)) {
    return [];
  }

  return readdirSync(metafieldDefinitionsParitySpecDir)
    .filter((entry) => entry.endsWith('.json'))
    .sort((left, right) => left.localeCompare(right))
    .map((entry) => path.join(metafieldDefinitionsParitySpecDir, entry));
}

function auditMetafieldDefinitionsProvenance(): string[] {
  const failures: string[] = [];

  for (const specPath of listMetafieldDefinitionsParitySpecPaths()) {
    const spec = readJsonObject(specPath);
    if (spec['scenarioStatus'] !== 'captured' || spec['comparisonMode'] !== 'captured-vs-proxy-request') {
      continue;
    }

    for (const captureFile of stringList(spec['liveCaptureFiles'])) {
      if (captureFile.startsWith('fixtures/conformance/local-runtime/')) {
        failures.push(`${specPath}: liveCaptureFiles must not use local-runtime evidence (${captureFile})`);
        continue;
      }

      if (!existsSync(captureFile)) {
        continue;
      }

      const fixture = readJsonObject(captureFile);
      const upstreamCalls = Array.isArray(fixture['upstreamCalls']) ? fixture['upstreamCalls'] : [];
      upstreamCalls.forEach((call, index) => {
        if (!isRecord(call)) {
          return;
        }
        const query = call['query'];
        if (typeof query === 'string' && descriptorQueryPattern.test(query)) {
          failures.push(`${captureFile}: upstreamCalls[${index}].query is descriptor provenance, not GraphQL`);
        }
      });
    }
  }

  return failures;
}

const metafieldDefinitionsFailures = auditMetafieldDefinitionsProvenance();
if (metafieldDefinitionsFailures.length > 0) {
  process.stderr.write(
    'metafield-definitions parity evidence must use live Shopify captures, not local-runtime or descriptor provenance.\n',
  );
  for (const failure of metafieldDefinitionsFailures) process.stderr.write(`- ${failure}\n`);
  process.exit(1);
}

process.stdout.write('Protected parity evidence changes are registered in the capture index.\n');
