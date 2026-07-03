import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');

describe('public TS API Rust runtime', () => {
  it('does not import Gleam-emitted build artifacts from the public runtime shim', () => {
    const runtimeSource = readFileSync(resolve(repoRoot, 'js/src/runtime.ts'), 'utf8');
    expect(runtimeSource).not.toContain('build/dev/javascript');
    expect(runtimeSource).not.toContain('GleamDraftProxy');
  });

  it('exposes createDraftProxy and answers /__meta/health end-to-end', async () => {
    const shim = (await import(resolve(repoRoot, 'js/src/index.ts'))) as {
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
    (proxy as unknown as { dispose: () => void }).dispose();
  }, 20_000);

  it('round-trips state via dumpState/restoreState with the documented Rust schema', async () => {
    const shim = (await import(resolve(repoRoot, 'js/src/index.ts'))) as {
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
    (proxy as unknown as { dispose: () => void }).dispose();
    (fresh as unknown as { dispose: () => void }).dispose();
  }, 30_000);

  it('reflects configured snapshot path in Rust config snapshots', async () => {
    const shim = (await import(resolve(repoRoot, 'js/src/index.ts'))) as {
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
        products: {},
        savedSearches: {},
      }),
      stagedState: expect.objectContaining({
        products: {},
        savedSearches: {},
      }),
    });
    (proxy as unknown as { dispose: () => void }).dispose();
  }, 20_000);

  it('supports the JS embeddable lifecycle through the Rust-backed TS shim', async () => {
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
      const { createDraftProxy, DRAFT_PROXY_STATE_DUMP_SCHEMA } = await import(resolve(repoRoot, 'js/src/index.ts'));
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
        ok: true,
        committed: 1,
        failed: 0,
        stopIndex: null,
        attempts: [
          {
            index: 0,
            logId: 'log-1',
            status: 'committed',
            request: { method: 'POST', path: '/admin/api/2025-01/graphql.json' },
            response: {
              status: 200,
              body: {
                data: {
                  savedSearchCreate: {
                    savedSearch: {
                      id: 'gid://shopify/SavedSearch/987654321',
                    },
                  },
                },
              },
            },
          },
        ],
      });
      expect(requests).toHaveLength(1);
      expect(requests[0]).toMatchObject({
        url: '/admin/api/2025-01/graphql.json',
        authorization: 'Bearer test-token',
        body: expect.stringContaining('savedSearchCreate'),
      });
      (proxy as unknown as { dispose: () => void }).dispose();
      (restored as unknown as { dispose: () => void }).dispose();
    } finally {
      await new Promise<void>((resolveClose, rejectClose) => {
        upstream.close((error) => {
          if (error) rejectClose(error);
          else resolveClose();
        });
      });
    }
  }, 60_000);

  it('preserves the core commit result on JS commit failures', async () => {
    const upstream = createServer((req, res) => {
      req.resume();
      req.on('end', () => {
        res.writeHead(500, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ errors: [{ message: 'upstream boom' }] }));
      });
    });
    await new Promise<void>((resolveListen) => {
      upstream.listen(0, '127.0.0.1', resolveListen);
    });

    const { createDraftProxy, DraftProxyCommitError } = await import(resolve(repoRoot, 'js/src/index.ts'));
    const address = upstream.address();
    if (address === null || typeof address === 'string') {
      throw new Error('Expected local upstream server to listen on a TCP port.');
    }
    const proxy = createDraftProxy({
      readMode: 'snapshot',
      port: 4000,
      shopifyAdminOrigin: `http://127.0.0.1:${address.port}`,
    });

    try {
      const create = await proxy.processGraphQLRequest({
        query:
          'mutation { savedSearchCreate(input: { name: "Promo orders", query: "tag:promo", resourceType: ORDER }) { savedSearch { id name query resourceType } userErrors { field message } } }',
      });
      expect(create.status).toBe(200);

      let thrown: unknown;
      try {
        await proxy.commit({ authorization: 'Bearer test-token' });
      } catch (error) {
        thrown = error;
      }

      expect(thrown).toBeInstanceOf(DraftProxyCommitError);
      const result = (thrown as InstanceType<typeof DraftProxyCommitError>).result;
      expect(result).toMatchObject({
        ok: false,
        committed: 0,
        failed: 1,
        stopIndex: 0,
        error: 'Upstream commit failed for log-1 with status 500',
        attempts: [
          {
            index: 0,
            logId: 'log-1',
            status: 'failed',
            request: { method: 'POST', path: '/admin/api/2025-01/graphql.json' },
            response: {
              status: 500,
              body: { errors: [{ message: 'upstream boom' }] },
            },
            error: 'Upstream commit failed for log-1 with status 500',
          },
        ],
      });
    } finally {
      (proxy as unknown as { dispose: () => void }).dispose();
      await new Promise<void>((resolveClose, rejectClose) => {
        upstream.close((error) => {
          if (error) rejectClose(error);
          else resolveClose();
        });
      });
    }
  }, 60_000);
});
