import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productUpdate parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged productUpdate slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productUpdate-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productUpdate-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/productUpdate-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      product?: Record<string, unknown>;
    };

    expect(document).toContain('mutation ProductUpdateParityPlan($product: ProductUpdateInput!)');
    expect(document).toContain('productUpdate(product: $product)');
    expect(document).toContain('onlineStorePreviewUrl');
    expect(document).toContain('seo {');

    expect(variables.product).toMatchObject({
      id: 'gid://shopify/Product/8397256720617',
      title: 'CONVERSE | TODDLER CHUCK TAYLOR ALL STAR AXEL MID (Parity Draft Rename)',
      vendor: 'CONVERSE',
      productType: 'SHOES',
      tags: ['conformance', 'parity-plan'],
      templateSuffix: 'parity-plan',
      descriptionHtml: '<p>Parity plan update scaffold</p>',
      seo: {
        title: 'Parity plan SEO',
        description: 'Parity plan SEO description',
      },
    });
  });
});
