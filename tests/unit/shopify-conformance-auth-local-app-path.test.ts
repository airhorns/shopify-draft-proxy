import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import * as path from 'node:path';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { resolveDefaultAppEnvPath, resolveDefaultAppRoot } from '../../scripts/shopify-conformance-auth.mjs';

async function createTempDir(prefix: string): Promise<string> {
  return await mkdtemp(path.join(tmpdir(), prefix));
}

afterEach(() => {
  vi.restoreAllMocks();
  delete process.env['SHOPIFY_CONFORMANCE_APP_ENV_PATH'];
  delete process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'];
});

describe('resolveDefaultAppRoot', () => {
  it('prefers a repo-local checked-in app directory when present', async () => {
    const repoRoot = await createTempDir('shopify-auth-repo-');
    process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] = 'hermes-conformance-products';
    const appRoot = path.join(repoRoot, 'shopify-conformance-app', 'hermes-conformance-products');
    await mkdir(appRoot, { recursive: true });

    expect(resolveDefaultAppRoot({ repoRoot })).toBe(appRoot);
  });

  it('falls back to the legacy /tmp app directory when no repo-local copy exists', async () => {
    const repoRoot = await createTempDir('shopify-auth-repo-');
    process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] = 'hermes-conformance-products';

    expect(resolveDefaultAppRoot({ repoRoot })).toBe('/tmp/shopify-conformance-app/hermes-conformance-products');
  });
});

describe('resolveDefaultAppEnvPath', () => {
  it('prefers an explicit SHOPIFY_CONFORMANCE_APP_ENV_PATH override', async () => {
    process.env['SHOPIFY_CONFORMANCE_APP_ENV_PATH'] = '/tmp/custom-shopify-app.env';

    expect(resolveDefaultAppEnvPath()).toBe('/tmp/custom-shopify-app.env');
  });

  it('prefers a repo-local checked-in app env path when present', async () => {
    const repoRoot = await createTempDir('shopify-auth-repo-');
    process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] = 'hermes-conformance-products';
    const envPath = path.join(repoRoot, 'shopify-conformance-app', 'hermes-conformance-products', '.env');
    await mkdir(path.dirname(envPath), { recursive: true });
    await writeFile(envPath, 'SHOPIFY_API_SECRET=repo-local-secret\n', 'utf8');

    expect(resolveDefaultAppEnvPath({ repoRoot })).toBe(envPath);
  });

  it('falls back to the legacy /tmp app env path when no repo-local app exists', async () => {
    const repoRoot = await createTempDir('shopify-auth-repo-');
    process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] = 'hermes-conformance-products';

    expect(resolveDefaultAppEnvPath({ repoRoot })).toBe(
      '/tmp/shopify-conformance-app/hermes-conformance-products/.env',
    );
  });

  it('falls back to the legacy /tmp app env path when the repo-local app has no env file', async () => {
    const repoRoot = await createTempDir('shopify-auth-repo-');
    process.env['SHOPIFY_CONFORMANCE_APP_HANDLE'] = 'hermes-conformance-products';
    await mkdir(path.join(repoRoot, 'shopify-conformance-app', 'hermes-conformance-products'), { recursive: true });

    expect(resolveDefaultAppEnvPath({ repoRoot })).toBe(
      '/tmp/shopify-conformance-app/hermes-conformance-products/.env',
    );
  });
});
