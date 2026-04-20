import { describe, expect, it } from 'vitest';

import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import {
  extractCliIdentityFromConfig,
  extractManualStoreAuthTokenSummary,
  extractScopesFromShopifyAppToml,
  extractShopifyAppDeployVersion,
  findConfiguredShopifyApp,
  findShopifyChannelConfigExtensions,
  isInvalidGrantRefreshResponse,
  parsePublicationTargetBlocker,
  shouldAttemptShopifyAppDeploy,
  shouldProbeManualStoreAuthFallback,
} from '../../scripts/product-publication-conformance-lib.mjs';

describe('extractCliIdentityFromConfig', () => {
  it('treats the sole Shopify CLI account session as active when active-session is absent', () => {
    const config = {
      sessionStore: JSON.stringify({
        'accounts.shopify.com': {
          'session-1': {
            identity: {
              accessToken: 'atkn_123',
              refreshToken: 'rtkn_123',
              expiresAt: '2026-04-14T16:58:05.114Z',
            },
          },
        },
      }),
    };

    expect(extractCliIdentityFromConfig(config)).toEqual({
      sessionId: 'session-1',
      identity: {
        accessToken: 'atkn_123',
        refreshToken: 'rtkn_123',
        expiresAt: '2026-04-14T16:58:05.114Z',
      },
    });
  });

  it('prefers the explicit active-session entry when present', () => {
    const config = {
      sessionStore: JSON.stringify({
        'accounts.shopify.com': {
          'active-session': 'session-2',
          'session-1': {
            identity: {
              accessToken: 'atkn_old',
              refreshToken: 'rtkn_old',
              expiresAt: '2026-04-14T16:58:05.114Z',
            },
          },
          'session-2': {
            identity: {
              accessToken: 'atkn_new',
              refreshToken: 'rtkn_new',
              expiresAt: '2026-04-16T16:58:05.114Z',
            },
          },
        },
      }),
    };

    expect(extractCliIdentityFromConfig(config)).toEqual({
      sessionId: 'session-2',
      identity: {
        accessToken: 'atkn_new',
        refreshToken: 'rtkn_new',
        expiresAt: '2026-04-16T16:58:05.114Z',
      },
    });
  });
});

describe('extractScopesFromShopifyAppToml', () => {
  it('parses the declared access scopes from shopify.app.toml', () => {
    const toml = `name = "hermes-conformance-products"

[access_scopes]
scopes = "read_products,write_products,read_product_listings,read_publications"
`;

    expect(extractScopesFromShopifyAppToml(toml)).toEqual([
      'read_products',
      'write_products',
      'read_product_listings',
      'read_publications',
    ]);
  });

  it('returns an empty list when no access_scopes block is declared', () => {
    expect(extractScopesFromShopifyAppToml('name = "hermes"\n')).toEqual([]);
  });
});

describe('findConfiguredShopifyApp', () => {
  it('finds the configured app entry by title and resolves the config path', () => {
    const cliAppConfig = {
      '/tmp/shopify-conformance-app/hermes-conformance-products': {
        directory: '/tmp/shopify-conformance-app/hermes-conformance-products',
        configFile: 'shopify.app.toml',
        appId: 'app_123',
        title: 'hermes-conformance-products',
      },
    };

    expect(findConfiguredShopifyApp(cliAppConfig, 'hermes-conformance-products')).toEqual({
      directory: '/tmp/shopify-conformance-app/hermes-conformance-products',
      configFile: 'shopify.app.toml',
      configPath: '/tmp/shopify-conformance-app/hermes-conformance-products/shopify.app.toml',
      appId: 'app_123',
      title: 'hermes-conformance-products',
    });
  });

  it('falls back to the directory basename when the title is absent', () => {
    const cliAppConfig = {
      '/tmp/shopify-conformance-app/hermes-conformance-products': {
        directory: '/tmp/shopify-conformance-app/hermes-conformance-products',
        configFile: 'shopify.app.toml',
      },
    };

    expect(findConfiguredShopifyApp(cliAppConfig, 'hermes-conformance-products')).toEqual({
      directory: '/tmp/shopify-conformance-app/hermes-conformance-products',
      configFile: 'shopify.app.toml',
      configPath: '/tmp/shopify-conformance-app/hermes-conformance-products/shopify.app.toml',
      appId: null,
      title: null,
    });
  });
});

describe('isInvalidGrantRefreshResponse', () => {
  it('detects top-level OAuth invalid_grant payloads', () => {
    expect(
      isInvalidGrantRefreshResponse({
        error: 'invalid_grant',
        error_description: 'grant expired',
      }),
    ).toBe(true);
  });

  it('detects nested Shopify errors payload invalid_grant responses', () => {
    expect(
      isInvalidGrantRefreshResponse({
        errors: [{ code: 'invalid_grant', message: 'grant expired' }],
      }),
    ).toBe(true);
  });

  it('does not misclassify other refresh failures as invalid_grant', () => {
    expect(
      isInvalidGrantRefreshResponse({
        error: 'invalid_request',
        error_description: 'missing client_id',
      }),
    ).toBe(false);
  });
});

describe('extractManualStoreAuthTokenSummary', () => {
  it('summarizes a saved manual store-auth token with its scope families', () => {
    expect(
      extractManualStoreAuthTokenSummary({
        access_token: 'shpat_1234567890',
        refresh_token: 'shprt_1234567890',
        scope: 'write_products,write_inventory,write_files,write_orders,write_customers',
        associated_user_scope: 'write_products,write_inventory,write_files,write_orders,write_customers',
        associated_user: {
          email: 'harry@harry.me',
        },
      }),
    ).toEqual({
      accessToken: 'shpat_1234567890',
      tokenFamily: 'shpat',
      hasRefreshToken: true,
      scopeHandles: ['write_products', 'write_inventory', 'write_files', 'write_orders', 'write_customers'],
      associatedUserScopeHandles: [
        'write_products',
        'write_inventory',
        'write_files',
        'write_orders',
        'write_customers',
      ],
      associatedUserEmail: 'harry@harry.me',
    });
  });

  it('recognizes non-shpat Shopify app token families from manual store auth', () => {
    expect(
      extractManualStoreAuthTokenSummary({
        access_token: 'shpua_1234567890',
        refresh_token: 'shpurt_1234567890',
        scope: 'write_products,write_publications',
        associated_user_scope: 'write_products,write_publications',
        associated_user: {
          email: 'harry@harry.me',
        },
      }),
    ).toEqual({
      accessToken: 'shpua_1234567890',
      tokenFamily: 'shpua',
      hasRefreshToken: true,
      scopeHandles: ['write_products', 'write_publications'],
      associatedUserScopeHandles: ['write_products', 'write_publications'],
      associatedUserEmail: 'harry@harry.me',
    });
  });

  it('returns null when the payload is missing an access token', () => {
    expect(
      extractManualStoreAuthTokenSummary({
        refresh_token: 'shprt_1234567890',
      }),
    ).toBeNull();
  });
});

describe('shouldProbeManualStoreAuthFallback', () => {
  it('probes saved manual store auth even when the cached scope string lacks publication scopes', () => {
    expect(
      shouldProbeManualStoreAuthFallback({
        accessToken: 'shpat_1234567890',
        tokenFamily: 'shpat',
        hasRefreshToken: true,
        scopeHandles: ['write_products', 'write_inventory'],
        associatedUserScopeHandles: ['write_products', 'write_inventory'],
        associatedUserEmail: 'harry@harry.me',
      }),
    ).toBe(true);
  });

  it('skips probing when the saved manual token summary has no access token', () => {
    expect(
      shouldProbeManualStoreAuthFallback({
        accessToken: '',
        tokenFamily: 'shpat',
        hasRefreshToken: true,
        scopeHandles: ['write_products'],
        associatedUserScopeHandles: ['write_products'],
        associatedUserEmail: 'harry@harry.me',
      }),
    ).toBe(false);
  });
});

describe('shouldAttemptShopifyAppDeploy', () => {
  it('attempts deploy when Shopify app CLI auth is available and requested scopes are still missing', () => {
    expect(
      shouldAttemptShopifyAppDeploy(
        {
          status: 'available',
          workdir: '/tmp/shopify-conformance-app/hermes-conformance-products',
          command: 'corepack pnpm exec shopify app info --json',
        },
        {
          missingRequestedScopes: ['read_product_listings', 'read_publications', 'write_publications'],
        },
      ),
    ).toBe(true);
  });

  it('skips deploy when Shopify app CLI auth is unavailable or no requested scopes are missing', () => {
    expect(
      shouldAttemptShopifyAppDeploy(
        {
          status: 'login-required',
          workdir: '/tmp/shopify-conformance-app/hermes-conformance-products',
          command: 'corepack pnpm exec shopify app info --json',
        },
        {
          missingRequestedScopes: ['read_publications'],
        },
      ),
    ).toBe(false);

    expect(
      shouldAttemptShopifyAppDeploy(
        {
          status: 'available',
          workdir: '/tmp/shopify-conformance-app/hermes-conformance-products',
          command: 'corepack pnpm exec shopify app info --json',
        },
        {
          missingRequestedScopes: [],
        },
      ),
    ).toBe(false);
  });
});

describe('extractShopifyAppDeployVersion', () => {
  it('extracts the released app version label from deploy output', () => {
    expect(
      extractShopifyAppDeployVersion(`
╭─ success ────────────────────────────────────────────────────────────────────╮
│  New version released to users.                                              │
│  hermes-conformance-products-4 [1]                                           │
╰──────────────────────────────────────────────────────────────────────────────╯
`),
    ).toBe('hermes-conformance-products-4');
  });

  it('returns null when deploy output does not include a released version label', () => {
    expect(extractShopifyAppDeployVersion('No release created')).toBeNull();
  });
});

describe('findShopifyChannelConfigExtensions', () => {
  it('finds channel_config extensions and extracts legacy-install flags from app directories', async () => {
    const appDir = mkdtempSync(join(tmpdir(), 'shopify-channel-config-'));
    const extensionDir = join(appDir, 'extensions', 'conformance-publication-target');
    mkdirSync(extensionDir, { recursive: true });
    writeFileSync(
      join(extensionDir, 'shopify.extension.toml'),
      `[[extensions]]\ntype = "channel_config"\nname = "example config"\nhandle = "conformance-publication-target"\ncreate_legacy_channel_on_app_install = false\n`,
      'utf8',
    );

    try {
      await expect(findShopifyChannelConfigExtensions(appDir)).resolves.toEqual([
        {
          extensionPath: join(extensionDir, 'shopify.extension.toml'),
          handle: 'conformance-publication-target',
          createLegacyChannelOnAppInstall: false,
        },
      ]);
    } finally {
      rmSync(appDir, { recursive: true, force: true });
    }
  });

  it('ignores non-channel-config extension definitions', async () => {
    const appDir = mkdtempSync(join(tmpdir(), 'shopify-channel-config-'));
    const extensionDir = join(appDir, 'extensions', 'theme-extension');
    mkdirSync(extensionDir, { recursive: true });
    writeFileSync(
      join(extensionDir, 'shopify.extension.toml'),
      `[[extensions]]\ntype = "theme_app_extension"\nname = "theme app ext"\nhandle = "theme-extension"\n`,
      'utf8',
    );

    try {
      await expect(findShopifyChannelConfigExtensions(appDir)).resolves.toEqual([]);
    } finally {
      rmSync(appDir, { recursive: true, force: true });
    }
  });
});

describe('parsePublicationTargetBlocker', () => {
  it('extracts the missing publication target blocker from a Shopify NOT_FOUND payload', () => {
    expect(
      parsePublicationTargetBlocker({
        status: 200,
        payload: {
          errors: [
            {
              message: "Your app doesn't have a publication for this shop.",
              path: ['productPublish', 'product', 'publishedOnCurrentPublication'],
              extensions: {
                code: 'NOT_FOUND',
              },
            },
          ],
        },
      }),
    ).toEqual({
      operationName: 'productPublish',
      message: "Your app doesn't have a publication for this shop.",
      errorCode: 'NOT_FOUND',
    });
  });

  it('ignores unrelated GraphQL errors', () => {
    expect(
      parsePublicationTargetBlocker({
        status: 200,
        payload: {
          errors: [
            {
              message: 'Access denied',
              path: ['productPublish'],
              extensions: {
                code: 'ACCESS_DENIED',
              },
            },
          ],
        },
      }),
    ).toBeNull();
  });
});
