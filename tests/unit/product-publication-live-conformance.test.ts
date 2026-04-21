import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { parseAccessDeniedErrors } from '../../scripts/product-mutation-conformance-lib.mjs';

describe('product publication live conformance harness', () => {
  it('removes a stale publication scope blocker note once capture can succeed', async () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const tempRoot = resolve(repoRoot, '.tmp-test-publication-blocker');
    const fs = await import('node:fs/promises');

    await fs.mkdir(tempRoot, { recursive: true });
    const blockerPath = resolve(tempRoot, 'product-publication-conformance-scope-blocker.md');
    await fs.writeFile(blockerPath, 'stale blocker\n', 'utf8');

    try {
      const { clearPublicationScopeBlocker } = await import('../../scripts/product-publication-conformance-lib.mjs');
      await clearPublicationScopeBlocker(blockerPath);
      expect(existsSync(blockerPath)).toBe(false);

      await clearPublicationScopeBlocker(blockerPath);
      expect(existsSync(blockerPath)).toBe(false);
    } finally {
      await fs.rm(tempRoot, { recursive: true, force: true });
    }
  });

  it('extracts multiple publication access blockers from a single GraphQL error payload', () => {
    const blockers = parseAccessDeniedErrors({
      payload: {
        errors: [
          {
            message:
              'Access denied for publishedOnCurrentPublication field. Required access: `read_product_listings` access scope.',
            path: ['product', 'publishedOnCurrentPublication'],
            extensions: {
              code: 'ACCESS_DENIED',
              requiredAccess: '`read_product_listings` access scope.',
            },
          },
          {
            message: 'Access denied for publications field. Required access: `read_publications` access scope.',
            path: ['publications'],
            extensions: {
              code: 'ACCESS_DENIED',
              requiredAccess: '`read_publications` access scope.',
            },
          },
        ],
      },
    });

    expect(blockers).toEqual([
      {
        operationName: 'product',
        message:
          'Access denied for publishedOnCurrentPublication field. Required access: `read_product_listings` access scope.',
        requiredAccess: '`read_product_listings` access scope.',
        errorCode: 'ACCESS_DENIED',
      },
      {
        operationName: 'publications',
        message: 'Access denied for publications field. Required access: `read_publications` access scope.',
        requiredAccess: '`read_publications` access scope.',
        errorCode: 'ACCESS_DENIED',
      },
    ]);
  });

  it('keeps a dedicated publication capture script before the family can be promoted', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const scriptPath = resolve(repoRoot, 'scripts/capture-product-publication-conformance.mts');

    expect(existsSync(scriptPath)).toBe(true);

    const script = readFileSync(scriptPath, 'utf8');
    expect(script).toContain('query ProductPublicationScopeHandles');
    expect(script).toContain('query ProductPublicationAggregateProbe');
    expect(script).toContain('query ProductPublicationListProbe');
    expect(script).toContain('publishedOnCurrentPublication');
    expect(script).toContain('availablePublicationsCount');
    expect(script).toContain('resourcePublicationsCount');
    expect(script).toContain('publications(first: 10)');
    expect(script).toContain('edges {');
    expect(script).toContain('cursor');
    expect(script).toContain('pageInfo {');
    expect(script).toContain('startCursor');
    expect(script).toContain('endCursor');
    expect(script).toContain('productPublish(input: $input)');
    expect(script).toContain('productUnpublish(input: $input)');
    expect(script).toContain('collectPublicationMutationScopeProbe');
    expect(script).toContain('publishMutationScopeProbe');
    expect(script).toContain('unpublishMutationScopeProbe');
    expect(script).toContain("publicationMutationScopeProbePublicationId = 'gid://shopify/Publication/1'");
    expect(script).toContain('product-publication-conformance-scope-blocker.md');
    expect(script).toContain('clearPublicationScopeBlocker(blockerPath)');
    expect(script).toContain('getDefaultShopifyCliConfigPath');
    expect(script).toContain('tryShopifyCliPublicationFallback');
    expect(script).toContain('probeShopifyAppCliAuth');
    expect(script).toContain('shouldAttemptShopifyAppDeploy');
    expect(script).toContain('attemptShopifyAppDeploy');
    expect(script).toContain('extractShopifyAppDeployVersion');
    expect(script).toContain('parsePublicationTargetBlocker');
    expect(script).toContain('publicationTargetBlocker');
    expect(script).toContain('corepack pnpm exec shopify app info --json');
    expect(script).toContain('corepack pnpm exec shopify app deploy --allow-updates');
    expect(script).toContain('deployed-but-still-scope-blocked');
    expect(script).toContain('invalid-grant');
    expect(script).toContain('dedicated Admin API token (`shpat_...`)');
    expect(script).toContain('findConfiguredShopifyApp');
    expect(script).toContain('extractScopesFromShopifyAppToml');
    expect(script).toContain('extractManualStoreAuthTokenSummary');
    expect(script).toContain('.manual-store-auth-token.json');
    expect(script).toContain('manualStoreAuthStatus');
    expect(script).toContain('tryManualStoreAuthPublicationFallback');
    expect(script).toContain('scope-blocked');
    expect(script).toContain('missingRequestedScopes');
    expect(script).toContain('shopify.app.toml');
    expect(script).toContain('re-authorize the app / token with the missing publication scopes');
  });
});
