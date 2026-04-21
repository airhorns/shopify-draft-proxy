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
    operationName: 'collectionCreate',
    scenarioId: 'collection-create-live-parity',
    paritySpecPath: 'config/parity-specs/collectionCreate-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/collection-create-parity.json',
  },
  {
    operationName: 'collectionUpdate',
    scenarioId: 'collection-update-live-parity',
    paritySpecPath: 'config/parity-specs/collectionUpdate-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/collection-update-parity.json',
  },
  {
    operationName: 'collectionDelete',
    scenarioId: 'collection-delete-live-parity',
    paritySpecPath: 'config/parity-specs/collectionDelete-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/collection-delete-parity.json',
  },
  {
    operationName: 'collectionAddProducts',
    scenarioId: 'collection-add-products-live-parity',
    paritySpecPath: 'config/parity-specs/collectionAddProducts-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/collection-add-products-parity.json',
  },
  {
    operationName: 'collectionRemoveProducts',
    scenarioId: 'collection-remove-products-live-parity',
    paritySpecPath: 'config/parity-specs/collectionRemoveProducts-parity-plan.json',
    captureFile:
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/collection-remove-products-parity.json',
  },
] as const;

describe('collection mutation live conformance wiring', () => {
  it('marks the collection mutation family covered by captured live scenarios', () => {
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

  it('upgrades the collection mutation parity specs to strict comparison contracts', () => {
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
