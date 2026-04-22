import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('tagsAdd parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged product tag-add slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/tagsAdd-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: {
        documentPath?: string | null;
        variablesPath?: string | null;
        variablesCapturePath?: string | null;
      };
      comparison?: {
        mode?: string;
        targets?: Array<{ name?: string; proxyRequest?: { documentPath?: string | null } }>;
        expectedDifferences?: unknown[];
      };
      blocker?: unknown;
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/tagsAdd-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/tagsAdd-parity-plan.variables.json');
    expect(spec.proxyRequest?.variablesCapturePath).toBe('$.mutation.variables');
    expect(spec.blocker).toBeUndefined();

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);
    const downstreamTarget = spec.comparison?.targets?.find((target) => target.name === 'downstream-read-data');
    const downstreamDocumentPath = resolve(repoRoot, downstreamTarget!.proxyRequest!.documentPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);
    expect(existsSync(downstreamDocumentPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const downstreamDocument = readFileSync(downstreamDocumentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      id?: string;
      tags?: string[];
    };

    expect(document).toContain('mutation TagsAddParityPlan($id: ID!, $tags: [String!]!)');
    expect(document).toContain('tagsAdd(id: $id, tags: $tags)');
    expect(document).toContain('... on Product');
    expect(document).toContain('id');
    expect(document).toContain('tags');
    expect(document).toContain('userErrors {');
    expect(downstreamDocument).toContain('query TagsAddDownstreamRead($id: ID!, $query: String!)');
    expect(downstreamDocument).toContain('product(id: $id)');
    expect(downstreamDocument).toContain('products(first: 10, query: $query)');
    expect(downstreamDocument).toContain('productsCount(query: $query)');

    expect(variables).toEqual({
      id: 'gid://shopify/Product/8397256720617',
      tags: ['existing', 'summer', 'sale'],
    });

    expect(spec.comparison?.mode).toBe('strict-json');
    expect(spec.comparison?.expectedDifferences).toEqual([]);
    expect(spec.comparison?.targets?.map((target) => target.name)).toEqual(['mutation-data', 'downstream-read-data']);
  });
});
