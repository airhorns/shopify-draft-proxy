import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productOptionUpdate parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged option-update slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productOptionUpdate-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productOptionUpdate-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/productOptionUpdate-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      productId?: string;
      option?: Record<string, unknown>;
      optionValuesToAdd?: Array<Record<string, unknown>>;
      optionValuesToUpdate?: Array<Record<string, unknown>>;
    };

    expect(document).toContain(
      'mutation ProductOptionUpdateParityPlan($productId: ID!, $option: OptionUpdateInput!, $optionValuesToAdd: [OptionValueCreateInput!], $optionValuesToUpdate: [OptionValueUpdateInput!])',
    );
    expect(document).toContain(
      'productOptionUpdate(productId: $productId, option: $option, optionValuesToAdd: $optionValuesToAdd, optionValuesToUpdate: $optionValuesToUpdate)',
    );
    expect(document).toContain('product {');
    expect(document).toContain('options {');
    expect(document).toContain('values');
    expect(document).toContain('optionValues {');
    expect(document).toContain('hasVariants');
    expect(document).toContain('userErrors {');

    expect(variables.productId).toBe('gid://shopify/Product/100');
    expect(variables.option).toEqual({
      id: 'gid://shopify/ProductOption/200',
      name: 'Shade',
      position: 2,
    });
    expect(variables.optionValuesToAdd).toEqual([{ name: 'Blue' }]);
    expect(variables.optionValuesToUpdate).toEqual([
      {
        id: 'gid://shopify/ProductOptionValue/300',
        name: 'Crimson',
      },
    ]);
  });
});
