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

describe('public TS API', () => {
  beforeAll(() => {
    if (!existsSync(compiledEntrypoint)) {
      execFileSync('gleam', ['build', '--target', 'javascript'], {
        cwd: gleamProjectRoot,
        stdio: 'inherit',
      });
    }
  });

  it('exposes createDraftProxy and answers /__meta/health end-to-end', async () => {
    const shim = (await import(resolve(gleamProjectRoot, 'js/src/index.ts'))) as {
      createDraftProxy: (
        config: {
          readMode: string;
          port: number;
          shopifyAdminOrigin: string;
          snapshotPath?: string;
        },
        options?: { state?: unknown },
      ) => {
        processRequest: (req: { method: string; path: string }) => Promise<{
          status: number;
          body: unknown;
        }>;
        dumpState: (createdAt?: string) => { schema: string; createdAt: string };
        restoreState: (dump: unknown) => void;
        getState: () => unknown;
      };
      DRAFT_PROXY_STATE_DUMP_SCHEMA: string;
    };
    const proxy = shim.createDraftProxy({
      readMode: 'snapshot',
      port: 4000,
      shopifyAdminOrigin: 'https://shopify.com',
    });
    const response = await proxy.processRequest({
      method: 'GET',
      path: '/__meta/health',
    });
    expect(response.status).toBe(200);
    expect(response.body).toMatchObject({
      ok: true,
      message: expect.stringContaining('shopify-draft-proxy'),
    });
  });

  it('round-trips state via dumpState/restoreState with the documented schema', async () => {
    const shim = (await import(resolve(gleamProjectRoot, 'js/src/index.ts'))) as {
      createDraftProxy: (
        config: {
          readMode: string;
          port: number;
          shopifyAdminOrigin: string;
          snapshotPath?: string;
        },
        options?: { state?: unknown },
      ) => {
        processRequest: (req: { method: string; path: string }) => Promise<{
          status: number;
          body: unknown;
        }>;
        dumpState: (createdAt?: string) => { schema: string; createdAt: string };
        restoreState: (dump: unknown) => void;
        getState: () => unknown;
      };
      DRAFT_PROXY_STATE_DUMP_SCHEMA: string;
    };
    const proxy = shim.createDraftProxy({
      readMode: 'snapshot',
      port: 4000,
      shopifyAdminOrigin: 'https://shopify.com',
    });
    const dump = proxy.dumpState('2026-04-29T12:00:00.000Z');
    expect(dump.schema).toBe(shim.DRAFT_PROXY_STATE_DUMP_SCHEMA);
    const fresh = shim.createDraftProxy(
      {
        readMode: 'snapshot',
        port: 4000,
        shopifyAdminOrigin: 'https://shopify.com',
      },
      { state: dump },
    );
    expect(fresh.getState()).toBeDefined();
  });

  it('loads an existing normalized snapshot file when snapshotPath is configured', async () => {
    const shim = (await import(resolve(gleamProjectRoot, 'js/src/index.ts'))) as {
      createDraftProxy: (config: {
        readMode: string;
        port: number;
        shopifyAdminOrigin: string;
        snapshotPath?: string;
      }) => {
        getConfig: () => { snapshot: { enabled: boolean; path: string | null } };
        getState: () => unknown;
      };
    };
    const snapshotPath = resolve(repoRoot, 'fixtures/snapshots/dev-store.json');
    const proxy = shim.createDraftProxy({
      readMode: 'snapshot',
      port: 4000,
      shopifyAdminOrigin: 'https://shopify.com',
      snapshotPath,
    });
    expect(proxy.getConfig().snapshot).toEqual({ enabled: true, path: snapshotPath });
    expect(proxy.getState()).toMatchObject({
      baseState: expect.objectContaining({
        savedSearches: {},
        webhookSubscriptions: {},
      }),
      stagedState: expect.objectContaining({
        savedSearches: {},
      }),
    });
  });
});
