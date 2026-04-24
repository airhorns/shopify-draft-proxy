import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

// scripts/ is intentionally exercised through Vitest runtime coverage.
import { pickCollectionCaptureSeed } from '../../scripts/collection-conformance-lib.mjs';

describe('pickCollectionCaptureSeed', () => {
  it('returns the first collection node from a product-scoped payload', () => {
    expect(
      pickCollectionCaptureSeed({
        data: {
          product: {
            collections: {
              edges: [
                {
                  node: {
                    id: 'gid://shopify/Collection/1',
                    title: 'Frontpage',
                    handle: 'frontpage',
                  },
                },
              ],
            },
          },
        },
      }),
    ).toEqual({
      id: 'gid://shopify/Collection/1',
      title: 'Frontpage',
      handle: 'frontpage',
    });
  });

  it('returns a structurally valid collection seed from the current collection-seed query shape', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const detailFixture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/collection-detail.json'),
        'utf8',
      ),
    ) as {
      data?: {
        customCollection?: {
          id?: string;
          title?: string;
          handle?: string;
        };
      };
    };

    expect(
      pickCollectionCaptureSeed({
        data: {
          products: {
            edges: [
              {
                node: {
                  id: 'gid://shopify/Product/seed',
                  title: 'Seed product',
                  collections: {
                    edges: [
                      {
                        node: {
                          id: detailFixture.data?.customCollection?.id,
                          title: detailFixture.data?.customCollection?.title,
                          handle: detailFixture.data?.customCollection?.handle,
                        },
                      },
                    ],
                  },
                },
              },
            ],
          },
        },
      }),
    ).toMatchObject({
      id: expect.stringMatching(/^gid:\/\/shopify\/Collection\//),
      title: expect.any(String),
      handle: expect.any(String),
    });
  });

  it('throws when the product detail capture does not expose any collections', () => {
    expect(() => pickCollectionCaptureSeed({ data: { product: { collections: { edges: [] } } } })).toThrow(
      'Could not find a sample collection from ProductDetail capture',
    );
  });
});
