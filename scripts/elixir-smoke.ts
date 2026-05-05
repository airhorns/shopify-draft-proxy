import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const repoRoot = resolve(import.meta.dirname, '..');

function hasCommand(command: string): boolean {
  const result = spawnSync('sh', ['-lc', `command -v ${command} >/dev/null 2>&1`], {
    cwd: repoRoot,
    stdio: 'ignore',
  });
  return result.status === 0;
}

function run(command: string, args: string[]): never {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: 'inherit',
    env: process.env,
  });
  process.exit(result.status ?? 1);
}

if (hasCommand('escript') && hasCommand('mix')) {
  run('sh', ['-lc', 'gleam export erlang-shipment && cd elixir_smoke && mix test']);
}

if (!hasCommand('docker')) {
  process.stderr.write(
    'elixir:smoke requires escript+mix or Docker with ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine available.\n',
  );
  process.exit(1);
}

const image = 'ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine';
const uid = process.getuid?.() ?? 1000;
const gid = process.getgid?.() ?? 1000;
const cleanupTargets = ['build', 'elixir_smoke/_build', 'elixir_smoke/deps'].filter((path) =>
  existsSync(resolve(repoRoot, path)),
);
const chownTargets = ['build', 'elixir_smoke/_build', 'elixir_smoke/deps'];
const command =
  'apk add --no-cache elixir >/dev/null' +
  ' && gleam export erlang-shipment' +
  ' && cd elixir_smoke' +
  `; mix test; status=$?; chown -R ${uid}:${gid} ${chownTargets.join(' ')} 2>/dev/null || true; exit $status`;

if (cleanupTargets.length > 0) {
  spawnSync('sh', ['-lc', `chmod -R u+w ${cleanupTargets.join(' ')} 2>/dev/null || true`], {
    cwd: repoRoot,
    stdio: 'ignore',
  });
}

run('docker', ['run', '--rm', '-v', `${repoRoot}:/work`, '-w', '/work', image, 'sh', '-lc', command]);
