import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';

import { conformanceCaptureIndex } from './conformance-capture-index.js';

const protectedPaths = ['config/parity-specs', 'config/parity-requests', 'fixtures/conformance'];
const retiredProtectedEvidencePaths = new Set([
  'config/parity-requests/media/fileAcknowledgeUpdateFailed-downstream-read.graphql',
  'config/parity-requests/media/fileAcknowledgeUpdateFailed-parity.graphql',
  'config/parity-requests/media/fileUpdate-product-reference-attach.graphql',
  'config/parity-requests/media/fileUpdate-product-reference-create.graphql',
  'config/parity-requests/media/fileUpdate-product-reference-files-read.graphql',
  'config/parity-requests/media/fileUpdate-product-reference-product-read.graphql',
  'config/parity-requests/media/files-upload-local-runtime-create.graphql',
  'config/parity-requests/media/files-upload-local-runtime-read.graphql',
  'config/parity-requests/media/files-upload-local-runtime-staged-upload.graphql',
  'config/parity-requests/media/media-file-acknowledge-update-failed-semantics-ack.graphql',
  'config/parity-requests/media/media-file-acknowledge-update-failed-semantics-create.graphql',
  'config/parity-requests/media/media-file-acknowledge-update-failed-semantics-read.graphql',
  'config/parity-specs/media/fileAcknowledgeUpdateFailed-local-staging.json',
  'config/parity-specs/media/fileUpdate-product-reference-local-staging.json',
  'config/parity-specs/media/files-upload-local-runtime.json',
  'config/parity-specs/media/media-file-acknowledge-update-failed-semantics.json',
  'fixtures/conformance/local-runtime/2026-04/media/file-acknowledge-update-failed-local-runtime.json',
  'fixtures/conformance/local-runtime/2026-04/media/file-update-product-reference-local-runtime.json',
  'fixtures/conformance/local-runtime/2026-04/media/files-upload-local-runtime.json',
  'fixtures/conformance/local-runtime/2026-04/media/media-file-acknowledge-update-failed-semantics.json',
]);

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
    !(status === 'D' && retiredProtectedEvidencePaths.has(changedPath) && !existsSync(changedPath)) &&
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

process.stdout.write('Protected parity evidence changes are registered in the capture index.\n');
process.stdout.write(
  'shipping-fulfillments protected evidence has no local-runtime parity fixtures or descriptor upstream calls.\n',
);
