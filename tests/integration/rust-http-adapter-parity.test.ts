import { createServer } from 'node:http';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { describe, expect, it } from 'vitest';

const repoRoot = new URL('../..', import.meta.url);
const pnpmCommand = 'corepack';

function pnpmArgs(args: string[]): string[] {
  return ['pnpm', ...args];
}

function collectOutput(child: ChildProcessWithoutNullStreams): { getOutput: () => string } {
  let output = '';
  child.stdout.on('data', (chunk: Buffer) => {
    output += chunk.toString();
  });
  child.stderr.on('data', (chunk: Buffer) => {
    output += chunk.toString();
  });
  return { getOutput: () => output };
}

async function waitForRustServer(child: ChildProcessWithoutNullStreams, getOutput: () => string): Promise<void> {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    if (getOutput().includes('shopify-draft-proxy rust runtime listening')) return;
    if (child.exitCode !== null) {
      throw new Error(`server process exited before listening:\n${getOutput()}`);
    }
    await delay(100);
  }
  throw new Error(`server did not start before timeout:\n${getOutput()}`);
}

async function stopServer(child: ChildProcessWithoutNullStreams): Promise<void> {
  if (child.exitCode !== null) return;
  killServerProcess(child, 'SIGTERM');
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (child.exitCode !== null) return;
    await delay(100);
  }
  killServerProcess(child, 'SIGKILL');
}

function killServerProcess(child: ChildProcessWithoutNullStreams, signal: NodeJS.Signals): void {
  try {
    if (child.pid) {
      process.kill(-child.pid, signal);
      return;
    }
    child.kill(signal);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error;
  }
}

async function withRustServer<T>(
  port: number,
  run: (origin: string) => Promise<T>,
  options: { shopifyAdminOrigin?: string; readMode?: string } = {},
): Promise<T> {
  const child = spawn(pnpmCommand, pnpmArgs(['dev']), {
    cwd: repoRoot,
    detached: true,
    env: {
      ...process.env,
      PORT: String(port),
      SHOPIFY_ADMIN_ORIGIN: options.shopifyAdminOrigin ?? 'https://shopify.com',
      READ_MODE: options.readMode,
    },
  });
  const { getOutput } = collectOutput(child);
  try {
    await waitForRustServer(child, getOutput);
    return await run(`http://127.0.0.1:${port}`);
  } finally {
    await stopServer(child);
  }
}

async function getJson(origin: string, path: string, init: RequestInit = {}) {
  const response = await fetch(`${origin}${path}`, init);
  return { status: response.status, body: await response.json() };
}

async function withChunkedUpstream<T>(port: number, run: (origin: string) => Promise<T>): Promise<T> {
  const server = createServer((request, response) => {
    request.resume();
    response.statusCode = 500;
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify({ errors: [{ message: 'unexpected upstream' }] }));
  });
  await new Promise<void>((resolve) => server.listen(port, '127.0.0.1', resolve));
  try {
    return await run(`http://127.0.0.1:${port}`);
  } finally {
    await new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
  }
}

describe('Rust HTTP adapter route surface', () => {
  it('serves the required meta route response shapes without the TS/Gleam HTTP adapter', async () => {
    await withRustServer(43_197, async (origin) => {
      expect(await getJson(origin, '/__meta/health')).toEqual({
        status: 200,
        body: {
          ok: true,
          message: 'shopify-draft-proxy is running',
        },
      });

      expect(await getJson(origin, '/__meta/config')).toEqual({
        status: 200,
        body: {
          runtime: {
            readMode: 'snapshot',
            unsupportedMutationMode: 'passthrough',
            bulkOperationRunMutationMaxInputFileSizeBytes: 104857600,
          },
          proxy: { port: 43197, shopifyAdminOrigin: 'https://shopify.com' },
          snapshot: { enabled: false, path: null },
        },
      });

      expect(await getJson(origin, '/__meta/log')).toEqual({
        status: 200,
        body: { entries: [] },
      });

      expect(await getJson(origin, '/__meta/state')).toEqual({
        status: 200,
        body: {
          baseState: { products: {}, savedSearches: {} },
          stagedState: {
            products: {},
            deletedProductIds: [],
            savedSearches: {},
            shippingPackages: {},
            deletedShippingPackageIds: {},
            delegatedAccessTokens: {},
          },
        },
      });

      expect(await getJson(origin, '/__meta/reset', { method: 'POST' })).toEqual({
        status: 200,
        body: { ok: true, message: 'state reset' },
      });
    });
  }, 25_000);

  it('serves Admin GraphQL, staged upload, and error envelopes through Rust HTTP', async () => {
    const graphQLBody = {
      query:
        'mutation { savedSearchCreate(input: { name: "Promo products", query: "tag:promo", resourceType: PRODUCT }) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } } }',
    };

    await withRustServer(43_198, async (origin) => {
      const rustCreate = await getJson(origin, '/admin/api/2026-04/graphql.json', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(graphQLBody),
      });
      expect(rustCreate).toMatchObject({
        status: 200,
        body: {
          data: {
            savedSearchCreate: {
              savedSearch: {
                id: 'gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic',
                name: 'Promo products',
                query: 'tag:promo',
                resourceType: 'PRODUCT',
              },
              userErrors: [],
            },
          },
        },
      });

      const stagedUpload = await getJson(origin, '/staged-uploads/gid%3A%2F%2Fshopify%2FProduct%2F1/import.jsonl', {
        method: 'PUT',
        body: '{"id":"gid://shopify/Product/1"}\n',
      });
      expect(stagedUpload).toEqual({
        status: 201,
        body: {
          ok: true,
          key: 'shopify-draft-proxy/gid://shopify/Product/1/import.jsonl',
        },
      });

      const rustMissing = await getJson(origin, '/missing');
      expect(rustMissing).toEqual({
        status: 404,
        body: { errors: [{ message: 'Not found' }] },
      });

      const rustMethod = await getJson(origin, '/__meta/health', { method: 'POST' });
      expect(rustMethod).toEqual({
        status: 405,
        body: { errors: [{ message: 'Method not allowed' }] },
      });
    });
  }, 25_000);

  it('forwards chunked upstream passthrough responses without producing duplicate hop-by-hop headers', async () => {
    await withChunkedUpstream(43_200, async (upstreamOrigin) => {
      await withRustServer(
        43_199,
        async (origin) => {
          const response = await getJson(origin, '/admin/api/2026-04/graphql.json', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              query:
                'mutation { delegateAccessTokenCreate(input: { expiresIn: 60, delegateAccessScope: ["read_products"] }) { userErrors { message } } }',
            }),
          });
          expect(response).toEqual({
            status: 500,
            body: { errors: [{ message: 'unexpected upstream' }] },
          });
        },
        { readMode: 'live-hybrid', shopifyAdminOrigin: upstreamOrigin },
      );
    });
  }, 25_000);
});
