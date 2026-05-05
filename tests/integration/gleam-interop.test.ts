import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createServer } from 'node:http';
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
      createDraftProxy: (options: {
        readMode: string;
        port: number;
        shopifyAdminOrigin: string;
        snapshotPath?: string;
        state?: unknown;
      }) => {
        processRequest: (req: { method: string; path: string }) => Promise<{
          status: number;
          body: unknown;
        }>;
        dumpState: (createdAt?: string) => {
          schema: string;
          createdAt: string;
        };
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
  }, 20_000);

  it('round-trips state via dumpState/restoreState with the documented schema', async () => {
    const shim = (await import(resolve(gleamProjectRoot, 'js/src/index.ts'))) as {
      createDraftProxy: (options: {
        readMode: string;
        port: number;
        shopifyAdminOrigin: string;
        snapshotPath?: string;
        state?: unknown;
      }) => {
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
    const fresh = shim.createDraftProxy({
      readMode: 'snapshot',
      port: 4000,
      shopifyAdminOrigin: 'https://shopify.com',
      state: dump,
    });
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

  it('supports the JS embeddable lifecycle through the TS-friendly shim', async () => {
    const requests: Array<{ url: string | undefined; body: string; authorization: string | undefined }> = [];
    const upstream = createServer((req, res) => {
      let body = '';
      req.setEncoding('utf8');
      req.on('data', (chunk: string) => {
        body += chunk;
      });
      req.on('end', () => {
        requests.push({
          url: req.url,
          body,
          authorization: req.headers.authorization,
        });
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(
          JSON.stringify({
            data: {
              savedSearchCreate: {
                savedSearch: {
                  id: 'gid://shopify/SavedSearch/987654321',
                  legacyResourceId: '987654321',
                },
                userErrors: [],
              },
            },
          }),
        );
      });
    });
    await new Promise<void>((resolveListen) => {
      upstream.listen(0, '127.0.0.1', resolveListen);
    });

    try {
      const { createDraftProxy, DRAFT_PROXY_STATE_DUMP_SCHEMA } = await import(
        resolve(gleamProjectRoot, 'js/src/index.ts')
      );
      const address = upstream.address();
      if (address === null || typeof address === 'string') {
        throw new Error('Expected local upstream server to listen on a TCP port.');
      }
      const proxy = createDraftProxy({
        readMode: 'snapshot',
        port: 4000,
        shopifyAdminOrigin: `http://127.0.0.1:${address.port}`,
      });
      const create = await proxy.processGraphQLRequest({
        query:
          'mutation { savedSearchCreate(input: { name: "Promo orders", query: "tag:promo", resourceType: ORDER }) { savedSearch { id name query resourceType } userErrors { field message } } }',
      });

      expect(create.status).toBe(200);
      expect(create.body).toMatchObject({
        data: {
          savedSearchCreate: {
            savedSearch: {
              id: 'gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic',
              name: 'Promo orders',
            },
            userErrors: [],
          },
        },
      });

      const read = await proxy.processGraphQLRequest({
        query: '{ orderSavedSearches(query: "Promo") { nodes { id name } } }',
      });
      expect(read.body).toMatchObject({
        data: {
          orderSavedSearches: {
            nodes: [
              {
                id: 'gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic',
                name: 'Promo orders',
              },
            ],
          },
        },
      });

      const log = proxy.getLog();
      expect(log).toMatchObject({
        entries: [
          {
            status: 'staged',
            path: '/admin/api/2025-01/graphql.json',
            query: expect.stringContaining('savedSearchCreate'),
          },
        ],
      });
      expect(proxy.getState()).toMatchObject({
        stagedState: {
          savedSearches: {
            'gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic': {
              name: 'Promo orders',
            },
          },
        },
      });

      const dump = proxy.dumpState('2026-04-30T00:00:00.000Z');
      expect(dump.schema).toBe(DRAFT_PROXY_STATE_DUMP_SCHEMA);
      const restored = createDraftProxy({
        readMode: 'snapshot',
        port: 4000,
        shopifyAdminOrigin: `http://127.0.0.1:${address.port}`,
        state: dump,
      });
      const restoredRead = await restored.processGraphQLRequest({
        query: '{ orderSavedSearches(query: "Promo") { nodes { id name } } }',
      });
      expect(restoredRead.body).toMatchObject(read.body as object);

      const commit = await restored.commit({ authorization: 'Bearer test-token' });
      expect(commit).toMatchObject({
        stopIndex: null,
        attempts: [{ status: 'committed', success: true }],
      });
      expect(requests).toHaveLength(1);
      expect(requests[0]).toMatchObject({
        url: '/admin/api/2025-01/graphql.json',
        authorization: 'Bearer test-token',
        body: expect.stringContaining('savedSearchCreate'),
      });
    } finally {
      await new Promise<void>((resolveClose, rejectClose) => {
        upstream.close((error) => {
          if (error) rejectClose(error);
          else resolveClose();
        });
      });
    }
  });
});
