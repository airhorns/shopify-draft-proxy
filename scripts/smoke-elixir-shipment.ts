import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const gleamRoot = resolve(repoRoot, 'gleam');
const elixirSmokeRoot = resolve(gleamRoot, 'elixir_smoke');

function hasCommand(command: string): boolean {
  const result = spawnSync(command, ['--version'], { stdio: 'ignore' });
  return result.status === 0;
}

function run(command: string, args: string[], cwd: string): void {
  execFileSync(command, args, { cwd, stdio: 'inherit' });
}

function dockerAvailable(): boolean {
  return hasCommand('docker');
}

function exportShipment(): void {
  if (hasCommand('escript')) {
    run('gleam', ['export', 'erlang-shipment'], gleamRoot);
    return;
  }

  if (!dockerAvailable()) {
    throw new Error('Cannot export Erlang shipment: host lacks escript and docker is unavailable');
  }

  run(
    'docker',
    [
      'run',
      '--rm',
      '--user',
      `${process.getuid?.() ?? 1000}:${process.getgid?.() ?? 1000}`,
      '-v',
      `${repoRoot}:/workspace`,
      '-w',
      '/workspace/gleam',
      'ghcr.io/gleam-lang/gleam:v1.16.0-erlang-alpine',
      'gleam',
      'export',
      'erlang-shipment',
    ],
    repoRoot,
  );
}

function runMixSmoke(): void {
  if (hasCommand('mix')) {
    run('mix', ['test'], elixirSmokeRoot);
    return;
  }

  if (!dockerAvailable()) {
    throw new Error('Cannot run Elixir smoke: host lacks mix and docker is unavailable');
  }

  run(
    'docker',
    [
      'run',
      '--rm',
      '--user',
      `${process.getuid?.() ?? 1000}:${process.getgid?.() ?? 1000}`,
      '-e',
      'HOME=/tmp',
      '-v',
      `${repoRoot}:/workspace`,
      '-w',
      '/workspace/gleam/elixir_smoke',
      'elixir:1.18-alpine',
      'mix',
      'test',
    ],
    repoRoot,
  );
}

exportShipment();

const shipmentDir = resolve(gleamRoot, 'build/erlang-shipment');
if (!existsSync(shipmentDir)) {
  throw new Error(`Erlang shipment was not produced at ${shipmentDir}`);
}

runMixSmoke();
