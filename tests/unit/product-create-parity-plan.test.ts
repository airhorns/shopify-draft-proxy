import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productCreate parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged rich productCreate slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productCreate-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
      comparison?: { mode?: string; targets?: Array<{ name?: string }>; allowedDifferences?: Array<{ path?: string }> };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productCreate-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/productCreate-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      product?: Record<string, unknown> & {
        seo?: Record<string, unknown>;
      };
    };

    expect(document).toContain('mutation ProductCreateParityPlan($product: ProductCreateInput!)');
    expect(document).toContain('productCreate(product: $product)');
    expect(document).toContain('vendor');
    expect(document).toContain('productType');
    expect(document).toContain('tags');
    expect(document).toContain('descriptionHtml');
    expect(document).toContain('templateSuffix');
    expect(document).toContain('seo {');

    expect(variables.product).toMatchObject({
      title: 'Hermes Product Conformance 1776299742511',
      status: 'DRAFT',
      vendor: 'HERMES',
      productType: 'ACCESSORIES',
      tags: ['1776299742511', 'conformance', 'product-mutation'],
      descriptionHtml: '<p>Hermes product mutation conformance 1776299742511</p>',
      templateSuffix: 'product-mutation-parity',
      seo: {
        title: 'Hermes Product 1776299742511',
        description: 'Hermes product mutation conformance 1776299742511',
      },
    });

    expect(spec.comparison?.mode).toBe('strict-json');
    expect(spec.comparison?.targets?.map((target) => target.name)).toEqual(['mutation-data', 'downstream-read-data']);
    expect(spec.comparison?.allowedDifferences?.map((difference) => difference.path)).toEqual([
      '$.productCreate.product.id',
      '$.product.id',
    ]);
  });
});
