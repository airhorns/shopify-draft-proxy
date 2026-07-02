import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { parse } from 'graphql';

import { conformanceCaptureIndex } from './conformance-capture-index.js';

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
  ({ path: changedPath }) =>
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
  if (!existsSync(directory)) {
    return [];
  }

  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return walkJsonFiles(entryPath);
    return entry.isFile() && entry.name.endsWith('.json') ? [entryPath] : [];
  });
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
    const spec = readJsonObject(specPath);
    const liveCaptureFiles = spec['liveCaptureFiles'];
    const liveCaptureFileList = Array.isArray(liveCaptureFiles) ? liveCaptureFiles : [];
    for (const liveCaptureFile of liveCaptureFileList) {
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

    const fixture = readJsonObject(fixturePath);
    const upstreamCalls = fixture['upstreamCalls'];
    if (!Array.isArray(upstreamCalls)) continue;

    upstreamCalls.forEach((call, index) => {
      const query = isJsonObject(call) ? call['query'] : undefined;
      if (typeof query === 'string' && descriptorPattern.test(query)) {
        failures.push(`${fixturePath}: upstreamCalls[${index}].query is a descriptor, not GraphQL`);
      }
    });
  }

  return failures;
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

const shippingFulfillmentEvidenceFailures = findForbiddenShippingFulfillmentEvidence();
if (shippingFulfillmentEvidenceFailures.length > 0) {
  process.stderr.write(
    'shipping-fulfillments parity evidence contains local-runtime fixtures or descriptor upstream calls.\n',
  );
  for (const failure of shippingFulfillmentEvidenceFailures) process.stderr.write(`- ${failure}\n`);
  process.exit(1);
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
process.stdout.write(
  'shipping-fulfillments protected evidence has no local-runtime parity fixtures or descriptor upstream calls.\n',
);
process.stdout.write('Admin-platform captured parity evidence has no synthetic/local-runtime provenance signals.\n');
