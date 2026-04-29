import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');
const gleamProjectRoot = resolve(repoRoot, 'gleam');
const compiledEntrypoint = resolve(
  gleamProjectRoot,
  'build/dev/javascript/shopify_draft_proxy/shopify_draft_proxy.mjs',
);

/**
 * Phase 0 interop smoke: ensures the Gleam port's JavaScript build artefacts
 * can be loaded as ESM by the Node consumers that the existing TypeScript
 * proxy already expects to run alongside. Real domain coverage starts in
 * Phase 2; this exists only to keep the JS interop boundary green from day
 * one of the Gleam port.
 */
describe('gleam JS interop', () => {
  beforeAll(() => {
    if (!existsSync(compiledEntrypoint)) {
      execFileSync('gleam', ['build', '--target', 'javascript'], {
        cwd: gleamProjectRoot,
        stdio: 'inherit',
      });
    }
  });

  it('loads the gleam-emitted ESM and reaches the phase-0 hello function', async () => {
    const mod = (await import(compiledEntrypoint)) as { hello: () => string };
    expect(typeof mod.hello).toBe('function');
    expect(mod.hello()).toBe('shopify_draft_proxy gleam port: phase 0');
  });
});
