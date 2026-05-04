/* oxlint-disable no-console -- CLI launcher writes status output to stdio. */
/**
 * Live end-to-end smoke for the Elixir+Gleam path.
 *
 * Pairs with `gleam/elixir_smoke/test/live_hybrid_e2e_test.exs`. This
 * launcher is the canonical way to run that `:live`-tagged test:
 *
 *   1. Builds a fresh Erlang shipment (`gleam export erlang-shipment`)
 *      so the smoke project loads the current Gleam source.
 *   2. Refreshes the conformance access token via
 *      `scripts/shopify-conformance-auth.mts` so the test never has to
 *      think about credential plumbing.
 *   3. Spawns `mix test --only live test/live_hybrid_e2e_test.exs`
 *      with the `SHOPIFY_CONFORMANCE_*` env populated.
 *
 * Sibling: `scripts/e2e-product-create-commit-smoke.mts` exercises the
 * same flow against the JS-target proxy embedded in a Node Koa app —
 * keep both green when changing committable mutation behaviour.
 *
 * Run via:  `pnpm e2e:elixir-product-create-commit-smoke`
 */
import 'dotenv/config';

import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const repoRoot = resolve(import.meta.dirname, '..');

function hasCommand(command: string): boolean {
  const result = spawnSync('sh', ['-lc', `command -v ${command} >/dev/null 2>&1`], {
    cwd: repoRoot,
    stdio: 'ignore',
  });
  return result.status === 0;
}

function runOrExit(command: string, args: string[], extraEnv: Record<string, string> = {}): void {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: 'inherit',
    env: { ...process.env, ...extraEnv },
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

if (!hasCommand('mix') || !hasCommand('gleam')) {
  console.error(
    'elixir e2e smoke requires both `mix` (Elixir) and `gleam` on PATH. Install Elixir 1.18+ and Gleam, then re-run.',
  );
  process.exit(1);
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const accessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const authHeaderForLog = Object.keys(buildAdminAuthHeaders(accessToken)).join(', ');

console.log(`live target: ${adminOrigin} (api ${apiVersion}, store ${storeDomain})`);
console.log(`auth headers: ${authHeaderForLog}`);

console.log('building Erlang shipment for shopify_draft_proxy...');
runOrExit('sh', ['-lc', 'cd gleam && gleam export erlang-shipment']);

console.log('running mix test --only live...');
runOrExit('sh', ['-lc', 'cd gleam/elixir_smoke && mix test --only live test/live_hybrid_e2e_test.exs'], {
  SHOPIFY_CONFORMANCE_STORE_DOMAIN: storeDomain,
  SHOPIFY_CONFORMANCE_ADMIN_ORIGIN: adminOrigin,
  SHOPIFY_CONFORMANCE_API_VERSION: apiVersion,
  SHOPIFY_CONFORMANCE_ACCESS_TOKEN: accessToken,
});
