import { describe, expect, it } from 'vitest';

import { readConformanceScriptConfig } from '../../scripts/conformance-script-config.js';

describe('conformance script config', () => {
  it('reads store domain, admin origin, and default API version from env', () => {
    expect(
      readConformanceScriptConfig({
        env: {
          SHOPIFY_CONFORMANCE_STORE_DOMAIN: 'example.myshopify.com',
          SHOPIFY_CONFORMANCE_ADMIN_ORIGIN: 'https://example.myshopify.com',
        },
      }),
    ).toEqual({
      storeDomain: 'example.myshopify.com',
      adminOrigin: 'https://example.myshopify.com',
      apiVersion: '2026-04',
    });
  });

  it('preserves script-specific default API versions', () => {
    expect(
      readConformanceScriptConfig({
        defaultApiVersion: '2026-04',
        env: {
          SHOPIFY_CONFORMANCE_STORE_DOMAIN: 'example.myshopify.com',
          SHOPIFY_CONFORMANCE_ADMIN_ORIGIN: 'https://example.myshopify.com',
        },
      }).apiVersion,
    ).toBe('2026-04');
  });

  it('allows explicit API version overrides', () => {
    expect(
      readConformanceScriptConfig({
        env: {
          SHOPIFY_CONFORMANCE_STORE_DOMAIN: 'example.myshopify.com',
          SHOPIFY_CONFORMANCE_ADMIN_ORIGIN: 'https://example.myshopify.com',
          SHOPIFY_CONFORMANCE_API_VERSION: '2026-10',
        },
      }).apiVersion,
    ).toBe('2026-10');
  });

  it('reports all missing required conformance variables together', () => {
    expect(() => readConformanceScriptConfig({ env: {} })).toThrow(
      'Missing required environment variables: SHOPIFY_CONFORMANCE_STORE_DOMAIN, SHOPIFY_CONFORMANCE_ADMIN_ORIGIN',
    );
  });

  it('can derive admin origin from the store domain for refresh-style scripts', () => {
    expect(
      readConformanceScriptConfig({
        requireAdminOrigin: false,
        env: {
          SHOPIFY_CONFORMANCE_STORE_DOMAIN: 'example.myshopify.com',
        },
      }),
    ).toEqual({
      storeDomain: 'example.myshopify.com',
      adminOrigin: 'https://example.myshopify.com',
      apiVersion: '2026-04',
    });
  });
});
