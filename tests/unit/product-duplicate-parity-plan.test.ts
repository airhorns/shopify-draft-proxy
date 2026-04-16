import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productDuplicate parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged productDuplicate slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productDuplicate-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productDuplicate-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/productDuplicate-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      productId?: string;
      newTitle?: string;
    };

    expect(document).toContain('mutation ProductDuplicateParityPlan($productId: ID!, $newTitle: String!)');
    expect(document).toContain('productDuplicate(productId: $productId, newTitle: $newTitle)');
    expect(document).toContain('newProduct {');
    expect(document).toContain('options {');
    expect(document).toContain('variants(first: 10) {');
    expect(document).toContain('collections(first: 10) {');
    expect(document).toContain('media(first: 10) {');
    expect(document).toContain('metafield(namespace: "custom", key: "material") {');
    expect(document).toContain('metafields(first: 10) {');

    expect(variables).toMatchObject({
      productId: 'gid://shopify/Product/100',
      newTitle: 'Copied Shoe',
    });
  });
});
