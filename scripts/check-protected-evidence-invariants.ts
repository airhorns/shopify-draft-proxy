import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { parse } from 'graphql';

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

type JsonObject = Record<string, unknown>;

function isJsonObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readJsonObject(filePath: string): JsonObject {
  const parsed = JSON.parse(readFileSync(filePath, 'utf8')) as unknown;
  if (!isJsonObject(parsed)) {
    throw new Error(`${filePath} must contain a JSON object`);
  }
  return parsed;
}

function walkJsonFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return walkJsonFiles(entryPath);
    return entry.isFile() && entry.name.endsWith('.json') ? [entryPath] : [];
  });
}

function isCapturedProxyParitySpec(spec: JsonObject): boolean {
  return spec['scenarioStatus'] === 'captured' && spec['comparisonMode'] === 'captured-vs-proxy-request';
}

function isGraphqlDocumentText(value: string): boolean {
  try {
    return parse(value).definitions.length > 0;
  } catch {
    return false;
  }
}

function adminPlatformEvidenceErrors(repoRoot = process.cwd()): string[] {
  const specRoot = path.join(repoRoot, 'config', 'parity-specs', 'admin-platform');
  const errors: string[] = [];
  if (!existsSync(specRoot)) return errors;

  const descriptorPattern = /^(?:sha:|hand-synthesized\b|cassette-backed\b|recorded by scripts\/|local-runtime\b)/iu;

  for (const specPath of walkJsonFiles(specRoot)) {
    const relativeSpecPath = path.relative(repoRoot, specPath).split(path.sep).join('/');
    const spec = readJsonObject(specPath);
    if (!isCapturedProxyParitySpec(spec)) continue;

    const scenarioId = typeof spec['scenarioId'] === 'string' ? spec['scenarioId'] : relativeSpecPath;
    const liveCaptureFiles = spec['liveCaptureFiles'];
    if (!Array.isArray(liveCaptureFiles)) continue;

    for (const fixturePathValue of liveCaptureFiles) {
      if (typeof fixturePathValue !== 'string') {
        errors.push(`${relativeSpecPath}: ${scenarioId} has a non-string liveCaptureFiles entry`);
        continue;
      }

      const normalizedFixturePath = fixturePathValue.split(path.sep).join('/');
      if (normalizedFixturePath.startsWith('fixtures/conformance/local-runtime/')) {
        errors.push(`${relativeSpecPath}: ${scenarioId} uses local-runtime fixture evidence: ${fixturePathValue}`);
        continue;
      }

      const absoluteFixturePath = path.resolve(repoRoot, fixturePathValue);
      if (!existsSync(absoluteFixturePath)) {
        errors.push(`${relativeSpecPath}: ${scenarioId} references missing fixture: ${fixturePathValue}`);
        continue;
      }

      const fixture = readJsonObject(absoluteFixturePath);
      const upstreamCalls = fixture['upstreamCalls'];
      if (!Array.isArray(upstreamCalls)) continue;

      for (let index = 0; index < upstreamCalls.length; index += 1) {
        const call = upstreamCalls[index];
        const query = isJsonObject(call) ? call['query'] : undefined;
        const prefix = `${fixturePathValue}: upstreamCalls[${index}].query`;

        if (typeof query !== 'string') {
          errors.push(`${prefix} is missing or is not a string`);
        } else if (descriptorPattern.test(query) || !isGraphqlDocumentText(query)) {
          errors.push(`${prefix} must be the exact GraphQL document sent to Shopify, not a descriptor`);
        }
      }
    }
  }

  return errors;
}

const adminPlatformErrors = adminPlatformEvidenceErrors();
if (adminPlatformErrors.length > 0) {
  process.stderr.write(
    'Admin-platform captured parity evidence contains synthetic/local-runtime provenance signals.\n',
  );
  for (const error of adminPlatformErrors) process.stderr.write(`- ${error}\n`);
  process.exit(1);
}

process.stdout.write('Protected parity evidence changes are registered in the capture index.\n');
