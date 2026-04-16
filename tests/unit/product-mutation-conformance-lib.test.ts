import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

// scripts/ is intentionally outside tsconfig's checked sources; runtime coverage here verifies the JS helper.
// @ts-expect-error local .mjs helper is exercised via Vitest rather than TS declarations
import { parseWriteScopeBlocker, pickProductMutationSeed, renderWriteScopeBlockerNote } from '../../scripts/product-mutation-conformance-lib.mjs';

describe('pickProductMutationSeed', () => {
  it('returns the first product node from the products catalog capture', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const catalogFixture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products-catalog-page.json'),
        'utf8',
      ),
    ) as {
      data?: {
        products?: {
          edges?: Array<{
            node?: { id?: string; title?: string; handle?: string; status?: string; vendor?: string; productType?: string };
          }>;
        };
      };
    };

    expect(pickProductMutationSeed(catalogFixture)).toEqual({
      id: 'gid://shopify/Product/8397256720617',
      title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID',
      handle: 'converse-toddler-chuck-taylor-all-star-axel-mid',
      status: 'ACTIVE',
      vendor: 'CONVERSE',
      productType: 'SHOES',
    });
  });

  it('throws when the catalog capture does not expose any valid product nodes', () => {
    expect(() => pickProductMutationSeed({ data: { products: { edges: [] } } })).toThrow(
      'Could not find a sample product from ProductCatalogPage capture',
    );
  });
});

describe('parseWriteScopeBlocker', () => {
  it('extracts write-scope blocker details from a Shopify GraphQL access denied payload', () => {
    const blocker = parseWriteScopeBlocker({
      status: 200,
      payload: {
        errors: [
          {
            message:
              'Access denied for productCreate field. Required access: `write_products` access scope. Also: The user must have a permission to create products.',
            extensions: {
              code: 'ACCESS_DENIED',
              requiredAccess: '`write_products` access scope. Also: The user must have a permission to create products.',
            },
            path: ['productCreate'],
          },
        ],
      },
    });

    expect(blocker).toEqual({
      operationName: 'productCreate',
      message:
        'Access denied for productCreate field. Required access: `write_products` access scope. Also: The user must have a permission to create products.',
      requiredAccess: '`write_products` access scope. Also: The user must have a permission to create products.',
      errorCode: 'ACCESS_DENIED',
    });
  });

  it('returns null when the payload is not a GraphQL access denied response', () => {
    expect(parseWriteScopeBlocker({ status: 200, payload: { data: { shop: { name: 'ok' } } } })).toBeNull();
  });
});

describe('renderWriteScopeBlockerNote', () => {
  it('renders a concise blocker note for a product mutation family', () => {
    const note = renderWriteScopeBlockerNote({
      title: 'Product mutation conformance blocker',
      whatFailed:
        'Attempted to capture live conformance for the staged product mutation family (`productCreate`, `productUpdate`, `productDelete`).',
      operations: ['productCreate', 'productUpdate', 'productDelete'],
      blocker: {
        operationName: 'productCreate',
        message:
          'Access denied for productCreate field. Required access: `write_products` access scope. Also: The user must have a permission to create products.',
        requiredAccess: '`write_products` access scope. Also: The user must have a permission to create products.',
        errorCode: 'ACCESS_DENIED',
      },
      whyBlocked:
        'Without a write-capable token, the repo cannot capture successful live mutation payloads or immediate downstream read-after-write parity for this family.',
      completedSteps: [
        'added a reusable live-write capture harness for the family',
        'kept the next safe capture step explicit for a future write-capable token',
      ],
      recommendedNextStep:
        'Switch the repo conformance credential to a safe dev-store token with `write_products`, then rerun the product mutation capture script.',
    });

    expect(note).toContain('# Product mutation conformance blocker');
    expect(note).toContain('- `productCreate`');
    expect(note).toContain('- `productUpdate`');
    expect(note).toContain('- `productDelete`');
    expect(note).toContain('- `ACCESS_DENIED`');
    expect(note).toContain('- required access: `write_products` access scope. Also: The user must have a permission to create products.');
    expect(note).toContain('Switch the repo conformance credential to a safe dev-store token with `write_products`');
  });
});
