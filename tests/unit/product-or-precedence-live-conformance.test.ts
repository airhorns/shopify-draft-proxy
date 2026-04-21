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

type ProductOrPrecedenceCapture = {
  variables?: {
    precedenceQuery?: string;
  };
  response?: {
    data?: {
      precedenceCount?: {
        count?: number;
        precision?: string;
      };
      precedenceMatches?: {
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
    };
  };
};

describe('product ungrouped OR precedence live conformance wiring', () => {
  it("registers a captured products/productsCount scenario for Shopify's implicit AND-before-OR precedence slice", () => {
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
        id: 'products-or-precedence-read',
        status: 'captured',
        operationNames: ['products', 'productsCount'],
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products-or-precedence.json'],
        paritySpecPath: 'config/parity-specs/products-or-precedence-read.json',
      }),
    );
  });

  it('keeps the proxy request aligned with the AND-before-OR precedence slice and records the live capture', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const documentPath = resolve(repoRoot, 'config/parity-requests/products-or-precedence-read.graphql');
    const variablesPath = resolve(repoRoot, 'config/parity-requests/products-or-precedence-read.variables.json');
    const fixturePath = resolve(
      repoRoot,
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products-or-precedence.json',
    );

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);
    expect(existsSync(fixturePath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as Record<string, unknown>;
    const capture = JSON.parse(readFileSync(fixturePath, 'utf8')) as ProductOrPrecedenceCapture;

    expect(document).toContain('query ProductsOrPrecedenceRead($precedenceQuery: String!)');
    expect(document).toContain('precedenceCount: productsCount(query: $precedenceQuery)');
    expect(document).toContain(
      'precedenceMatches: products(first: 10, query: $precedenceQuery, sortKey: UPDATED_AT, reverse: true)',
    );
    expect(document).toContain('productType');
    expect(document).toContain('tags');

    expect(variables).toMatchObject({
      precedenceQuery: 'vendor:NIKE OR vendor:VANS tag:egnition-sample-data product_type:ACCESSORIES',
    });
    expect(capture.variables).toMatchObject({
      precedenceQuery: 'vendor:NIKE OR vendor:VANS tag:egnition-sample-data product_type:ACCESSORIES',
    });
    expect(capture.response?.data).toMatchObject({
      precedenceCount: {
        count: 2,
        precision: 'EXACT',
      },
    });
    expect(capture.response?.data?.precedenceMatches?.edges).toEqual([
      expect.objectContaining({
        node: expect.objectContaining({
          id: expect.any(String),
          title: expect.stringContaining('SWOOSH PRO FLAT PEAK CAP'),
          vendor: 'NIKE',
          productType: 'ACCESSORIES',
          tags: expect.arrayContaining(['cap']),
        }),
      }),
      expect.objectContaining({
        node: expect.objectContaining({
          id: expect.any(String),
          title: expect.stringContaining('CLASSIC SUPER NO SHOW SOCKS 3 PACK WHITE'),
          vendor: 'VANS',
          productType: 'ACCESSORIES',
          tags: expect.arrayContaining(['vans']),
        }),
      }),
    ]);
  });
});
