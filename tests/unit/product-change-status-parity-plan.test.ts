import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productChangeStatus parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged status-only mutation slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productChangeStatus-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productChangeStatus-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/productChangeStatus-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      productId?: string;
      status?: string;
    };

    expect(document).toContain('mutation ProductChangeStatusParityPlan($productId: ID!, $status: ProductStatus!)');
    expect(document).toContain('productChangeStatus(productId: $productId, status: $status)');
    expect(document).toContain('product {');
    expect(document).toContain('id');
    expect(document).toContain('status');
    expect(document).toContain('updatedAt');
    expect(document).toContain('userErrors {');

    expect(variables).toEqual({
      productId: 'gid://shopify/Product/8397256720617',
      status: 'ARCHIVED',
    });
  });
});
