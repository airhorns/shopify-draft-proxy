import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

type ScenarioRegistryEntry = {
  id: string;
  operationNames: string[];
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
};

type OperationRegistryEntry = {
  name: string;
};

type ParitySpec = {
  scenarioId: string;
  scenarioStatus: string;
  liveCaptureFiles: string[];
  comparisonMode: string;
};

const expectedFamilies = [
  {
    operationName: 'productCreateMedia',
    scenarioId: 'product-create-media-live-parity',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-create-media-parity.json',
    paritySpecPath: 'config/parity-specs/productCreateMedia-parity-plan.json',
  },
  {
    operationName: 'productUpdateMedia',
    scenarioId: 'product-update-media-live-parity',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-update-media-parity.json',
    paritySpecPath: 'config/parity-specs/productUpdateMedia-parity-plan.json',
  },
  {
    operationName: 'productDeleteMedia',
    scenarioId: 'product-delete-media-live-parity',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-delete-media-parity.json',
    paritySpecPath: 'config/parity-specs/productDeleteMedia-parity-plan.json',
  },
] as const;

const repoRoot = resolve(import.meta.dirname, '../..');

describe('product media live conformance wiring', () => {
  it('marks the product media mutation family covered by captured live scenarios', () => {
    const scenarios = loadConformanceScenarios(repoRoot) as ScenarioRegistryEntry[];
    const operationRegistry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];

    for (const expected of expectedFamilies) {
      expect(scenarios).toContainEqual(
        expect.objectContaining({
          id: expected.scenarioId,
          operationNames: [expected.operationName],
          status: 'captured',
          captureFiles: [expected.captureFile],
          paritySpecPath: expected.paritySpecPath,
        }),
      );
      expect(existsSync(resolve(repoRoot, expected.captureFile))).toBe(true);

      expect(operationRegistry).toContainEqual(
        expect.objectContaining({
          name: expected.operationName,
        }),
      );
    }
  });

  it('upgrades the product media parity specs to captured-vs-proxy-request mode', () => {
    for (const expected of expectedFamilies) {
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
});
