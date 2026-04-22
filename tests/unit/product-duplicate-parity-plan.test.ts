import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('productDuplicate parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged productDuplicate slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/productDuplicate-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      blocker?: unknown;
      comparison?: {
        targets?: Array<{
          name?: string;
          capturePath?: string;
          proxyPath?: string;
          proxyRequest?: { documentPath?: string | null };
        }>;
      };
      proxyRequest?: {
        documentPath?: string | null;
        variablesPath?: string | null;
        variablesCapturePath?: string | null;
      };
    };

    expect(spec.blocker).toBeUndefined();
    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/productDuplicate-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/productDuplicate-parity-plan.variables.json');
    expect(spec.proxyRequest?.variablesCapturePath).toBe('$.mutation.variables');
    expect(spec.comparison?.targets).toEqual([
      {
        name: 'mutation-data',
        capturePath: '$.mutation.response.data',
        proxyPath: '$.data',
      },
      {
        name: 'downstream-read-data',
        capturePath: '$.downstreamRead.data',
        proxyRequest: {
          documentPath: 'config/parity-requests/productDuplicate-downstream-read.graphql',
          variables: {
            id: {
              fromPrimaryProxyPath: '$.data.productDuplicate.newProduct.id',
            },
          },
        },
        proxyPath: '$.data',
      },
    ]);

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);
    const downstreamDocumentPath = resolve(repoRoot, spec.comparison!.targets![1]!.proxyRequest!.documentPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);
    expect(existsSync(downstreamDocumentPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const downstreamDocument = readFileSync(downstreamDocumentPath, 'utf8');
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
    expect(downstreamDocument).toContain('query ProductDuplicateDownstreamRead($id: ID!)');
    expect(downstreamDocument).toContain('product(id: $id)');
    expect(downstreamDocument).toContain('variants(first: 10) {');
    expect(downstreamDocument).toContain('metafields(first: 10) {');

    expect(variables).toMatchObject({
      productId: 'gid://shopify/Product/100',
      newTitle: 'Copied Shoe',
    });
  });
});
