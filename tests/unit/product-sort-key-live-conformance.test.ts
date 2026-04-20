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

type ProductSortKeysCapture = {
  variables?: {
    query?: string;
  };
  response?: {
    data?: {
      titleOrder?: { edges?: Array<{ node?: { id?: string; title?: string; vendor?: string; productType?: string } }> };
      vendorOrder?: { edges?: Array<{ node?: { id?: string; title?: string; vendor?: string; productType?: string } }> };
      productTypeOrder?: { edges?: Array<{ node?: { id?: string; title?: string; vendor?: string; productType?: string } }> };
      publishedAtOrder?: { edges?: Array<{ node?: { id?: string; publishedAt?: string | null } }> };
      idOrder?: { edges?: Array<{ node?: { id?: string; legacyResourceId?: string } }> };
    };
  };
};

describe('product sort-key live conformance wiring', () => {
  it('registers a captured products-only scenario for the live schema-backed sort-key slice', () => {
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
          scenarioIds: expect.arrayContaining(['products-sort-keys-read']),
        }),
      }),
    );
    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'productsCount',
        conformance: expect.objectContaining({
          scenarioIds: expect.not.arrayContaining(['products-sort-keys-read']),
        }),
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'products-sort-keys-read',
        status: 'captured',
        operationNames: ['products'],
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products-sort-keys.json'],
        paritySpecPath: 'config/parity-specs/products-sort-keys-read.json',
      }),
    );
  });

  it('keeps the proxy request aligned with the live ProductSortKeys subset and preserves the captured aliases', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const document = readFileSync(
      resolve(repoRoot, 'config/parity-requests/products-sort-keys-read.graphql'),
      'utf8',
    );
    const capture = JSON.parse(
      readFileSync(
        resolve(
          repoRoot,
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products-sort-keys.json',
        ),
        'utf8',
      ),
    ) as ProductSortKeysCapture;

    expect(document).toContain('titleOrder: products(first: $first, query: $query, sortKey: TITLE)');
    expect(document).toContain('vendorOrder: products(first: $first, query: $query, sortKey: VENDOR)');
    expect(document).toContain('productTypeOrder: products(first: $first, query: $query, sortKey: PRODUCT_TYPE, reverse: true)');
    expect(document).toContain('publishedAtOrder: products(first: $first, query: $query, sortKey: PUBLISHED_AT, reverse: true)');
    expect(document).toContain('idOrder: products(first: $first, query: $query, sortKey: ID, reverse: true)');
    expect(document).not.toContain('sortKey: HANDLE');
    expect(document).not.toContain('sortKey: STATUS');

    expect(capture.variables).toMatchObject({
      query: 'egnition-sample-data status:active',
    });
    expect(capture.response?.data).toEqual(
      expect.objectContaining({
        titleOrder: expect.objectContaining({
          edges: expect.arrayContaining([
            expect.objectContaining({
              node: expect.objectContaining({
                title: expect.any(String),
                vendor: expect.any(String),
                productType: expect.any(String),
              }),
            }),
          ]),
        }),
        vendorOrder: expect.objectContaining({
          edges: expect.arrayContaining([
            expect.objectContaining({
              node: expect.objectContaining({
                title: expect.any(String),
                vendor: expect.any(String),
                productType: expect.any(String),
              }),
            }),
          ]),
        }),
        productTypeOrder: expect.objectContaining({
          edges: expect.arrayContaining([
            expect.objectContaining({
              node: expect.objectContaining({
                title: expect.any(String),
                vendor: expect.any(String),
                productType: expect.any(String),
              }),
            }),
          ]),
        }),
        publishedAtOrder: expect.objectContaining({
          edges: expect.arrayContaining([
            expect.objectContaining({
              node: expect.objectContaining({
                id: expect.any(String),
              }),
            }),
          ]),
        }),
        idOrder: expect.objectContaining({
          edges: expect.arrayContaining([
            expect.objectContaining({
              node: expect.objectContaining({
                id: expect.any(String),
                legacyResourceId: expect.any(String),
              }),
            }),
          ]),
        }),
      }),
    );
  });
});
