import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

interface PackEntry {
  filename: string;
}

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const tempRoot = mkdtempSync(join(tmpdir(), 'shopify-draft-proxy-js-package-'));
const packDir = join(tempRoot, 'pack');
const consumerDir = join(tempRoot, 'consumer');

function run(command: string, args: string[], cwd: string): string {
  return execFileSync(command, args, {
    cwd,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
  });
}

try {
  execFileSync('mkdir', ['-p', packDir, consumerDir]);

  const packOut = run('npm', ['pack', '--json', '--ignore-scripts', '--pack-destination', packDir], repoRoot);
  const [packEntry] = JSON.parse(packOut) as PackEntry[];
  if (!packEntry) {
    throw new Error('npm pack returned no tarball metadata');
  }

  const tarballPath = join(packDir, packEntry.filename);
  writeFileSync(join(consumerDir, 'package.json'), '{"type":"module","private":true}\n');
  run('npm', ['install', '--silent', tarballPath], consumerDir);

  const smokeFile = join(consumerDir, 'smoke.mjs');
  writeFileSync(
    smokeFile,
    `
import { createDraftProxy, DRAFT_PROXY_STATE_DUMP_SCHEMA } from 'shopify-draft-proxy';

const proxy = createDraftProxy({
  readMode: 'snapshot',
  port: 4000,
  shopifyAdminOrigin: 'https://shopify.com',
});

const health = await proxy.processRequest({ method: 'GET', path: '/__meta/health' });
if (health.status !== 200 || health.body?.ok !== true) {
  throw new Error('installed package health check failed: ' + JSON.stringify(health));
}

const dump = proxy.dumpState('2026-04-30T00:00:00.000Z');
if (dump.schema !== DRAFT_PROXY_STATE_DUMP_SCHEMA) {
  throw new Error('installed package dump schema mismatch: ' + JSON.stringify(dump));
}

const fresh = createDraftProxy({
  readMode: 'snapshot',
  port: 4000,
  shopifyAdminOrigin: 'https://shopify.com',
});
fresh.restoreState(dump);
console.log('installed JS package smoke ok');
`,
  );

  const result = run('node', [smokeFile], consumerDir);
  process.stdout.write(result);
} finally {
  rmSync(tempRoot, { force: true, recursive: true });
}
