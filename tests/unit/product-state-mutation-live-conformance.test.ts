import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

type PackageJson = {
  scripts?: Record<string, string>;
};

type OperationRegistryEntry = {
  name: string;
  conformance?: {
    status?: string;
    scenarioIds?: string[];
  };
};

type ConformanceScenario = {
  id: string;
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
};

type ParitySpec = {
  scenarioId: string;
  scenarioStatus: string;
  liveCaptureFiles: string[];
  comparisonMode: string;
};

interface ProductChangeStatusCapture {
  mutation?: {
    response?: {
      data?: {
        productChangeStatus?: {
          product?: { id?: string; status?: string; updatedAt?: string } | null;
          userErrors?: Array<{ field?: string[] | null; message?: string }>;
        };
      };
    };
  };
  validation?: {
    unknownProduct?: {
      response?: {
        data?: {
          productChangeStatus?: {
            product?: null;
            userErrors?: Array<{ field?: string[] | null; message?: string }>;
          };
        };
      };
    };
    nullLiteralProductId?: {
      response?: {
        errors?: Array<{
          message?: string;
          path?: string[];
          extensions?: {
            code?: string;
            typeName?: string;
            argumentName?: string;
          };
        }>;
      };
    };
  };
}

const expectedLiveFamilies = [
  {
    operationName: 'tagsAdd',
    scenarioId: 'tags-add-live-parity',
    paritySpecPath: 'config/parity-specs/tagsAdd-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/tags-add-parity.json',
  },
  {
    operationName: 'tagsRemove',
    scenarioId: 'tags-remove-live-parity',
    paritySpecPath: 'config/parity-specs/tagsRemove-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/tags-remove-parity.json',
  },
  {
    operationName: 'productChangeStatus',
    scenarioId: 'product-change-status-live-parity',
    paritySpecPath: 'config/parity-specs/productChangeStatus-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-change-status-parity.json',
  },
] as const;

describe('product state mutation live conformance wiring', () => {
  it('exposes a package script for the product state mutation capture harness', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const packageJson = JSON.parse(readFileSync(resolve(repoRoot, 'package.json'), 'utf8')) as PackageJson;

    expect(packageJson.scripts?.['conformance:capture-product-state-mutations']).toBe(
      'node ./scripts/capture-product-state-mutation-conformance.mjs',
    );
  });

  it('marks the product state mutation family covered by captured live scenarios', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const scenarios = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/conformance-scenarios.json'), 'utf8'),
    ) as ConformanceScenario[];

    for (const expected of expectedLiveFamilies) {
      expect(registry).toContainEqual(
        expect.objectContaining({
          name: expected.operationName,
          conformance: expect.objectContaining({
            status: 'covered',
            scenarioIds: [expected.scenarioId],
          }),
        }),
      );

      expect(scenarios).toContainEqual(
        expect.objectContaining({
          id: expected.scenarioId,
          status: 'captured',
          paritySpecPath: expected.paritySpecPath,
          captureFiles: [expected.captureFile],
        }),
      );
    }
  });

  it('upgrades the product state mutation parity specs to captured-vs-proxy-request mode', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    for (const expected of expectedLiveFamilies) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;

      expect(spec).toEqual(
        expect.objectContaining({
          scenarioId: expected.scenarioId,
          scenarioStatus: 'captured',
          liveCaptureFiles: [expected.captureFile],
          comparisonMode: 'captured-vs-proxy-request',
        }),
      );
    }
  });

  it('keeps the productChangeStatus live fixture aligned with the captured unknown-id and null-literal validation slices', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const capture = JSON.parse(
      readFileSync(
        resolve(
          repoRoot,
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-change-status-parity.json',
        ),
        'utf8',
      ),
    ) as ProductChangeStatusCapture;

    expect(capture.mutation?.response?.data?.productChangeStatus).toEqual(
      expect.objectContaining({
        product: expect.objectContaining({
          id: expect.any(String),
          status: expect.any(String),
          updatedAt: expect.any(String),
        }),
        userErrors: [],
      }),
    );
    expect(capture.validation?.unknownProduct?.response?.data?.productChangeStatus).toEqual({
      product: null,
      userErrors: [{ field: ['productId'], message: 'Product does not exist' }],
    });
    expect(capture.validation?.nullLiteralProductId?.response?.errors).toEqual([
      expect.objectContaining({
        message: "Argument 'productId' on Field 'productChangeStatus' has an invalid value (null). Expected type 'ID!'.",
        path: expect.arrayContaining(['productChangeStatus', 'productId']),
        extensions: expect.objectContaining({
          code: 'argumentLiteralsIncompatible',
          typeName: 'Field',
          argumentName: 'productId',
        }),
      }),
    ]);
  });
});
