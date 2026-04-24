import { describe, expect, it } from 'vitest';
import { resolveConformanceTargetEnv } from '../../scripts/conformance-env-guard.js';

describe('resolveConformanceTargetEnv', () => {
  it('rejects missing conformance target variables', () => {
    expect(() => resolveConformanceTargetEnv({})).toThrow(
      'Missing required environment variables: SHOPIFY_CONFORMANCE_STORE_DOMAIN, SHOPIFY_CONFORMANCE_ADMIN_ORIGIN',
    );
  });

  it('rejects .env.example placeholder store values before token probing', () => {
    expect(() =>
      resolveConformanceTargetEnv({
        SHOPIFY_CONFORMANCE_ADMIN_ORIGIN: 'https://your-store.myshopify.com',
        SHOPIFY_CONFORMANCE_STORE_DOMAIN: 'your-store.myshopify.com',
      }),
    ).toThrow('Conformance environment still contains .env.example placeholder store values.');
  });

  it('rejects mismatched store domain and admin origin', () => {
    expect(() =>
      resolveConformanceTargetEnv({
        SHOPIFY_CONFORMANCE_ADMIN_ORIGIN: 'https://different-store.myshopify.com',
        SHOPIFY_CONFORMANCE_STORE_DOMAIN: 'very-big-test-store.myshopify.com',
      }),
    ).toThrow(
      'Expected SHOPIFY_CONFORMANCE_ADMIN_ORIGIN=https://very-big-test-store.myshopify.com to match SHOPIFY_CONFORMANCE_STORE_DOMAIN=very-big-test-store.myshopify.com',
    );
  });

  it('returns a valid concrete conformance target', () => {
    expect(
      resolveConformanceTargetEnv({
        SHOPIFY_CONFORMANCE_ADMIN_ORIGIN: 'https://very-big-test-store.myshopify.com',
        SHOPIFY_CONFORMANCE_STORE_DOMAIN: 'very-big-test-store.myshopify.com',
      }),
    ).toEqual({
      adminOrigin: 'https://very-big-test-store.myshopify.com',
      storeDomain: 'very-big-test-store.myshopify.com',
    });
  });
});
