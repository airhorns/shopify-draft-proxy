import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

type ConformanceScenario = {
  id: string;
  captureFiles?: string[];
};

type ParitySpec = {
  scenarioId: string;
  liveCaptureFiles?: string[];
};

describe('product empty/null live conformance evidence', () => {
  it('tracks the extra empty/null product-family fixture in the captured product detail scenario', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const scenarios = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/conformance-scenarios.json'), 'utf8'),
    ) as ConformanceScenario[];
    const paritySpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/product-detail-read.json'), 'utf8'),
    ) as ParitySpec;

    const productDetailScenario = scenarios.find((scenario) => scenario.id === 'product-detail-read');

    expect(productDetailScenario?.captureFiles).toContain(
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-empty-state.json',
    );
    expect(paritySpec.liveCaptureFiles).toContain(
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-empty-state.json',
    );
  });

  it('preserves the live null/empty behavior slices for product lookups and empty catalog queries', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const capture = JSON.parse(
      readFileSync(resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-empty-state.json'), 'utf8'),
    ) as {
      variables?: Record<string, unknown>;
      response?: {
        data?: {
          missingProduct?: null;
          emptyCount?: { count?: number; precision?: string };
          emptyProducts?: {
            edges?: unknown[];
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

    expect(capture.variables).toMatchObject({
      missingId: 'gid://shopify/Product/999999999999999',
      emptyQuery: 'title:__hermes_empty_catalog_probe__',
    });
    expect(capture.response?.data?.missingProduct).toBeNull();
    expect(capture.response?.data?.emptyCount).toEqual({
      count: 0,
      precision: 'EXACT',
    });
    expect(capture.response?.data?.emptyProducts?.edges).toEqual([]);
    expect(capture.response?.data?.emptyProducts?.pageInfo).toEqual({
      hasNextPage: false,
      hasPreviousPage: false,
      startCursor: null,
      endCursor: null,
    });
  });
});
