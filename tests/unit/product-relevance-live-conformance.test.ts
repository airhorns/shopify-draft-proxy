import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

type OperationRegistryEntry = {
  name: string;
  conformance?: {
    scenarioIds?: string[];
  };
};

type ConformanceScenario = {
  id: string;
  status: string;
  operationNames: string[];
  captureFiles: string[];
  paritySpecPath: string;
};

describe('product relevance live conformance wiring', () => {
  it('registers a captured products-only relevance scenario with a concrete proxy request scaffold', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const scenarios = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/conformance-scenarios.json'), 'utf8'),
    ) as ConformanceScenario[];

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'products',
        conformance: expect.objectContaining({
          scenarioIds: expect.arrayContaining(['products-relevance-search-read']),
        }),
      }),
    );
    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'productsCount',
        conformance: expect.objectContaining({
          scenarioIds: expect.not.arrayContaining(['products-relevance-search-read']),
        }),
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'products-relevance-search-read',
        status: 'captured',
        operationNames: ['products'],
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products-relevance-search.json'],
        paritySpecPath: 'config/parity-specs/products-relevance-search-read.json',
      }),
    );
  });

  it('keeps the relevance parity request focused on opaque cursor replay under sortKey RELEVANCE', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const document = readFileSync(
      resolve(repoRoot, 'config/parity-requests/products-relevance-search-read.graphql'),
      'utf8',
    );
    const variables = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'config/parity-requests/products-relevance-search-read.variables.json'),
        'utf8',
      ),
    ) as Record<string, unknown>;

    expect(document).toContain('query ProductsRelevanceSearchRead($first: Int!, $query: String!)');
    expect(document).toContain('products(first: $first, query: $query, sortKey: RELEVANCE)');
    expect(document).toContain('cursor');
    expect(document).toContain('legacyResourceId');
    expect(document).toContain('pageInfo {');
    expect(variables).toMatchObject({
      first: 3,
      query: 'swoo* status:active',
    });
  });
});
