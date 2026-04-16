import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

// scripts/ is intentionally outside tsconfig's checked sources; runtime coverage here verifies the JS helper.
// @ts-expect-error local .mjs helper is exercised via Vitest rather than TS declarations
import { pickCollectionCaptureSeed } from '../../scripts/collection-conformance-lib.mjs';

describe('pickCollectionCaptureSeed', () => {
  it('returns the first collection node from a product detail capture', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const detailFixture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-detail.json'),
        'utf8',
      ),
    ) as {
      data?: {
        product?: {
          collections?: {
            edges?: Array<{ node?: { id?: string; title?: string; handle?: string } }>;
          };
        };
      };
    };

    expect(pickCollectionCaptureSeed(detailFixture)).toEqual({
      id: 'gid://shopify/Collection/429826244841',
      title: 'CONVERSE',
      handle: 'converse',
    });
  });

  it('throws when the product detail capture does not expose any collections', () => {
    expect(() => pickCollectionCaptureSeed({ data: { product: { collections: { edges: [] } } } })).toThrow(
      'Could not find a sample collection from ProductDetail capture',
    );
  });
});
