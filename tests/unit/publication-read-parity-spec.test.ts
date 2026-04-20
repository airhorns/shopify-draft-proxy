import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('publication read parity spec', () => {
  it('declares a concrete proxy request scaffold for publications parity', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/publications-catalog-read.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/publications-catalog-read.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/publications-catalog-read.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as { first?: number };

    expect(document).toContain('query PublicationsCatalogRead($first: Int!)');
    expect(document).toContain('publications(first: $first)');
    expect(document).toContain('edges');
    expect(document).toContain('cursor');
    expect(document).toContain('node {');
    expect(document).toContain('id');
    expect(document).toContain('name');
    expect(document).toContain('pageInfo {');
    expect(variables.first).toBe(10);
  });
});
