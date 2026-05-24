/* oxlint-disable no-console -- CLI smoke runner reports selected Ruby execution path. */
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const repoRoot = resolve(import.meta.dirname, '..');
const rubyDir = resolve(repoRoot, 'ruby');
const serverBin = resolve(repoRoot, 'target/debug/shopify-draft-proxy-server');

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

run('cargo', ['build', '--bin', 'shopify-draft-proxy-server']);

if (!existsSync(serverBin)) {
  console.error(`Expected built Rust server at ${serverBin}`);
  process.exit(1);
}

if (hasCommand('ruby')) {
  run('ruby', ['-Ilib:test', 'test/shopify_draft_proxy_smoke_test.rb'], {
    cwd: rubyDir,
    env: { ...process.env, SHOPIFY_DRAFT_PROXY_SERVER_BIN: serverBin },
  });
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
  '-u',
  `${uid}:${gid}`,
  '-v',
  `${repoRoot}:/work`,
  '-w',
  '/work/ruby',
  '-e',
  'SHOPIFY_DRAFT_PROXY_SERVER_BIN=/work/target/debug/shopify-draft-proxy-server',
  'ruby:3.3',
  'ruby',
  '-Ilib:test',
  'test/shopify_draft_proxy_smoke_test.rb',
]);
