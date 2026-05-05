import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(here, '../../..');
const compiledEntrypoint = resolve(
  projectRoot,
  'build/dev/javascript/shopify_draft_proxy/shopify_draft_proxy/proxy/draft_proxy.mjs',
);

export function ensureGleamJavaScriptBuild(): void {
  if (existsSync(compiledEntrypoint)) {
    return;
  }
  execFileSync('gleam', ['build', '--target', 'javascript'], {
    cwd: projectRoot,
    stdio: 'inherit',
  });
}
