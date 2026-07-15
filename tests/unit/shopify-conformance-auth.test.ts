import { mkdtemp, readFile, stat, writeFile } from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import * as path from 'node:path';

import { afterEach, describe, expect, it, vi } from 'vitest';

// scripts/ is intentionally outside tsconfig's checked sources; runtime coverage here verifies the JS helper.
import {
  buildAdminAuthHeaders,
  createConformanceAuthRequest,
  getValidConformanceAccessToken,
} from '../../scripts/shopify-conformance-auth.mjs';
import * as conformanceAuth from '../../scripts/shopify-conformance-auth.mjs';

async function createTempDir(prefix: string): Promise<string> {
  return await mkdtemp(path.join(tmpdir(), prefix));
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Storefront conformance credential separation', () => {
  it('uses a separate app identity and credential paths from the Admin conformance app', () => {
    expect(conformanceAuth.STOREFRONT_CONFORMANCE_APP_HANDLE).toBe('hermes-conformance-storefront');
    expect(conformanceAuth.STOREFRONT_CONFORMANCE_REDIRECT_URI).toBe('http://127.0.0.1:13388/auth/callback');
    expect(conformanceAuth.SHOPIFY_CONFORMANCE_STOREFRONT_ADMIN_AUTH_PATH).toBe(
      path.join(homedir(), '.shopify-draft-proxy', 'conformance-storefront-admin-auth.json'),
    );
    expect(conformanceAuth.SHOPIFY_CONFORMANCE_STOREFRONT_ADMIN_AUTH_REQUEST_PATH).toBe(
      path.join(homedir(), '.shopify-draft-proxy', 'conformance-storefront-admin-auth-request.json'),
    );
    expect(conformanceAuth.SHOPIFY_CONFORMANCE_STOREFRONT_ADMIN_PKCE_PATH).toBe(
      path.join(homedir(), '.shopify-draft-proxy', 'conformance-storefront-admin-auth-pkce.json'),
    );
  });

  it('returns the isolated app and credential profile used by Storefront workflows', () => {
    expect(conformanceAuth.getStorefrontConformanceAuthProfile()).toEqual({
      appHandle: 'hermes-conformance-storefront',
      redirectUri: 'http://127.0.0.1:13388/auth/callback',
      credentialPath: path.join(homedir(), '.shopify-draft-proxy', 'conformance-storefront-admin-auth.json'),
      authRequestPath: path.join(homedir(), '.shopify-draft-proxy', 'conformance-storefront-admin-auth-request.json'),
      pkcePath: path.join(homedir(), '.shopify-draft-proxy', 'conformance-storefront-admin-auth-pkce.json'),
    });
  });
});

describe('conformance credential file permissions', () => {
  it('writes OAuth request state with owner-only permissions', async () => {
    const dir = await createTempDir('shopify-auth-file-mode-');
    const authRequestPath = path.join(dir, 'request.json');
    const pkcePath = path.join(dir, 'pkce.json');

    await createConformanceAuthRequest({
      storeDomain: 'very-big-test-store.myshopify.com',
      clientId: 'client-id',
      scopes: ['read_products'],
      authRequestPath,
      pkcePath,
    });

    expect((await stat(authRequestPath)).mode & 0o777).toBe(0o600);
    expect((await stat(pkcePath)).mode & 0o777).toBe(0o600);
  });
});

describe('buildAdminAuthHeaders', () => {
  it('sends shpca tokens as raw X-Shopify-Access-Token headers', () => {
    expect(buildAdminAuthHeaders('shpca_live_token')).toEqual({
      'X-Shopify-Access-Token': 'shpca_live_token',
    });
  });

  it('falls back to bearer auth for non-Shopify token families', () => {
    expect(buildAdminAuthHeaders('legacy-token')).toEqual({
      Authorization: 'Bearer legacy-token',
      'X-Shopify-Access-Token': 'Bearer legacy-token',
    });
  });
});

describe('getValidConformanceAccessToken', () => {
  it('probes stored credentials against the default Admin API version', async () => {
    const dir = await createTempDir('shopify-auth-');
    const credentialPath = path.join(dir, 'conformance-admin-auth.json');
    await writeFile(
      credentialPath,
      `${JSON.stringify(
        {
          shop: 'very-big-test-store.myshopify.com',
          client_id: 'client-id',
          access_token: 'shpca_valid_token',
          refresh_token: 'shprt_valid_refresh',
        },
        null,
        2,
      )}\n`,
      'utf8',
    );

    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify({ data: { shop: { id: 'gid://shopify/Shop/1' } } }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );

    await expect(
      getValidConformanceAccessToken({
        adminOrigin: 'https://very-big-test-store.myshopify.com',
        credentialPath,
        fetchImpl: fetchMock,
        appEnvPath: path.join(dir, 'unused.env'),
      }),
    ).resolves.toBe('shpca_valid_token');

    expect(fetchMock.mock.calls[0]?.[0].toString()).toBe(
      'https://very-big-test-store.myshopify.com/admin/api/2026-04/graphql.json',
    );
  });

  it('returns the stored token when the probe succeeds', async () => {
    const dir = await createTempDir('shopify-auth-');
    const credentialPath = path.join(dir, 'conformance-admin-auth.json');
    await writeFile(
      credentialPath,
      `${JSON.stringify(
        {
          shop: 'very-big-test-store.myshopify.com',
          client_id: 'client-id',
          access_token: 'shpca_valid_token',
          refresh_token: 'shprt_valid_refresh',
        },
        null,
        2,
      )}\n`,
      'utf8',
    );

    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify({ data: { shop: { id: 'gid://shopify/Shop/1' } } }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );

    await expect(
      getValidConformanceAccessToken({
        adminOrigin: 'https://very-big-test-store.myshopify.com',
        apiVersion: '2025-01',
        credentialPath,
        fetchImpl: fetchMock,
        appEnvPath: path.join(dir, 'unused.env'),
      }),
    ).resolves.toBe('shpca_valid_token');

    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('refreshes and persists rotated tokens when the stored access token probe fails', async () => {
    const dir = await createTempDir('shopify-auth-');
    const credentialPath = path.join(dir, 'conformance-admin-auth.json');
    const appEnvPath = path.join(dir, 'app.env');
    await writeFile(
      credentialPath,
      `${JSON.stringify(
        {
          shop: 'very-big-test-store.myshopify.com',
          client_id: 'client-id',
          access_token: 'shpca_expired_token',
          refresh_token: 'shprt_old_refresh',
        },
        null,
        2,
      )}\n`,
      'utf8',
    );
    await writeFile(appEnvPath, 'SHOPIFY_API_SECRET=secret-value\n', 'utf8');

    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ errors: '[API] Invalid API key or access token' }), {
          status: 401,
          headers: { 'Content-Type': 'application/json' },
        }),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            access_token: 'shpca_new_token',
            refresh_token: 'shprt_new_refresh',
            expires_in: 3600,
            refresh_token_expires_in: 7200,
            scope: 'write_products',
          }),
          {
            status: 200,
            headers: { 'Content-Type': 'application/json' },
          },
        ),
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ data: { shop: { id: 'gid://shopify/Shop/1' } } }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      );

    await expect(
      getValidConformanceAccessToken({
        adminOrigin: 'https://very-big-test-store.myshopify.com',
        apiVersion: '2025-01',
        credentialPath,
        appEnvPath,
        fetchImpl: fetchMock,
      }),
    ).resolves.toBe('shpca_new_token');

    const persisted = JSON.parse(await readFile(credentialPath, 'utf8')) as {
      access_token: string;
      refresh_token: string;
      token_family: string;
    };
    expect(persisted.access_token).toBe('shpca_new_token');
    expect(persisted.refresh_token).toBe('shprt_new_refresh');
    expect(persisted.token_family).toBe('shpca');
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it('fails clearly when the fixed credential file does not exist', async () => {
    const dir = await createTempDir('shopify-auth-');
    const credentialPath = path.join(dir, 'missing.json');

    await expect(
      getValidConformanceAccessToken({
        adminOrigin: 'https://very-big-test-store.myshopify.com',
        apiVersion: '2025-01',
        credentialPath,
        fetchImpl: vi.fn(),
        appEnvPath: path.join(dir, 'unused.env'),
      }),
    ).rejects.toThrow(`Shopify conformance credential file not found at ${credentialPath}`);
  });

  it('fails clearly when Shopify returns the HTML invalid_request refresh page', async () => {
    const dir = await createTempDir('shopify-auth-');
    const credentialPath = path.join(dir, 'conformance-admin-auth.json');
    const appEnvPath = path.join(dir, 'app.env');
    await writeFile(
      credentialPath,
      `${JSON.stringify(
        {
          shop: 'very-big-test-store.myshopify.com',
          client_id: 'client-id',
          access_token: 'shpca_expired_token',
          refresh_token: 'shprt_dead_refresh',
        },
        null,
        2,
      )}\n`,
      'utf8',
    );
    await writeFile(appEnvPath, 'SHOPIFY_API_SECRET=secret-value\n', 'utf8');

    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ errors: '[API] Invalid API key or access token' }), {
          status: 401,
          headers: { 'Content-Type': 'application/json' },
        }),
      )
      .mockResolvedValueOnce(
        new Response(
          '<html><head><title>unauthorized - Oauth error invalid_request</title><style type="text/css">*{border:0;margin:0;padding:0} body{font-family:"Helvetica Neue";color:#6c6c6c}</style></head><body><div>Oops, something went wrong.</div><div>Oauth error invalid_request: This request requires an active refresh_token</div></body></html>',
          {
            status: 401,
            headers: { 'Content-Type': 'text/html' },
          },
        ),
      );

    await expect(
      getValidConformanceAccessToken({
        adminOrigin: 'https://very-big-test-store.myshopify.com',
        apiVersion: '2025-01',
        credentialPath,
        appEnvPath,
        fetchImpl: fetchMock,
      }),
    ).rejects.toThrow(
      'Stored Shopify conformance access token is invalid and refresh failed: This request requires an active refresh_token',
    );
  });

  it('fails clearly when the stored token is invalid and refresh also fails', async () => {
    const dir = await createTempDir('shopify-auth-');
    const credentialPath = path.join(dir, 'conformance-admin-auth.json');
    const appEnvPath = path.join(dir, 'app.env');
    await writeFile(
      credentialPath,
      `${JSON.stringify(
        {
          shop: 'very-big-test-store.myshopify.com',
          client_id: 'client-id',
          access_token: 'shpca_expired_token',
          refresh_token: 'shprt_dead_refresh',
        },
        null,
        2,
      )}\n`,
      'utf8',
    );
    await writeFile(appEnvPath, 'SHOPIFY_API_SECRET=secret-value\n', 'utf8');

    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ errors: '[API] Invalid API key or access token' }), {
          status: 401,
          headers: { 'Content-Type': 'application/json' },
        }),
      )
      .mockResolvedValueOnce(
        new Response(
          'Oops, something went wrong. Oauth error invalid_request: This request requires an active refresh_token',
          {
            status: 401,
            headers: { 'Content-Type': 'text/html' },
          },
        ),
      );

    await expect(
      getValidConformanceAccessToken({
        adminOrigin: 'https://very-big-test-store.myshopify.com',
        apiVersion: '2025-01',
        credentialPath,
        appEnvPath,
        fetchImpl: fetchMock,
      }),
    ).rejects.toThrow(
      'Stored Shopify conformance access token is invalid and refresh failed: This request requires an active refresh_token',
    );
  });
});
