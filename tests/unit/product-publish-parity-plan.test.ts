import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productPublish parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged product publication slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productPublish-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productPublish-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/productPublish-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      input?: {
        id?: string;
        productPublications?: Array<Record<string, unknown>>;
      };
    };

    expect(document).toContain('mutation ProductPublishParityPlan($input: ProductPublishInput!)');
    expect(document).toContain('productPublish(input: $input)');
    expect(document).toContain('product {');
    expect(document).toContain('publishedOnCurrentPublication');
    expect(document).toContain('availablePublicationsCount {');
    expect(document).toContain('resourcePublicationsCount {');
    expect(document).toContain('userErrors {');

    expect(variables).toEqual({
      input: {
        id: 'gid://shopify/Product/100',
        productPublications: [{ publicationId: 'gid://shopify/Publication/1' }],
      },
    });
  });
});
