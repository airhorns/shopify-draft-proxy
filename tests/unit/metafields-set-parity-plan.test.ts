import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('metafieldsSet parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged product metafield write slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/metafieldsSet-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/metafieldsSet-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/metafieldsSet-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      metafields?: Array<Record<string, unknown>>;
    };

    expect(document).toContain('mutation MetafieldsSetParityPlan($metafields: [MetafieldsSetInput!]!)');
    expect(document).toContain('metafieldsSet(metafields: $metafields)');
    expect(document).toContain('metafields {');
    expect(document).toContain('id');
    expect(document).toContain('namespace');
    expect(document).toContain('key');
    expect(document).toContain('type');
    expect(document).toContain('value');
    expect(document).toContain('userErrors {');

    expect(variables.metafields).toEqual([
      {
        ownerId: 'gid://shopify/Product/8397256720617',
        namespace: 'custom',
        key: 'material',
        type: 'single_line_text_field',
        value: 'Canvas',
      },
      {
        ownerId: 'gid://shopify/Product/8397256720617',
        namespace: 'details',
        key: 'origin',
        type: 'single_line_text_field',
        value: 'VN',
      },
    ]);
  });
});
