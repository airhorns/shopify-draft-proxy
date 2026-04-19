import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productOptionsCreate parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged option-create slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productOptionsCreate-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productOptionsCreate-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe(
      'config/parity-requests/productOptionsCreate-parity-plan.variables.json',
    );

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      productId?: string;
      options?: Array<Record<string, unknown>>;
    };

    expect(document).toContain(
      'mutation ProductOptionsCreateParityPlan($productId: ID!, $options: [OptionCreateInput!]!)',
    );
    expect(document).toContain('productOptionsCreate(productId: $productId, options: $options)');
    expect(document).toContain('product {');
    expect(document).toContain('options {');
    expect(document).toContain('optionValues {');
    expect(document).toContain('userErrors {');

    expect(variables.productId).toBe('gid://shopify/Product/100');
    expect(variables.options).toEqual([
      {
        name: 'Color',
        position: 1,
        values: [{ name: 'Red' }],
      },
    ]);
  });
});
