import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { conformanceCaptureIndex } from './conformance-capture-index.js';
import { validateRecordedUpstreamCalls, type RecordedUpstreamCall } from './parity-cassette.js';

export const protectedPaths = ['config/parity-specs', 'config/parity-requests', 'fixtures/conformance'];

type ParitySpecEvidence = {
  scenarioStatus?: unknown;
  comparisonMode?: unknown;
  liveCaptureFiles?: unknown;
};

export type EvidenceInvariantFailure = {
  path: string;
  message: string;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readJsonFile(filePath: string): unknown {
  return JSON.parse(readFileSync(filePath, 'utf8')) as unknown;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
}

export function fixtureOutputMatchesPath(output: string, fixturePath: string): boolean {
  const pattern = escapeRegExp(output)
    .replaceAll('<store>', '[^/]+')
    .replaceAll('<api-version>', '[^/]+')
    .replaceAll('<domain-folder>', '[^/]+');
  return new RegExp(`^${pattern}$`, 'u').test(fixturePath);
}

export function changedProtectedEvidencePaths(baseRef = 'origin/main'): string[] {
  const diffResult = spawnSync(
    'git',
    ['diff', '--name-only', '--diff-filter=ACMRT', baseRef, '--', ...protectedPaths],
    {
      encoding: 'utf8',
    },
  );

  if (diffResult.error) {
    throw diffResult.error;
  }

  if (diffResult.status !== 0) {
    throw new Error(diffResult.stderr || `git diff exited with status ${diffResult.status ?? 'unknown'}`);
  }

  const untrackedResult = spawnSync('git', ['ls-files', '--others', '--exclude-standard', '--', ...protectedPaths], {
    encoding: 'utf8',
  });

  if (untrackedResult.error) {
    throw untrackedResult.error;
  }

  if (untrackedResult.status !== 0) {
    throw new Error(untrackedResult.stderr || `git ls-files exited with status ${untrackedResult.status ?? 'unknown'}`);
  }

  return [...new Set(`${diffResult.stdout}\n${untrackedResult.stdout}`.split('\n').map((line) => line.trim()))]
    .filter(Boolean)
    .sort((left, right) => left.localeCompare(right));
}

export function findUnregisteredProtectedEvidenceChanges(changed: string[]): EvidenceInvariantFailure[] {
  const registeredFixtureOutputs = conformanceCaptureIndex.flatMap((entry) => entry.fixtureOutputs);
  return changed
    .filter((changedPath) => registeredFixtureOutputs.every((output) => !fixtureOutputMatchesPath(output, changedPath)))
    .map((changedPath) => ({
      path: changedPath,
      message: 'changed protected evidence path is not declared by any capture-index fixtureOutputs entry',
    }));
}

function listJsonFiles(directory: string): string[] {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return listJsonFiles(entryPath);
    return entry.isFile() && entry.name.endsWith('.json') ? [entryPath] : [];
  });
}

function productsEvidenceSpec(paritySpecPath: string, paritySpec: ParitySpecEvidence): boolean {
  if (paritySpec.scenarioStatus !== 'captured') {
    return false;
  }

  return (
    paritySpecPath.split(path.sep).join('/').startsWith('config/parity-specs/products/') ||
    paritySpecPath.split(path.sep).join('/').startsWith('config/parity-specs/store-properties/')
  );
}

export function validateProductsParitySpecEvidence(
  paritySpecPath: string,
  paritySpec: ParitySpecEvidence,
  fixtureLoader: (fixturePath: string) => unknown,
): EvidenceInvariantFailure[] {
  if (!productsEvidenceSpec(paritySpecPath, paritySpec)) {
    return [];
  }

  const liveCaptureFiles = Array.isArray(paritySpec.liveCaptureFiles)
    ? paritySpec.liveCaptureFiles.filter((entry): entry is string => typeof entry === 'string')
    : [];
  const failures: EvidenceInvariantFailure[] = [];

  for (const fixturePath of liveCaptureFiles) {
    const normalizedFixturePath = fixturePath.split(path.sep).join('/');
    if (
      normalizedFixturePath.startsWith('fixtures/conformance/local-runtime/') &&
      normalizedFixturePath.includes('/products/')
    ) {
      failures.push({
        path: paritySpecPath,
        message: `products/store-properties parity spec references local-runtime fixture ${fixturePath}; remove the synthetic fixture/spec from parity evidence or replace it with live Shopify capture`,
      });
      continue;
    }

    let fixture: unknown;
    try {
      fixture = fixtureLoader(fixturePath);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      failures.push({
        path: paritySpecPath,
        message: `could not read live capture fixture ${fixturePath}: ${message}`,
      });
      continue;
    }

    if (!isRecord(fixture) || !Array.isArray(fixture['upstreamCalls'])) {
      continue;
    }

    const upstreamCallErrors = validateRecordedUpstreamCalls(fixture['upstreamCalls'] as RecordedUpstreamCall[]);
    for (const error of upstreamCallErrors) {
      failures.push({
        path: paritySpecPath,
        message: `${fixturePath}: ${error}`,
      });
    }
  }

  return failures;
}

export function findProductsProvenanceFailures(repoRoot = process.cwd()): EvidenceInvariantFailure[] {
  const paritySpecPaths = [
    ...listJsonFiles(path.join(repoRoot, 'config', 'parity-specs', 'products')),
    ...listJsonFiles(path.join(repoRoot, 'config', 'parity-specs', 'store-properties')),
  ];
  const failures: EvidenceInvariantFailure[] = [];

  for (const absoluteSpecPath of paritySpecPaths.sort((left, right) => left.localeCompare(right))) {
    const relativeSpecPath = path.relative(repoRoot, absoluteSpecPath);
    const paritySpec = readJsonFile(absoluteSpecPath) as ParitySpecEvidence;
    failures.push(
      ...validateProductsParitySpecEvidence(relativeSpecPath, paritySpec, (fixturePath) =>
        readJsonFile(path.join(repoRoot, fixturePath)),
      ),
    );
  }

  return failures;
}
