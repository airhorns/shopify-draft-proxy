import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('metafieldsDelete parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the live Shopify plural metafield delete root', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/metafieldsDelete-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/metafieldsDelete-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/metafieldsDelete-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      metafields?: Array<{ ownerId?: string; namespace?: string; key?: string }>;
    };

    expect(document).toContain('mutation MetafieldsDeleteParityPlan($metafields: [MetafieldIdentifierInput!]!)');
    expect(document).toContain('metafieldsDelete(metafields: $metafields)');
    expect(document).toContain('deletedMetafields {');
    expect(document).toContain('ownerId');
    expect(document).toContain('namespace');
    expect(document).toContain('key');

    expect(variables.metafields).toEqual([
      {
        ownerId: 'gid://shopify/Product/9255391559913',
        namespace: 'custom',
        key: 'material',
      },
    ]);
  });
});
