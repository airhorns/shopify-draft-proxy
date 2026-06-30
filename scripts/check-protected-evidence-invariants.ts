import { spawnSync } from 'node:child_process';

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

const changed = result.stdout
  .split('\n')
  .map((line) => line.trim())
  .filter(Boolean)
  .map((line) => {
    const [status = '', ...paths] = line.split('\t');
    return {
      status,
      path: paths.at(-1) ?? '',
    };
  })
  .filter((entry) => entry.path.length > 0);

const unregistered = changed.filter(
  ({ status, path }) =>
    status !== 'D' && !registeredFixtureOutputs.some((output) => fixtureOutputMatchesPath(output, path)),
);

if (unregistered.length > 0) {
  process.stderr.write(
    'Protected parity specs, parity requests, or conformance fixtures changed without capture-index registration.\n',
  );
  for (const { path } of unregistered) process.stderr.write(`- ${path}\n`);
  process.exit(1);
}

process.stdout.write('Protected parity evidence changes are registered in the capture index.\n');
