import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { once } from 'node:events';
import { beforeAll, describe, expect, it } from 'vitest';

import { createDraftProxy, type AppConfig } from '../src/index.js';
import { ensureGleamJavaScriptBuild } from './support/gleam-build.js';

beforeAll(() => {
  ensureGleamJavaScriptBuild();
});

const baseConfig: AppConfig = {
  readMode: 'live-hybrid',
  port: 0,
  shopifyAdminOrigin: 'https://unused.example',
};

async function withUpstream<T>(
  handler: Parameters<typeof createServer>[0],
  run: (origin: string, server: Server) => Promise<T>,
): Promise<T> {
  const server = createServer(handler);
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address() as AddressInfo;

  try {
    return await run(`http://127.0.0.1:${address.port}`, server);
  } finally {
    await new Promise<void>((resolve, reject) => {
      server.close((error) => {
        if (error) reject(error);
        else resolve();
      });
    });
  }
}

describe('JS live-hybrid passthrough', () => {
  it('forwards domain-owned cold reads through processRequest with auth and upstream response headers', async () => {
    const upstreamRequests: Array<{ authorization: string | undefined; body: string }> = [];
    const upstreamBody = {
      data: {
        currentAppInstallation: {
          id: 'gid://shopify/AppInstallation/42',
        },
      },
    };

    await withUpstream(
      (req, res) => {
        let body = '';
        req.setEncoding('utf8');
        req.on('data', (chunk: string) => {
          body += chunk;
        });
        req.on('end', () => {
          upstreamRequests.push({ authorization: req.headers.authorization, body });
          res.writeHead(202, {
            'content-type': 'application/json',
            'x-test-upstream': 'domain-read',
          });
          res.end(JSON.stringify(upstreamBody));
        });
      },
      async (origin) => {
        const proxy = createDraftProxy({ ...baseConfig, shopifyAdminOrigin: origin });

        const response = await proxy.processRequest({
          method: 'POST',
          path: '/admin/api/2026-04/graphql.json',
          headers: { authorization: 'Bearer passthrough-token' },
          body: { query: '{ currentAppInstallation { id } }' },
        });

        expect(response).toMatchObject({
          status: 202,
          body: upstreamBody,
          headers: { 'x-test-upstream': 'domain-read' },
        });
        expect(upstreamRequests).toEqual([
          {
            authorization: 'Bearer passthrough-token',
            body: JSON.stringify({ query: '{ currentAppInstallation { id } }' }),
          },
        ]);
      },
    );
  });

  it('proxies unsupported mutations and records visible passthrough observability', async () => {
    await withUpstream(
      (_req, res) => {
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ data: { definitelyUnsupportedMutation: { ok: true } } }));
      },
      async (origin) => {
        const proxy = createDraftProxy({ ...baseConfig, shopifyAdminOrigin: origin });

        const response = await proxy.processRequest({
          method: 'POST',
          path: '/admin/api/2026-04/graphql.json',
          body: {
            query: 'mutation { definitelyUnsupportedMutation { ok } }',
          },
        });

        expect(response).toMatchObject({
          status: 200,
          body: { data: { definitelyUnsupportedMutation: { ok: true } } },
        });
        expect(proxy.getLog()).toMatchObject({
          entries: [
            {
              operationName: 'definitelyUnsupportedMutation',
              status: 'proxied',
              query: 'mutation { definitelyUnsupportedMutation { ok } }',
              interpreted: {
                operationType: 'mutation',
                rootFields: ['definitelyUnsupportedMutation'],
                primaryRootField: 'definitelyUnsupportedMutation',
                capability: {
                  operationName: 'definitelyUnsupportedMutation',
                  domain: 'unknown',
                  execution: 'passthrough',
                },
              },
              notes: 'Mutation passthrough placeholder until supported local staging is implemented.',
            },
          ],
        });
      },
    );
  });

  it('surfaces async upstream network failures as 502 JSON errors', async () => {
    await withUpstream(
      (req) => {
        req.socket.destroy();
      },
      async (origin) => {
        const proxy = createDraftProxy({ ...baseConfig, shopifyAdminOrigin: origin });

        const response = await proxy.processRequest({
          method: 'POST',
          path: '/admin/api/2026-04/graphql.json',
          body: { query: '{ currentAppInstallation { id } }' },
        });

        expect(response.status).toBe(502);
        expect(response.body).toMatchObject({
          errors: [{ message: expect.stringContaining('upstream network error') }],
        });
      },
    );
  });

  it('keeps supported mutations local in live-hybrid mode', async () => {
    let upstreamHits = 0;

    await withUpstream(
      (_req, res) => {
        upstreamHits += 1;
        res.writeHead(500, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ errors: [{ message: 'supported mutation leaked upstream' }] }));
      },
      async (origin) => {
        const proxy = createDraftProxy({ ...baseConfig, shopifyAdminOrigin: origin });

        const response = await proxy.processRequest({
          method: 'POST',
          path: '/admin/api/2026-04/graphql.json',
          body: {
            query:
              'mutation { savedSearchCreate(input: { name: "Promo", query: "tag:promo", resourceType: PRODUCT }) { savedSearch { id name query resourceType } userErrors { field message } } }',
          },
        });

        expect(response).toMatchObject({
          status: 200,
          body: {
            data: {
              savedSearchCreate: {
                savedSearch: {
                  id: 'gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic',
                  name: 'Promo',
                  query: 'tag:promo',
                  resourceType: 'PRODUCT',
                },
                userErrors: [],
              },
            },
          },
        });
        expect(upstreamHits).toBe(0);
      },
    );
  });
});
