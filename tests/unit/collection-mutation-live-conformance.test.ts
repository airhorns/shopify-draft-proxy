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
  comparison?: {
    mode?: string;
    targets?: Array<{ name?: string }>;
  };
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
  it('exposes a package script for the collection mutation capture harness', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const packageJson = JSON.parse(readFileSync(resolve(repoRoot, 'package.json'), 'utf8')) as PackageJson;

    expect(packageJson.scripts?.['conformance:capture-collection-mutations']).toBe(
      'node ./scripts/capture-collection-mutation-conformance.mjs',
    );
  });

  it('marks the collection mutation family covered by captured live scenarios', () => {
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
