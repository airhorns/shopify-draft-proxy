/* oxlint-disable no-console -- CLI smoke runner reports selected Ruby execution path. */
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const repoRoot = resolve(import.meta.dirname, '..');
const rubyDir = resolve(repoRoot, 'ruby');
const nativeDir = resolve(rubyDir, 'native');
const nativeLibDir = resolve(rubyDir, 'lib/shopify_draft_proxy');
const nativeExtension = resolve(nativeLibDir, 'shopify_draft_proxy_native.so');

function hasCommand(command: string): boolean {
  const result = spawnSync('sh', ['-lc', `command -v ${command} >/dev/null 2>&1`], {
    cwd: repoRoot,
    stdio: 'ignore',
  });
  return result.status === 0;
}

function run(command: string, args: string[], options: { cwd?: string; env?: NodeJS.ProcessEnv } = {}): void {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    stdio: 'inherit',
    env: options.env ?? process.env,
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function buildAndTestWithLocalRuby(): void {
  run('cargo', [
    'build',
    '--manifest-path',
    resolve(nativeDir, 'Cargo.toml'),
    '--target-dir',
    resolve(repoRoot, 'target/ruby-native'),
  ]);
  run('sh', [
    '-lc',
    `mkdir -p ${JSON.stringify(nativeLibDir)} && cp ${JSON.stringify(resolve(repoRoot, 'target/ruby-native/debug/libshopify_draft_proxy_native.so'))} ${JSON.stringify(nativeExtension)}`,
  ]);
  run('ruby', ['-Ilib:test', 'test/shopify_draft_proxy_smoke_test.rb'], { cwd: rubyDir });
}

if (hasCommand('ruby')) {
  buildAndTestWithLocalRuby();
  process.exit(0);
}

if (!hasCommand('docker')) {
  console.error('ruby:smoke requires local Ruby or Docker.');
  process.exit(1);
}

const uid = process.getuid?.() ?? 1000;
const gid = process.getgid?.() ?? 1000;
run('docker', [
  'run',
  '--rm',
  '-v',
  `${repoRoot}:/work`,
  '-w',
  '/work',
  'ruby:3.3',
  'bash',
  '-lc',
  [
    'set -euo pipefail',
    'apt-get update >/dev/null',
    'apt-get install -y --no-install-recommends curl ca-certificates build-essential pkg-config libclang-dev >/dev/null',
    'curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal >/dev/null',
    'source "$HOME/.cargo/env"',
    'cargo build --manifest-path ruby/native/Cargo.toml --target-dir target/ruby-native',
    'mkdir -p ruby/lib/shopify_draft_proxy',
    'cp target/ruby-native/debug/libshopify_draft_proxy_native.so ruby/lib/shopify_draft_proxy/shopify_draft_proxy_native.so',
    'cd ruby',
    'ruby -Ilib:test test/shopify_draft_proxy_smoke_test.rb',
    'cd /work',
    `chown -R ${uid}:${gid} target/ruby-native ruby/lib/shopify_draft_proxy`,
  ].join(' && '),
]);
