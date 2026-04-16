import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('collection read parity specs', () => {
  it('declares a concrete proxy request scaffold for collection detail parity', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/collection-detail-read.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/collection-detail-read.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/collection-detail-read.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as { id?: string };

    expect(document).toContain('query CollectionDetailRead($id: ID!)');
    expect(document).toContain('collection(id: $id)');
    expect(document).toContain('products(first: 3)');
    expect(document).toContain('productType');
    expect(document).toContain('totalInventory');
    expect(document).toContain('pageInfo {');
    expect(variables.id).toBe('gid://shopify/Collection/429826244841');
  });

  it('declares a concrete proxy request scaffold for collections catalog parity', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/collections-catalog-read.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/collections-catalog-read.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/collections-catalog-read.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as { first?: number };

    expect(document).toContain('query CollectionsCatalogRead($first: Int!)');
    expect(document).toContain('collections(first: $first)');
    expect(document).toContain('handle');
    expect(document).toContain('products(first: 2)');
    expect(document).toContain('vendor');
    expect(document).toContain('pageInfo {');
    expect(variables.first).toBe(3);
  });
});
