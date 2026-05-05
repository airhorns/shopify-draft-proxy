import { execFileSync } from 'node:child_process';
import { once } from 'node:events';
import type { Server } from 'node:http';
import { beforeAll, describe, expect, it } from 'vitest';

import type { AppConfig, DraftProxyHttpApp } from '../../js/src/index.js';

let createGleamApp: (config: AppConfig) => DraftProxyHttpApp;

const config: AppConfig = {
  readMode: 'snapshot',
  port: 0,
  shopifyAdminOrigin: 'https://shopify.com',
};
const adapterTimeoutMs = 20_000;

beforeAll(() => {
  execFileSync('gleam', ['build', '--target', 'javascript'], {
    cwd: new URL('../..', import.meta.url),
    stdio: 'pipe',
  });
}, adapterTimeoutMs);

beforeAll(async () => {
  ({ createApp: createGleamApp } = await import('../../js/src/index.js'));
}, adapterTimeoutMs);

async function withGleamServer<T>(run: (origin: string) => Promise<T>): Promise<T> {
  const server = createGleamApp(config).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (address === null || typeof address === 'string') {
    throw new Error('Expected Gleam HTTP adapter to listen on a TCP port.');
  }

  try {
    return await run(`http://127.0.0.1:${address.port}`);
  } finally {
    await closeServer(server);
  }
}

async function closeServer(server: Server): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    server.close((error) => {
      if (error) reject(error);
      else resolve();
    });
  });
}

async function getGleamJson(origin: string, path: string, init: RequestInit = {}) {
  const response = await fetch(`${origin}${path}`, init);
  return { status: response.status, body: await response.json() };
}

describe('Gleam JS HTTP adapter route surface', () => {
  it('serves the required meta route response shapes', async () => {
    await withGleamServer(async (origin) => {
      expect(await getGleamJson(origin, '/__meta/health')).toEqual({
        status: 200,
        body: {
          ok: true,
          message: 'shopify-draft-proxy is running',
        },
      });

      expect(await getGleamJson(origin, '/__meta/config')).toEqual({
        status: 200,
        body: {
          runtime: { readMode: 'snapshot', unsupportedMutationMode: 'passthrough' },
          proxy: { port: 0, shopifyAdminOrigin: 'https://shopify.com' },
          snapshot: { enabled: false, path: null },
        },
      });

      expect(await getGleamJson(origin, '/__meta/log')).toEqual({
        status: 200,
        body: { entries: [] },
      });

      expect(await getGleamJson(origin, '/__meta/state')).toMatchObject({
        status: 200,
        body: {
          baseState: expect.any(Object),
          stagedState: expect.any(Object),
        },
      });

      expect(await getGleamJson(origin, '/__meta/reset', { method: 'POST' })).toEqual({
        status: 200,
        body: { ok: true, message: 'state reset' },
      });
    });
  });

  it('serves Admin GraphQL and error envelopes for the required route surface', async () => {
    const graphQLBody = {
      query:
        'mutation { savedSearchCreate(input: { name: "Promo products", query: "tag:promo", resourceType: PRODUCT }) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } } }',
    };

    await withGleamServer(async (origin) => {
      const gleamCreate = await getGleamJson(origin, '/admin/api/2026-04/graphql.json', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(graphQLBody),
      });
      expect(gleamCreate).toMatchObject({
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

      const gleamMissing = await getGleamJson(origin, '/missing');
      expect(gleamMissing).toEqual({
        status: 404,
        body: { errors: [{ message: 'Not found' }] },
      });

      const gleamMethod = await getGleamJson(origin, '/__meta/health', { method: 'POST' });
      expect(gleamMethod).toEqual({
        status: 405,
        body: { errors: [{ message: 'Method not allowed' }] },
      });
    });
  });
});
