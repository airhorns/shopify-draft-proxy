import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('tagsAdd parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged product tag-add slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/tagsAdd-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/tagsAdd-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/tagsAdd-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
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

    expect(variables).toEqual({
      id: 'gid://shopify/Product/8397256720617',
      tags: ['existing', 'summer', 'sale'],
    });
  });
});
