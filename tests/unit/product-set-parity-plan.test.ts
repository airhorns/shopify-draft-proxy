import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productSet parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged productSet create slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productSet-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productSet-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/productSet-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      synchronous?: boolean;
      input?: Record<string, unknown> & {
        productOptions?: Array<Record<string, unknown>>;
        variants?: Array<Record<string, unknown>>;
        metafields?: Array<Record<string, unknown>>;
      };
    };

    expect(document).toContain('mutation ProductSetParityPlan($input: ProductSetInput!, $synchronous: Boolean!)');
    expect(document).toContain('productSet(input: $input, synchronous: $synchronous)');
    expect(document).toContain('productSetOperation {');
    expect(document).toContain('options {');
    expect(document).toContain('variants(first: 10) {');
    expect(document).toContain('metafields(first: 10) {');

    expect(variables.synchronous).toBe(true);
    expect(variables.input).toMatchObject({
      title: 'Parity Set Snowboard',
      status: 'DRAFT',
      vendor: 'BURTON',
      productType: 'SNOWBOARD',
      tags: ['parity-plan', 'winter'],
    });
    expect(variables.input?.productOptions).toEqual([
      {
        name: 'Color',
        position: 1,
        values: [{ name: 'Blue' }, { name: 'Black' }],
      },
    ]);
    expect(variables.input?.variants).toEqual([
      {
        optionValues: [{ optionName: 'Color', name: 'Blue' }],
        sku: 'PARITY-SET-BLUE',
        price: '79.99',
        inventoryQuantities: [{ quantity: 7 }],
        inventoryItem: { tracked: true, requiresShipping: true },
      },
      {
        optionValues: [{ optionName: 'Color', name: 'Black' }],
        sku: 'PARITY-SET-BLACK',
        price: '69.99',
        inventoryQuantities: [{ quantity: 3 }],
        inventoryItem: { tracked: false, requiresShipping: true },
      },
    ]);
    expect(variables.input?.metafields).toEqual([
      {
        namespace: 'custom',
        key: 'season',
        type: 'single_line_text_field',
        value: 'winter',
      },
    ]);
  });
});
