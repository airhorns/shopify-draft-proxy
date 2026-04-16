import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productDelete parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged productDelete slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productDelete-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productDelete-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/productDelete-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      input?: Record<string, unknown>;
    };

    expect(document).toContain('mutation ProductDeleteParityPlan($input: ProductDeleteInput!)');
    expect(document).toContain('productDelete(input: $input)');
    expect(document).toContain('deletedProductId');
    expect(document).toContain('userErrors {');

    expect(variables.input).toMatchObject({
      id: 'gid://shopify/Product/8397256720617',
    });
  });
});
