import { execFileSync } from 'node:child_process';

interface PackFile {
  path: string;
}

interface PackEntry {
  files: PackFile[];
  entryCount: number;
  size: number;
  unpackedSize: number;
}

function runPackDryRun(): PackEntry {
  const out = execFileSync('npm', ['pack', '--dry-run', '--json', '--ignore-scripts'], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const parsed = JSON.parse(out) as PackEntry[];
  const [entry] = parsed;
  if (!entry) {
    throw new Error('npm pack --dry-run returned no package metadata');
  }
  return entry;
}

function assertIncludes(paths: Set<string>, required: string): void {
  if (!paths.has(required)) {
    throw new Error(`package is missing required release file: ${required}`);
  }
}

function assertExcludes(paths: string[], forbidden: string): void {
  const match = paths.find((path) => path === forbidden || path.startsWith(`${forbidden}/`));
  if (match) {
    throw new Error(`package includes forbidden path ${match}; expected ${forbidden} to be excluded`);
  }
}

const entry = runPackDryRun();
const paths = entry.files.map((file) => file.path).sort();
const pathSet = new Set(paths);

for (const required of [
  'package.json',
  'README.md',
  'gleam/README.md',
  'gleam/js/dist/index.js',
  'gleam/js/dist/index.d.ts',
  'gleam/build/dev/javascript/shopify_draft_proxy/shopify_draft_proxy/proxy/draft_proxy.mjs',
  'gleam/build/dev/javascript/gleam_json/gleam/json.mjs',
  'scripts/conformance-capture-index.ts',
  'scripts/shopify-conformance-auth.mts',
  'config/operation-registry.json',
]) {
  assertIncludes(pathSet, required);
}

for (const forbidden of [
  '.agents',
  'src',
  'tests',
  'node_modules',
  'gleam/src',
  'gleam/test',
  'gleam/js/src',
  'gleam/elixir_smoke',
  'shopify-conformance-app',
]) {
  assertExcludes(paths, forbidden);
}

const tsRuntimeLeak = paths.find(
  (path) =>
    path.startsWith('src/') ||
    path.endsWith('/proxy-instance.ts') ||
    path.endsWith('/routes.ts') ||
    path.includes('/src/proxy/'),
);
if (tsRuntimeLeak) {
  throw new Error(`package includes obsolete TypeScript runtime file: ${tsRuntimeLeak}`);
}

process.stdout.write(
  `npm package dry-run ok: ${entry.entryCount} files, ${entry.size} bytes packed, ${entry.unpackedSize} bytes unpacked\n`,
);
