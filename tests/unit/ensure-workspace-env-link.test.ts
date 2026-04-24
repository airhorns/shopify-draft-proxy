import { mkdirSync, mkdtempSync, readlinkSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { ensureWorkspaceEnvLink } from '../../scripts/ensure-workspace-env-link.js';

const exampleEnv = [
  'SHOPIFY_CONFORMANCE_STORE_DOMAIN=your-store.myshopify.com',
  'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN=https://your-store.myshopify.com',
  '',
].join('\n');

const realEnv = [
  'SHOPIFY_CONFORMANCE_STORE_DOMAIN=very-big-test-store.myshopify.com',
  'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN=https://very-big-test-store.myshopify.com',
  '',
].join('\n');

describe('ensureWorkspaceEnvLink', () => {
  let tempRoot: string | null = null;

  afterEach(() => {
    if (tempRoot) {
      rmSync(tempRoot, { force: true, recursive: true });
      tempRoot = null;
    }
  });

  function makeWorkspace(): { canonicalEnvPath: string; envExamplePath: string; envPath: string; repoRoot: string } {
    tempRoot = mkdtempSync(path.join(tmpdir(), 'shopify-draft-proxy-env-link-'));
    const repoRoot = path.join(tempRoot, 'workspace');
    const canonicalRoot = path.join(tempRoot, 'canonical');
    mkdirSync(repoRoot);
    mkdirSync(canonicalRoot);

    const envPath = path.join(repoRoot, '.env');
    const envExamplePath = path.join(repoRoot, '.env.example');
    const canonicalEnvPath = path.join(canonicalRoot, '.env');
    writeFileSync(envExamplePath, exampleEnv);

    return { canonicalEnvPath, envExamplePath, envPath, repoRoot };
  }

  it('links a missing workspace .env to the canonical env', () => {
    const paths = makeWorkspace();
    writeFileSync(paths.canonicalEnvPath, realEnv);

    const result = ensureWorkspaceEnvLink(paths);

    expect(result).toMatchObject({ ok: true, status: 'linked' });
    expect(path.resolve(path.dirname(paths.envPath), readlinkSync(paths.envPath))).toBe(paths.canonicalEnvPath);
  });

  it('replaces a workspace .env copied from .env.example', () => {
    const paths = makeWorkspace();
    writeFileSync(paths.canonicalEnvPath, realEnv);
    writeFileSync(paths.envPath, exampleEnv);

    const result = ensureWorkspaceEnvLink(paths);

    expect(result).toMatchObject({ ok: true, status: 'replaced-example-copy' });
    expect(path.resolve(path.dirname(paths.envPath), readlinkSync(paths.envPath))).toBe(paths.canonicalEnvPath);
  });

  it('fails loudly when only a stale example-copy .env is available', () => {
    const paths = makeWorkspace();
    writeFileSync(paths.envPath, exampleEnv);

    const result = ensureWorkspaceEnvLink(paths);

    expect(result).toMatchObject({ ok: false, status: 'stale-example-copy' });
    expect(result.message).toContain('canonical env');
  });

  it('preserves an existing non-example workspace .env', () => {
    const paths = makeWorkspace();
    const workspaceEnv = 'SHOPIFY_CONFORMANCE_STORE_DOMAIN=custom-store.myshopify.com\n';
    writeFileSync(paths.canonicalEnvPath, realEnv);
    writeFileSync(paths.envPath, workspaceEnv);

    const result = ensureWorkspaceEnvLink(paths);

    expect(result).toMatchObject({ ok: true, status: 'kept-existing-env' });
  });
});
