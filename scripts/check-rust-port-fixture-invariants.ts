import { spawnSync } from 'node:child_process';

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

const changed = result.stdout
  .split('\n')
  .map((line) => line.trim())
  .filter(Boolean);

if (changed.length > 0) {
  process.stderr.write(
    'Rust port must not change checked-in parity specs, parity requests, or conformance fixtures.\n',
  );
  for (const path of changed) process.stderr.write(`- ${path}\n`);
  process.exit(1);
}

process.stdout.write('Parity specs, parity requests, and conformance fixtures match origin/main.\n');
