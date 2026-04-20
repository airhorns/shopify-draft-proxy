import { existsSync, readFileSync } from 'node:fs';
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
  notes?: string;
};

type ProductSearchPaginationCapture = {
  variables?: {
    query?: string;
    afterCursor?: string;
    beforeCursor?: string;
  };
  response?: {
    data?: {
      count?: {
        count?: number;
        precision?: string;
      };
      firstPage?: {
        edges?: Array<{ cursor?: string; node?: { id?: string; title?: string; updatedAt?: string } }>;
        pageInfo?: {
          hasNextPage?: boolean;
          hasPreviousPage?: boolean;
          startCursor?: string | null;
          endCursor?: string | null;
        };
      };
      nextPage?: {
        edges?: Array<{ cursor?: string; node?: { id?: string; title?: string; updatedAt?: string } }>;
        pageInfo?: {
          hasNextPage?: boolean;
          hasPreviousPage?: boolean;
          startCursor?: string | null;
          endCursor?: string | null;
        };
      };
      previousPage?: {
        edges?: Array<{ cursor?: string; node?: { id?: string; title?: string; updatedAt?: string } }>;
        pageInfo?: {
          hasNextPage?: boolean;
          hasPreviousPage?: boolean;
          startCursor?: string | null;
          endCursor?: string | null;
        };
      };
    };
  };
};

describe('product search pagination live conformance wiring', () => {
  it('registers a captured products/productsCount scenario for filtered cursor pagination windows', () => {
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
          scenarioIds: expect.arrayContaining(['products-search-pagination-read']),
        }),
      }),
    );
    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'productsCount',
        conformance: expect.objectContaining({
          scenarioIds: expect.arrayContaining(['products-search-pagination-read']),
        }),
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'products-search-pagination-read',
        status: 'captured',
        operationNames: ['products', 'productsCount'],
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products-search-pagination.json'],
        paritySpecPath: 'config/parity-specs/products-search-pagination-read.json',
      }),
    );
  });

  it('keeps the proxy request aligned with filtered forward/backward cursor pagination and records the live capture', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const documentPath = resolve(repoRoot, 'config/parity-requests/products-search-pagination-read.graphql');
    const variablesPath = resolve(repoRoot, 'config/parity-requests/products-search-pagination-read.variables.json');
    const fixturePath = resolve(
      repoRoot,
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products-search-pagination.json',
    );

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);
    expect(existsSync(fixturePath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as Record<string, unknown>;
    const capture = JSON.parse(readFileSync(fixturePath, 'utf8')) as ProductSearchPaginationCapture;

    expect(document).toContain('query ProductsSearchPaginationRead($query: String!, $afterCursor: String!, $beforeCursor: String!)');
    expect(document).toContain('count: productsCount(query: $query)');
    expect(document).toContain('firstPage: products(first: 1, query: $query, sortKey: UPDATED_AT, reverse: true)');
    expect(document).toContain('nextPage: products(first: 1, after: $afterCursor, query: $query, sortKey: UPDATED_AT, reverse: true)');
    expect(document).toContain('previousPage: products(last: 1, before: $beforeCursor, query: $query, sortKey: UPDATED_AT, reverse: true)');
    expect(document).toContain('pageInfo {');
    expect(document).toContain('updatedAt');

    expect(variables).toMatchObject({
      query: 'tag:egnition-sample-data product_type:ACCESSORIES',
      afterCursor: expect.any(String),
      beforeCursor: expect.any(String),
    });
    expect(capture.variables).toMatchObject({
      query: 'tag:egnition-sample-data product_type:ACCESSORIES',
      afterCursor: expect.any(String),
      beforeCursor: expect.any(String),
    });
    expect(capture.response?.data).toMatchObject({
      count: {
        count: expect.any(Number),
        precision: 'EXACT',
      },
      firstPage: {
        edges: [
          {
            cursor: expect.any(String),
            node: {
              id: expect.any(String),
              title: expect.any(String),
              updatedAt: expect.any(String),
            },
          },
        ],
        pageInfo: {
          hasNextPage: expect.any(Boolean),
          hasPreviousPage: false,
          startCursor: expect.any(String),
          endCursor: expect.any(String),
        },
      },
      nextPage: {
        edges: [
          {
            cursor: expect.any(String),
            node: {
              id: expect.any(String),
              title: expect.any(String),
              updatedAt: expect.any(String),
            },
          },
        ],
        pageInfo: {
          hasPreviousPage: true,
          startCursor: expect.any(String),
          endCursor: expect.any(String),
        },
      },
      previousPage: {
        edges: [
          {
            cursor: expect.any(String),
            node: {
              id: expect.any(String),
              title: expect.any(String),
              updatedAt: expect.any(String),
            },
          },
        ],
        pageInfo: {
          hasNextPage: true,
          startCursor: expect.any(String),
          endCursor: expect.any(String),
        },
      },
    });

    const firstCursor = capture.response?.data?.firstPage?.edges?.[0]?.cursor;
    const secondCursor = capture.response?.data?.nextPage?.edges?.[0]?.cursor;
    const previousCursor = capture.response?.data?.previousPage?.edges?.[0]?.cursor;

    expect(capture.variables?.afterCursor).toBe(firstCursor);
    expect(capture.variables?.beforeCursor).toBe(secondCursor);
    expect(previousCursor).toBe(firstCursor);
  });
});
