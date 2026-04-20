import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productVariantsBulkCreate parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged variant bulk-create slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productVariantsBulkCreate-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe(
      'config/parity-requests/productVariantsBulkCreate-parity-plan.graphql',
    );
    expect(spec.proxyRequest?.variablesPath).toBe(
      'config/parity-requests/productVariantsBulkCreate-parity-plan.variables.json',
    );

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      productId?: string;
      variants?: Array<Record<string, unknown>>;
    };

    expect(document).toContain(
      'mutation ProductVariantsBulkCreateParityPlan($productId: ID!, $variants: [ProductVariantsBulkInput!]!)',
    );
    expect(document).toContain('productVariantsBulkCreate(productId: $productId, variants: $variants)');
    expect(document).toContain('product {');
    expect(document).toContain('totalInventory');
    expect(document).toContain('tracksInventory');
    expect(document).toContain('variants(first: 10) {');
    expect(document).toContain('productVariants {');
    expect(document).toContain('inventoryItem {');
    expect(document).toContain('requiresShipping');
    expect(document).toContain('userErrors {');

    expect(variables.productId).toBe('gid://shopify/Product/100');
    expect(variables.variants).toEqual([
      {
        optionValues: [{ optionName: 'Color', name: 'Blue' }],
        barcode: '2222222222222',
        price: '26.00',
        inventoryQuantities: [{ availableQuantity: 6, locationId: 'gid://shopify/Location/1' }],
        inventoryItem: {
          sku: 'HAT-BLUE',
          tracked: true,
          requiresShipping: false,
        },
      },
    ]);
  });
});
