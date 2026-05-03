import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { once } from 'node:events';
import { execFileSync, spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';

import { createApp, createDraftProxy, loadConfig, type AppConfig } from '../src/index.js';
import { ensureGleamJavaScriptBuild } from './support/gleam-build.js';

const baseConfig: AppConfig = {
  readMode: 'snapshot',
  port: 0,
  shopifyAdminOrigin: 'https://shopify.com',
};

beforeAll(() => {
  ensureGleamJavaScriptBuild();
});

async function withApp<T>(config: AppConfig, run: (origin: string) => Promise<T>, proxy = createDraftProxy(config)) {
  const server = createApp(config, proxy).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (address === null || typeof address === 'string') {
    throw new Error('Expected HTTP adapter to listen on a TCP port.');
  }

  try {
    return await run(`http://127.0.0.1:${address.port}`);
  } finally {
    await new Promise<void>((resolve, reject) => {
      server.close((error) => {
        if (error) reject(error);
        else resolve();
      });
    });
  }
}

async function jsonRequest(origin: string, path: string, init: RequestInit = {}) {
  const response = await fetch(`${origin}${path}`, init);
  return {
    status: response.status,
    contentType: response.headers.get('content-type') ?? '',
    body: await response.json(),
  };
}

async function textRequest(origin: string, path: string, init: RequestInit = {}) {
  const response = await fetch(`${origin}${path}`, init);
  return {
    status: response.status,
    contentType: response.headers.get('content-type') ?? '',
    body: await response.text(),
  };
}

describe('Node HTTP adapter', () => {
  it('loads config from the legacy service environment variables', () => {
    expect(
      loadConfig({
        PORT: '43196',
        SHOPIFY_ADMIN_ORIGIN: 'https://example.myshopify.com',
        SHOPIFY_DRAFT_PROXY_READ_MODE: 'snapshot',
        SHOPIFY_DRAFT_PROXY_SNAPSHOT_PATH: '/tmp/snapshot.json',
      }),
    ).toEqual({
      port: 43196,
      shopifyAdminOrigin: 'https://example.myshopify.com',
      readMode: 'snapshot',
      snapshotPath: '/tmp/snapshot.json',
    });
  });

  it('serves the required meta routes with legacy JSON response shapes', async () => {
    await withApp(baseConfig, async (origin) => {
      await expect(jsonRequest(origin, '/__meta/health')).resolves.toMatchObject({
        status: 200,
        contentType: expect.stringContaining('application/json'),
        body: {
          ok: true,
          message: 'shopify-draft-proxy is running',
        },
      });
      await expect(jsonRequest(origin, '/__meta/config')).resolves.toMatchObject({
        status: 200,
        body: {
          runtime: { readMode: 'snapshot' },
          proxy: { port: 0, shopifyAdminOrigin: 'https://shopify.com' },
          snapshot: { enabled: false, path: null },
        },
      });
      await expect(jsonRequest(origin, '/__meta/log')).resolves.toMatchObject({
        status: 200,
        body: { entries: [] },
      });
      await expect(jsonRequest(origin, '/__meta/state')).resolves.toMatchObject({
        status: 200,
        body: {
          baseState: expect.any(Object),
          stagedState: expect.any(Object),
        },
      });
      await expect(jsonRequest(origin, '/__meta/reset', { method: 'POST' })).resolves.toMatchObject({
        status: 200,
        body: { ok: true, message: 'state reset' },
      });
    });
  });

  it('routes Admin GraphQL bodies through the Gleam core and preserves commit auth headers', async () => {
    const upstreamRequests: Array<{ url: string | undefined; body: string; authorization: string | undefined }> = [];
    const upstream = createServer((req, res) => {
      let body = '';
      req.setEncoding('utf8');
      req.on('data', (chunk: string) => {
        body += chunk;
      });
      req.on('end', () => {
        upstreamRequests.push({
          url: req.url,
          body,
          authorization: req.headers.authorization,
        });
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(
          JSON.stringify({
            data: {
              savedSearchCreate: {
                savedSearch: { id: 'gid://shopify/SavedSearch/987654321' },
                userErrors: [],
              },
            },
          }),
        );
      });
    });
    upstream.listen(0, '127.0.0.1');
    await once(upstream, 'listening');
    const address = upstream.address() as AddressInfo;

    try {
      await withApp(
        {
          ...baseConfig,
          shopifyAdminOrigin: `http://127.0.0.1:${address.port}`,
        },
        async (origin) => {
          const create = await jsonRequest(origin, '/admin/api/2026-04/graphql.json', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              query:
                'mutation { savedSearchCreate(input: { name: "Promo products", query: "tag:promo", resourceType: PRODUCT }) { savedSearch { id name query resourceType } userErrors { field message } } }',
            }),
          });

          expect(create).toMatchObject({
            status: 200,
            body: {
              data: {
                savedSearchCreate: {
                  savedSearch: {
                    id: 'gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic',
                    name: 'Promo products',
                  },
                  userErrors: [],
                },
              },
            },
          });

          await expect(
            jsonRequest(origin, '/__meta/commit', {
              method: 'POST',
              headers: { authorization: 'Bearer test-token' },
            }),
          ).resolves.toMatchObject({
            status: 200,
            body: {
              ok: true,
              stopIndex: null,
              attempts: [{ status: 'committed', success: true }],
            },
          });

          expect(upstreamRequests).toHaveLength(1);
          expect(upstreamRequests[0]).toMatchObject({
            url: '/admin/api/2026-04/graphql.json',
            authorization: 'Bearer test-token',
            body: expect.stringContaining('savedSearchCreate'),
          });
        },
      );
    } finally {
      await new Promise<void>((resolve, reject) => {
        upstream.close((error) => {
          if (error) reject(error);
          else resolve();
        });
      });
    }
  });

  it('preserves legacy HTTP error envelopes for unknown paths and methods', async () => {
    await withApp(baseConfig, async (origin) => {
      await expect(jsonRequest(origin, '/missing')).resolves.toMatchObject({
        status: 404,
        body: { errors: [{ message: 'Not found' }] },
      });
      await expect(jsonRequest(origin, '/__meta/health', { method: 'POST' })).resolves.toMatchObject({
        status: 405,
        body: { errors: [{ message: 'Method not allowed' }] },
      });
    });
  });

  it('serves staged uploads and generated bulk operation artifacts from instance-owned state', async () => {
    let resultPath = '';
    let legacyResultPath = '';

    await withApp(baseConfig, async (origin) => {
      const stagedUpload = await jsonRequest(origin, '/admin/api/2026-04/graphql.json', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          query: `mutation StagedUploadsCreate($input: [StagedUploadInput!]!) {
            stagedUploadsCreate(input: $input) {
              stagedTargets {
                resourceUrl
                parameters {
                  name
                  value
                }
              }
              userErrors {
                field
                message
              }
            }
          }`,
          variables: {
            input: [
              {
                resource: 'BULK_MUTATION_VARIABLES',
                filename: 'product-create.jsonl',
                mimeType: 'text/jsonl',
                httpMethod: 'POST',
              },
            ],
          },
        }),
      });
      const target = (
        stagedUpload.body as {
          data: {
            stagedUploadsCreate: {
              stagedTargets: Array<{
                resourceUrl: string;
                parameters: Array<{ name: string; value: string }>;
              }>;
              userErrors: unknown[];
            };
          };
        }
      ).data.stagedUploadsCreate.stagedTargets[0];
      const stagedUploadPath = target?.parameters.find((parameter) => parameter.name === 'key')?.value;

      expect(stagedUpload.status).toBe(200);
      expect(stagedUploadPath).toEqual(
        expect.stringMatching(
          /^shopify-draft-proxy\/gid:\/\/shopify\/StagedUploadTarget0\/\d+\/product-create\.jsonl$/,
        ),
      );
      expect(
        (stagedUpload.body as { data: { stagedUploadsCreate: { userErrors: unknown[] } } }).data.stagedUploadsCreate
          .userErrors,
      ).toEqual([]);
      if (target === undefined || stagedUploadPath === undefined) {
        throw new Error('stagedUploadsCreate did not return an upload target.');
      }

      const upload = await jsonRequest(origin, new URL(target.resourceUrl).pathname, {
        method: 'POST',
        headers: { 'content-type': 'text/jsonl' },
        body: `${JSON.stringify({ product: { title: 'Bulk HTTP Hat', status: 'DRAFT' } })}\n`,
      });

      expect(upload).toMatchObject({
        status: 201,
        body: { ok: true, key: stagedUploadPath },
      });

      const innerMutation = `mutation ProductCreate($product: ProductCreateInput!) {
        productCreate(product: $product) {
          product {
            id
            title
            handle
            status
          }
          userErrors {
            field
            message
          }
        }
      }`;
      const bulkRun = await jsonRequest(origin, '/admin/api/2026-04/graphql.json', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          query: `mutation BulkImport($mutation: String!, $stagedUploadPath: String!) {
            bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $stagedUploadPath) {
              bulkOperation {
                id
                status
                type
                objectCount
                rootObjectCount
                fileSize
                url
                partialDataUrl
                query
              }
              userErrors {
                field
                message
              }
            }
          }`,
          variables: {
            mutation: innerMutation,
            stagedUploadPath,
          },
        }),
      });
      const bulkOperation = (
        bulkRun.body as {
          data: {
            bulkOperationRunMutation: {
              bulkOperation: {
                id: string;
                status: string;
                type: string;
                objectCount: string;
                rootObjectCount: string;
                url: string;
                partialDataUrl: string | null;
              };
              userErrors: unknown[];
            };
          };
        }
      ).data.bulkOperationRunMutation.bulkOperation;

      expect(bulkRun.status).toBe(200);
      expect(
        (bulkRun.body as { data: { bulkOperationRunMutation: { userErrors: unknown[] } } }).data
          .bulkOperationRunMutation.userErrors,
      ).toEqual([]);
      expect(bulkOperation).toMatchObject({
        status: 'COMPLETED',
        type: 'MUTATION',
        objectCount: '1',
        rootObjectCount: '1',
        partialDataUrl: null,
      });

      resultPath = new URL(bulkOperation.url).pathname;
      legacyResultPath = `/__bulk_operations/${bulkOperation.id.split('/').at(-1)}/result.jsonl`;

      const result = await textRequest(origin, resultPath);
      expect(result.status).toBe(200);
      expect(result.contentType).toContain('application/jsonl');
      const rows = result.body
        .trim()
        .split('\n')
        .map((line) => JSON.parse(line) as Record<string, unknown>);
      expect(rows).toMatchObject([
        {
          line: 1,
          response: {
            data: {
              productCreate: {
                product: {
                  id: expect.stringMatching(/^gid:\/\/shopify\/Product\/\d+\?shopify-draft-proxy=synthetic$/),
                  title: 'Bulk HTTP Hat',
                  handle: 'bulk-http-hat',
                  status: 'DRAFT',
                },
                userErrors: [],
              },
            },
          },
        },
      ]);

      const legacyResult = await textRequest(origin, legacyResultPath);
      expect(legacyResult).toMatchObject({
        status: 200,
        body: result.body,
      });
    });

    await withApp(baseConfig, async (origin) => {
      await expect(textRequest(origin, resultPath)).resolves.toMatchObject({
        status: 404,
        body: 'Bulk operation result not found',
      });
      await expect(textRequest(origin, legacyResultPath)).resolves.toMatchObject({
        status: 404,
        body: 'Bulk operation result not found',
      });
    });
  });
});

describe('package launch script', () => {
  it('starts the JS adapter with the dev script and serves health', async () => {
    await expectLaunchScriptHealth('dev', 43_197);
  }, 20_000);

  it('starts the built JS adapter with the start script and serves health', async () => {
    execFileSync('corepack', ['pnpm', 'build'], {
      cwd: new URL('..', import.meta.url),
      stdio: 'pipe',
    });
    await expectLaunchScriptHealth('start', 43_198);
  }, 20_000);
});

async function expectLaunchScriptHealth(script: 'dev' | 'start', port: number): Promise<void> {
  const child = spawn('corepack', ['pnpm', script], {
    cwd: new URL('..', import.meta.url),
    detached: true,
    env: {
      ...process.env,
      PORT: String(port),
      SHOPIFY_ADMIN_ORIGIN: 'https://example.myshopify.com',
      SHOPIFY_DRAFT_PROXY_READ_MODE: 'snapshot',
    },
  });
  const output = collectOutput(child);

  try {
    await waitForListening(child, output);
    const response = await fetch(`http://127.0.0.1:${port}/__meta/health`);
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual({
      ok: true,
      message: 'shopify-draft-proxy is running',
    });
  } finally {
    await stopProcessGroup(child);
  }
}

function collectOutput(child: ChildProcessWithoutNullStreams): () => string {
  let output = '';
  child.stdout.on('data', (chunk: Buffer) => {
    output += chunk.toString();
  });
  child.stderr.on('data', (chunk: Buffer) => {
    output += chunk.toString();
  });
  return () => output;
}

async function waitForListening(child: ChildProcessWithoutNullStreams, output: () => string): Promise<void> {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    if (output().includes('shopify-draft-proxy listening')) {
      return;
    }
    if (child.exitCode !== null) {
      throw new Error(`server exited before listening:\n${output()}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`server did not listen before timeout:\n${output()}`);
}

async function stopProcessGroup(child: ChildProcessWithoutNullStreams): Promise<void> {
  if (child.exitCode !== null) {
    return;
  }
  if (child.pid) {
    process.kill(-child.pid, 'SIGTERM');
  } else {
    child.kill('SIGTERM');
  }
  const deadline = Date.now() + 2_000;
  while (Date.now() < deadline && child.exitCode === null) {
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (child.exitCode === null) {
    child.kill('SIGKILL');
  }
}
