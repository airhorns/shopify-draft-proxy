import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import type { ParitySpec } from '../../scripts/conformance-parity-lib.js';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

type ConformanceScenario = {
  id: string;
  status: string;
  operationNames: string[];
  captureFiles: string[];
  paritySpecPath: string;
};

const expectedLiveFamilies = [
  {
    operationName: 'productOptionsCreate',
    scenarioId: 'product-options-create-live-parity',
    paritySpecPath: 'config/parity-specs/productOptionsCreate-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-options-create-parity.json',
  },
  {
    operationName: 'productOptionUpdate',
    scenarioId: 'product-option-update-live-parity',
    paritySpecPath: 'config/parity-specs/productOptionUpdate-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-option-update-parity.json',
  },
  {
    operationName: 'productOptionsDelete',
    scenarioId: 'product-options-delete-live-parity',
    paritySpecPath: 'config/parity-specs/productOptionsDelete-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-options-delete-parity.json',
  },
] as const;

describe('product option mutation live conformance wiring', () => {
  it('marks the product option mutation family covered by captured live scenarios', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];

    for (const expected of expectedLiveFamilies) {
      expect(scenarios).toContainEqual(
        expect.objectContaining({
          id: expected.scenarioId,
          status: 'captured',
          operationNames: [expected.operationName],
          paritySpecPath: expected.paritySpecPath,
          captureFiles: [expected.captureFile],
        }),
      );
    }
  });

  it('upgrades the product option mutation parity specs to strict comparison contracts', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    for (const expected of expectedLiveFamilies) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.paritySpecPath), 'utf8')) as ParitySpec;

      expect(spec).toEqual(
        expect.objectContaining({
          scenarioId: expected.scenarioId,
          scenarioStatus: 'captured',
          liveCaptureFiles: [expected.captureFile],
        }),
      );
      expect(spec.comparison?.mode).toBe('strict-json');
      expect(spec.comparison?.targets?.map((target) => target.name)).toEqual(['mutation-data']);
    }
  });
});
