import { execFileSync } from 'node:child_process';
import { once } from 'node:events';
import type { Server } from 'node:http';
import request from 'supertest';
import { beforeAll, describe, expect, it } from 'vitest';

import { createApp as createLegacyApp } from '../../src/app.js';
import type { AppConfig, DraftProxyHttpApp } from '../../gleam/js/src/index.js';

let createGleamApp: (config: AppConfig) => DraftProxyHttpApp;

const config: AppConfig = {
  readMode: 'snapshot',
  port: 0,
  shopifyAdminOrigin: 'https://shopify.com',
};
const adapterTimeoutMs = 20_000;

beforeAll(() => {
  execFileSync('gleam', ['build', '--target', 'javascript'], {
    cwd: new URL('../../gleam', import.meta.url),
    stdio: 'pipe',
  });
}, adapterTimeoutMs);

beforeAll(async () => {
  ({ createApp: createGleamApp } = await import('../../gleam/js/src/index.js'));
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

describe('Gleam JS HTTP adapter legacy route parity', () => {
  it('matches legacy Koa meta route response shapes', async () => {
    const legacy = createLegacyApp(config).callback();

    await withGleamServer(async (origin) => {
      for (const path of ['/__meta/health', '/__meta/config', '/__meta/log'] as const) {
        const legacyResponse = await request(legacy).get(path);
        const gleamResponse = await getGleamJson(origin, path);
        expect(gleamResponse).toEqual({
          status: legacyResponse.status,
          body: legacyResponse.body,
        });
      }

      const legacyState = await request(legacy).get('/__meta/state');
      const gleamState = await getGleamJson(origin, '/__meta/state');
      expect(gleamState.status).toBe(legacyState.status);
      expect(gleamState.body).toEqual({
        baseState: expect.any(Object),
        stagedState: expect.any(Object),
      });

      const legacyReset = await request(legacy).post('/__meta/reset');
      const gleamReset = await getGleamJson(origin, '/__meta/reset', { method: 'POST' });
      expect(gleamReset).toEqual({
        status: legacyReset.status,
        body: legacyReset.body,
      });
    });
  });

  it('matches legacy Koa Admin GraphQL and error envelopes for the required route surface', async () => {
    const legacy = createLegacyApp(config).callback();
    const graphQLBody = {
      query:
        'mutation { savedSearchCreate(input: { name: "Promo products", query: "tag:promo", resourceType: PRODUCT }) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } } }',
    };

    await withGleamServer(async (origin) => {
      const legacyCreate = await request(legacy).post('/admin/api/2026-04/graphql.json').send(graphQLBody);
      const gleamCreate = await getGleamJson(origin, '/admin/api/2026-04/graphql.json', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(graphQLBody),
      });
      expect(gleamCreate).toEqual({
        status: legacyCreate.status,
        body: legacyCreate.body,
      });

      const legacyMissing = await request(legacy).get('/missing');
      const gleamMissing = await getGleamJson(origin, '/missing');
      expect(gleamMissing).toEqual({
        status: legacyMissing.status,
        body: legacyMissing.body,
      });

      const legacyMethod = await request(legacy).post('/__meta/health');
      const gleamMethod = await getGleamJson(origin, '/__meta/health', { method: 'POST' });
      expect(gleamMethod).toEqual({
        status: legacyMethod.status,
        body: legacyMethod.body,
      });
    });
  });
});
