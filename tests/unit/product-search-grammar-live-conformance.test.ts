import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

type OperationRegistryEntry = {
  name: string;
};

type ConformanceScenario = {
  id: string;
  status: string;
  operationNames: string[];
  captureFiles: string[];
  paritySpecPath: string;
  notes?: string;
};

type ProductSearchGrammarCapture = {
  variables?: {
    phraseQuery?: string;
  };
  response?: {
    data?: {
      phraseMatches?: {
        edges?: Array<{
          node?: {
            id?: string;
            title?: string;
            vendor?: string;
            productType?: string;
            tags?: string[];
          };
        }>;
      };
      phraseCount?: {
        count?: number;
        precision?: string;
      };
    };
  };
};

describe('product search grammar live conformance wiring', () => {
  it('registers a captured products/productsCount scenario for quoted phrases, bare terms, and negated filters', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'products',
      }),
    );
    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'productsCount',
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'products-search-grammar-read',
        status: 'captured',
        operationNames: ['products', 'productsCount'],
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products-search-grammar.json'],
        paritySpecPath: 'config/parity-specs/products-search-grammar-read.json',
      }),
    );
  });

  it('keeps the proxy request aligned with the quoted-phrase/bare-term/negation search slice and records the live capture', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const documentPath = resolve(repoRoot, 'config/parity-requests/products-search-grammar-read.graphql');
    const variablesPath = resolve(repoRoot, 'config/parity-requests/products-search-grammar-read.variables.json');
    const fixturePath = resolve(
      repoRoot,
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products-search-grammar.json',
    );

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);
    expect(existsSync(fixturePath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as Record<string, unknown>;
    const capture = JSON.parse(readFileSync(fixturePath, 'utf8')) as ProductSearchGrammarCapture;

    expect(document).toContain('query ProductsSearchGrammarRead($phraseQuery: String!)');
    expect(document).toContain('phraseMatches: products(first: 5, query: $phraseQuery)');
    expect(document).toContain('phraseCount: productsCount(query: $phraseQuery)');
    expect(document).toContain('tags');

    expect(variables).toMatchObject({
      phraseQuery: '"flat peak cap" accessories -vendor:VANS -tag:vans',
    });
    expect(capture.variables).toMatchObject({
      phraseQuery: '"flat peak cap" accessories -vendor:VANS -tag:vans',
    });
    expect(capture.response?.data).toMatchObject({
      phraseCount: {
        count: 1,
        precision: 'EXACT',
      },
      phraseMatches: {
        edges: [
          {
            node: {
              id: expect.any(String),
              title: expect.stringContaining('FLAT PEAK CAP'),
              vendor: 'NIKE',
              productType: 'ACCESSORIES',
              tags: expect.arrayContaining(['cap']),
            },
          },
        ],
      },
    });
  });
});
