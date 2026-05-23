import { spawnSync } from 'node:child_process';

const protectedPaths = ['config/parity-specs', 'config/parity-requests', 'fixtures/conformance'];
const rustRuntimePaths = ['Cargo.toml', 'Cargo.lock', 'src', 'js/src'];

function changedFiles(paths: string[]): string[] {
  const result = spawnSync('git', ['diff', '--name-only', 'origin/main', '--', ...paths], {
    encoding: 'utf8',
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    process.stderr.write(result.stderr);
    process.exit(result.status ?? 1);
  }

  return result.stdout
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);
}

const protectedChanged = changedFiles(protectedPaths);
const rustRuntimeChanged = changedFiles(rustRuntimePaths);

if (protectedChanged.length > 0 && rustRuntimeChanged.length > 0) {
  process.stderr.write(
    'Rust runtime/adapter changes must not also change checked-in parity specs, parity requests, or conformance fixtures.\n',
  );
  for (const path of protectedChanged) process.stderr.write(`- ${path}\n`);
  process.exit(1);
}

if (protectedChanged.length > 0) {
  process.stdout.write(
    'Protected parity evidence changed without Rust runtime/adapter changes; deferring validation to parity and conformance checks.\n',
  );
} else {
  process.stdout.write('Parity specs, parity requests, and conformance fixtures match origin/main.\n');
}
