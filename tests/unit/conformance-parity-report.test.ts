import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('generated conformance parity reports', () => {
  it('publishes machine-readable and markdown parity status reports with isolated publication aggregate blockers', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const jsonPath = resolve(repoRoot, 'docs/generated/conformance-parity-status.json');
    const markdownPath = resolve(repoRoot, 'docs/generated/conformance-parity-status.md');

    expect(existsSync(jsonPath)).toBe(true);
    expect(existsSync(markdownPath)).toBe(true);

    const status = JSON.parse(readFileSync(jsonPath, 'utf8')) as {
      total?: number;
      readyForComparison?: number;
      blocked?: number;
      pending?: number;
      results?: Array<{
        scenarioId?: string;
        state?: string;
        blocker?: {
          kind?: string;
          blockerPath?: string;
          details?: {
            blockedFields?: string[];
            blockedMutations?: string[];
            appConfigPath?: string;
            appId?: string;
            appHandle?: string;
            publicationTargetStatus?: string;
            publicationTargetMessage?: string;
            shopifyAppCliAuthStatus?: string;
            shopifyAppCliAuthWorkdir?: string;
            shopifyAppDeployStatus?: string;
            shopifyAppDeployCommand?: string;
            shopifyAppDeployVersion?: string;
            channelConfigExtensionPath?: string;
            channelConfigHandle?: string;
            channelConfigCreateLegacyChannelOnAppInstall?: boolean;
            publicationTargetRemediation?: string;
            activeCredentialTokenFamily?: string;
            activeCredentialHeaderMode?: string;
            activeCredentialSummary?: string;
          };
        } | null;
      }>;
    };
    const markdown = readFileSync(markdownPath, 'utf8');

    expect(status.total).toBeGreaterThan(0);
    expect(status.total).toBe(102);
    expect(status.readyForComparison).toBe(93);
    expect(status.blocked).toBe(9);
    expect(status.pending).toBe(0);
    expect((status.readyForComparison ?? 0) + (status.blocked ?? 0) + (status.pending ?? 0)).toBe(status.total);
    expect(status.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          scenarioId: 'customer-detail-parity-plan',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'customers-catalog-parity-plan',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'productPublish-parity-plan',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'productUnpublish-parity-plan',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'productPublish-aggregate-parity-blocker',
          state: 'blocked-with-proxy-request',
          blocker: expect.objectContaining({
            kind: 'missing-publication-target',
            blockerPath: 'pending/product-publication-conformance-scope-blocker.md',
            details: expect.objectContaining({
              blockedFields: [
                'publishedOnCurrentPublication',
                'availablePublicationsCount',
                'resourcePublicationsCount',
              ],
              blockedMutations: ['productPublish', 'productUnpublish'],
              appConfigPath: '/tmp/shopify-conformance-app/hermes-conformance-products/shopify.app.toml',
              appId: '0db6d7e08e4ba05ce97440df36c7ed33',
              appHandle: 'hermes-conformance-products',
              publicationTargetStatus: 'app-missing-publication',
              publicationTargetMessage: "Your app doesn't have a publication for this shop.",
              shopifyAppCliAuthStatus: 'available',
              shopifyAppCliAuthWorkdir: '/tmp/shopify-conformance-app/hermes-conformance-products',
              shopifyAppDeployStatus: 'deployed-but-app-still-lacks-publication',
              shopifyAppDeployCommand: 'corepack pnpm exec shopify app deploy --allow-updates',
              shopifyAppDeployVersion: expect.stringContaining('hermes-conformance-products-'),
              channelConfigExtensionPath: '/tmp/shopify-conformance-app/hermes-conformance-products/extensions/conformance-publication-target/shopify.extension.toml',
              channelConfigHandle: 'conformance-publication-target',
              channelConfigCreateLegacyChannelOnAppInstall: true,
              publicationTargetRemediation: 'channel-config-change-still-needs-reinstall',
              activeCredentialTokenFamily: 'shpca',
              activeCredentialHeaderMode: 'raw-x-shopify-access-token',
              activeCredentialSummary: expect.stringContaining('shpca'),
            }),
          }),
        }),
        expect.objectContaining({
          scenarioId: 'productUnpublish-aggregate-parity-blocker',
          state: 'blocked-with-proxy-request',
          blocker: expect.objectContaining({
            kind: 'missing-publication-target',
            blockerPath: 'pending/product-publication-conformance-scope-blocker.md',
            details: expect.objectContaining({
              blockedFields: [
                'publishedOnCurrentPublication',
                'availablePublicationsCount',
                'resourcePublicationsCount',
              ],
              blockedMutations: ['productPublish', 'productUnpublish'],
              appConfigPath: '/tmp/shopify-conformance-app/hermes-conformance-products/shopify.app.toml',
              appId: '0db6d7e08e4ba05ce97440df36c7ed33',
              appHandle: 'hermes-conformance-products',
              publicationTargetStatus: 'app-missing-publication',
              publicationTargetMessage: "Your app doesn't have a publication for this shop.",
              shopifyAppCliAuthStatus: 'available',
              shopifyAppCliAuthWorkdir: '/tmp/shopify-conformance-app/hermes-conformance-products',
              shopifyAppDeployStatus: 'deployed-but-app-still-lacks-publication',
              shopifyAppDeployCommand: 'corepack pnpm exec shopify app deploy --allow-updates',
              shopifyAppDeployVersion: expect.stringContaining('hermes-conformance-products-'),
              channelConfigExtensionPath: '/tmp/shopify-conformance-app/hermes-conformance-products/extensions/conformance-publication-target/shopify.extension.toml',
              channelConfigHandle: 'conformance-publication-target',
              channelConfigCreateLegacyChannelOnAppInstall: true,
              publicationTargetRemediation: 'channel-config-change-still-needs-reinstall',
              activeCredentialTokenFamily: 'shpca',
              activeCredentialHeaderMode: 'raw-x-shopify-access-token',
              activeCredentialSummary: expect.stringContaining('shpca'),
            }),
          }),
        }),
        expect.objectContaining({
          scenarioId: 'order-empty-state-read',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-update-live-parity',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-create-inline-missing-order-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-create-inline-null-order-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-create-missing-order-invalid-variable',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-update-inline-missing-id-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-update-inline-null-id-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-update-missing-id-invalid-variable',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'draft-order-detail-read',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'draft-orders-catalog-read',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'draft-orders-count-read',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-create-live-parity',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'draft-order-create-missing-input-invalid-variable',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'draft-order-create-live-parity',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-edit-begin-missing-id-invalid-variable',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-edit-add-variant-missing-id-invalid-variable',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'order-edit-begin-live-parity',
          state: 'blocked-with-proxy-request',
          blocker: expect.objectContaining({
            kind: 'missing-live-order-edit-begin-access',
            blockerPath: 'pending/order-editing-conformance-scope-blocker.md',
            details: expect.objectContaining({
              requiredScopes: ['write_order_edits'],
              probeRoots: ['orderEditBegin'],
              blockedMutations: ['orderEditBegin'],
              failingMessage: expect.stringContaining('write_order_edits'),
              activeCredentialTokenFamily: 'shpca',
              activeCredentialHeaderMode: 'raw-x-shopify-access-token',
              activeCredentialSummary: expect.stringContaining('shpca'),
              manualStoreAuthStatus: 'present-shpca-user-token-not-offline-capable',
              manualStoreAuthTokenPath: '.manual-store-auth-token.json',
              manualStoreAuthCachedScopes: expect.arrayContaining(['write_orders']),
              manualStoreAuthAssociatedUserScopes: [],
            }),
          }),
        }),
        expect.objectContaining({
          scenarioId: 'fulfillment-tracking-info-update-inline-missing-id-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'fulfillment-tracking-info-update-inline-null-id-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'fulfillment-tracking-info-update-missing-id-invalid-variable',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'fulfillment-cancel-inline-missing-id-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'fulfillment-cancel-inline-null-id-argument-error',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'fulfillment-cancel-missing-id-invalid-variable',
          state: 'ready-for-comparison',
          blocker: null,
        }),
        expect.objectContaining({
          scenarioId: 'fulfillment-tracking-info-update-live-parity',
          state: 'blocked-with-proxy-request',
          blocker: expect.objectContaining({
            kind: 'missing-live-fulfillment-tracking-update-access',
            blockerPath: 'pending/fulfillment-lifecycle-conformance-scope-blocker.md',
            details: expect.objectContaining({
              requiredScopes: [
                'write_assigned_fulfillment_orders',
                'write_merchant_managed_fulfillment_orders',
                'write_third_party_fulfillment_orders',
              ],
              requiredPermissions: ['fulfill_and_ship_orders'],
              probeRoots: ['fulfillmentTrackingInfoUpdate'],
              blockedMutations: ['fulfillmentTrackingInfoUpdate'],
              failingMessage: expect.stringContaining('fulfill and ship orders'),
              activeCredentialTokenFamily: 'shpca',
              activeCredentialHeaderMode: 'raw-x-shopify-access-token',
              activeCredentialSummary: expect.stringContaining('shpca'),
              manualStoreAuthStatus: 'present-shpca-user-token-not-offline-capable',
              manualStoreAuthTokenPath: '.manual-store-auth-token.json',
              manualStoreAuthCachedScopes: expect.arrayContaining(['write_fulfillments']),
            }),
          }),
        }),
        expect.objectContaining({
          scenarioId: 'fulfillment-cancel-live-parity',
          state: 'blocked-with-proxy-request',
          blocker: expect.objectContaining({
            kind: 'missing-live-fulfillment-cancel-access',
            blockerPath: 'pending/fulfillment-lifecycle-conformance-scope-blocker.md',
            details: expect.objectContaining({
              probeRoots: ['fulfillmentCancel'],
              blockedMutations: ['fulfillmentCancel'],
              failingMessage: expect.stringContaining('Access denied for fulfillmentCancel field.'),
              activeCredentialTokenFamily: 'shpca',
              activeCredentialHeaderMode: 'raw-x-shopify-access-token',
              activeCredentialSummary: expect.stringContaining('shpca'),
              manualStoreAuthStatus: 'present-shpca-user-token-not-offline-capable',
              manualStoreAuthTokenPath: '.manual-store-auth-token.json',
              manualStoreAuthCachedScopes: expect.arrayContaining(['write_fulfillments']),
            }),
          }),
        }),
      ]),
    );

    expect(markdown).toContain('# Conformance Parity Status');
    expect(markdown).toContain('Ready for comparison: 93');
    expect(markdown).toContain('Blocked scenarios: 9');
    expect(markdown).toContain('Pending scenarios: 0');
    expect(markdown).toContain('`customer-detail-parity-plan` (`customer`) → `ready-for-comparison`');
    expect(markdown).toContain('`customers-catalog-parity-plan` (`customers`) → `ready-for-comparison`');
    expect(markdown).toContain('`productPublish-parity-plan` (`productPublish`) → `ready-for-comparison`');
    expect(markdown).toContain('`productUnpublish-parity-plan` (`productUnpublish`) → `ready-for-comparison`');
    expect(markdown).toContain('`productPublish-aggregate-parity-blocker`');
    expect(markdown).toContain('`productUnpublish-aggregate-parity-blocker`');
    expect(markdown).toContain('`order-empty-state-read` (`order`, `orders`, `ordersCount`) → `ready-for-comparison`');
    expect(markdown).toContain('`order-create-inline-missing-order-argument-error` (`orderCreate`) → `ready-for-comparison`');
    expect(markdown).toContain('`order-create-inline-null-order-argument-error` (`orderCreate`) → `ready-for-comparison`');
    expect(markdown).toContain('`order-update-missing-id-invalid-variable` (`orderUpdate`) → `ready-for-comparison`');
    expect(markdown).toContain('`order-update-live-parity` (`orderUpdate`) → `ready-for-comparison`');
    expect(markdown).toContain('`draft-order-detail-read` (`draftOrder`) → `ready-for-comparison`');
    expect(markdown).toContain('`draft-orders-catalog-read` (`draftOrders`) → `ready-for-comparison`');
    expect(markdown).toContain('`draft-orders-count-read` (`draftOrdersCount`) → `ready-for-comparison`');
    expect(markdown).toContain('`order-create-live-parity` (`orderCreate`) → `ready-for-comparison`');
    expect(markdown).toContain('`draft-order-create-missing-input-invalid-variable` (`draftOrderCreate`) → `ready-for-comparison`');
    expect(markdown).toContain('`draft-order-create-live-parity` (`draftOrderCreate`) → `ready-for-comparison`');
    expect(markdown).toContain('`draft-order-complete-inline-missing-id-argument-error` (`draftOrderComplete`) → `ready-for-comparison`');
    expect(markdown).toContain('`draft-order-complete-inline-null-id-argument-error` (`draftOrderComplete`) → `ready-for-comparison`');
    expect(markdown).toContain('`draft-order-complete-missing-id-invalid-variable` (`draftOrderComplete`) → `ready-for-comparison`');
    expect(markdown).toContain('`draft-order-complete-live-parity` (`draftOrderComplete`) → `blocked-with-proxy-request`');
    expect(markdown).toContain('`order-edit-begin-missing-id-invalid-variable` (`orderEditBegin`) → `ready-for-comparison`');
    expect(markdown).toContain('`order-edit-add-variant-missing-id-invalid-variable` (`orderEditAddVariant`) → `ready-for-comparison`');
    expect(markdown).toContain('`order-edit-begin-live-parity` (`orderEditBegin`) → `blocked-with-proxy-request`');
    expect(markdown).toContain('required permissions: `mark-as-paid`, `set-payment-terms`');
    expect(markdown).not.toContain('`inventory-linkage-multi-location-blocker`');
    expect(markdown).toContain('blocked-with-proxy-request');
    expect(markdown).toContain('pending/product-publication-conformance-scope-blocker.md');
    expect(markdown).toContain('manual store auth: `present-shpca-user-token-not-offline-capable` @ `.manual-store-auth-token.json`');
    expect(markdown).toContain('manual store auth cached scopes: `read_product_listings`');
    expect(markdown).not.toContain('pending/inventory-linkage-single-location-blocker.md');
    expect(markdown).toContain(
      'blocked fields: `publishedOnCurrentPublication`, `availablePublicationsCount`, `resourcePublicationsCount`',
    );
    expect(markdown).toContain('blocked mutations: `productPublish`, `productUnpublish`');
    expect(markdown).toContain('configured app: `hermes-conformance-products` (`0db6d7e08e4ba05ce97440df36c7ed33`)');
    expect(markdown).toContain('app config: `/tmp/shopify-conformance-app/hermes-conformance-products/shopify.app.toml`');
    expect(markdown).toContain("publication target: `app-missing-publication` — Your app doesn't have a publication for this shop.");
    expect(markdown).toContain(
      'Shopify app CLI auth: `available` @ `/tmp/shopify-conformance-app/hermes-conformance-products`',
    );
    expect(markdown).toContain(
      'Shopify app deploy: `deployed-but-app-still-lacks-publication` via `corepack pnpm exec shopify app deploy --allow-updates` (`hermes-conformance-products-',
    );
    expect(markdown).toContain(
      'channel config extension: `conformance-publication-target` @ `/tmp/shopify-conformance-app/hermes-conformance-products/extensions/conformance-publication-target/shopify.extension.toml`',
    );
    expect(markdown).toContain('createLegacyChannelOnAppInstall: `true`');
    expect(markdown).toContain('publication remediation: `channel-config-change-still-needs-reinstall`');
    expect(markdown).toContain('credential family: `shpca`');
    expect(markdown).toContain('credential header mode: `raw-x-shopify-access-token`');
  });
});
