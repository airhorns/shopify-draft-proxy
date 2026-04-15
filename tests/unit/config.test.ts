import { describe, expect, it } from 'vitest';
import { loadConfig } from '../../src/config.js';

describe('loadConfig', () => {
  it('loads required upstream config with defaults', () => {
    expect(
      loadConfig({
        SHOPIFY_ADMIN_ORIGIN: 'https://example.myshopify.com',
      }),
    ).toEqual({
      port: 3000,
      readMode: 'live-hybrid',
      shopifyAdminOrigin: 'https://example.myshopify.com',
    });
  });

  it('loads explicit mode and snapshot path', () => {
    expect(
      loadConfig({
        SHOPIFY_ADMIN_ORIGIN: 'https://example.myshopify.com',
        PORT: '4010',
        SHOPIFY_DRAFT_PROXY_READ_MODE: 'snapshot',
        SHOPIFY_DRAFT_PROXY_SNAPSHOT_PATH: '/tmp/shopify-snapshot.json',
      }),
    ).toEqual({
      port: 4010,
      readMode: 'snapshot',
      shopifyAdminOrigin: 'https://example.myshopify.com',
      snapshotPath: '/tmp/shopify-snapshot.json',
    });
  });
});
