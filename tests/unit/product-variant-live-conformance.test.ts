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

const repoRoot = resolve(import.meta.dirname, '../..');
const scenarioRegistry = loadConformanceScenarios(repoRoot) as ScenarioRegistryEntry[];
const operationRegistry = JSON.parse(
  readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
) as OperationRegistryEntry[];

function expectCapturedScenario(options: {
  id: string;
  operationName: string;
  captureFiles: string[];
  paritySpecPath: string;
}) {
  const scenario = scenarioRegistry.find((entry) => entry.id === options.id);
  expect(scenario).toBeDefined();
  expect(scenario?.operationNames).toEqual([options.operationName]);
  expect(scenario?.status).toBe('captured');
  expect(scenario?.paritySpecPath).toBe(options.paritySpecPath);
  expect(scenario?.captureFiles).toEqual(options.captureFiles);
  for (const captureFile of options.captureFiles) {
    expect(existsSync(resolve(repoRoot, captureFile))).toBe(true);
  }
}

function expectCoveredOperation(options: { name: string; scenarioId: string }) {
  const operation = operationRegistry.find((entry) => entry.name === options.name);
  expect(operation).toBeDefined();
  expect(scenarioRegistry).toContainEqual(
    expect.objectContaining({
      id: options.scenarioId,
      operationNames: [options.name],
      status: 'captured',
    }),
  );
}

describe('product variant live conformance coverage', () => {
  it('promotes the bulk variant mutation family to captured live parity', () => {
    expectCapturedScenario({
      id: 'product-variants-bulk-create-live-parity',
      operationName: 'productVariantsBulkCreate',
      captureFiles: [
        'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-variants-bulk-create-parity.json',
      ],
      paritySpecPath: 'config/parity-specs/productVariantsBulkCreate-parity-plan.json',
    });
    expectCapturedScenario({
      id: 'product-variants-bulk-update-live-parity',
      operationName: 'productVariantsBulkUpdate',
      captureFiles: [
        'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-variants-bulk-update-parity.json',
      ],
      paritySpecPath: 'config/parity-specs/productVariantsBulkUpdate-parity-plan.json',
    });
    expectCapturedScenario({
      id: 'product-variants-bulk-delete-live-parity',
      operationName: 'productVariantsBulkDelete',
      captureFiles: [
        'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-variants-bulk-delete-parity.json',
      ],
      paritySpecPath: 'config/parity-specs/productVariantsBulkDelete-parity-plan.json',
    });

    expectCoveredOperation({
      name: 'productVariantsBulkCreate',
      scenarioId: 'product-variants-bulk-create-live-parity',
    });
    expectCoveredOperation({
      name: 'productVariantsBulkUpdate',
      scenarioId: 'product-variants-bulk-update-live-parity',
    });
    expectCoveredOperation({
      name: 'productVariantsBulkDelete',
      scenarioId: 'product-variants-bulk-delete-live-parity',
    });
  });

  it('promotes the single-variant compatibility family with captured compatibility evidence when the live roots are absent', () => {
    expectCapturedScenario({
      id: 'product-variant-create-compatibility-evidence',
      operationName: 'productVariantCreate',
      captureFiles: [
        'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-variants-bulk-create-parity.json',
        'pending/product-variant-compatibility-live-schema-blocker.md',
      ],
      paritySpecPath: 'config/parity-specs/productVariantCreate-parity-plan.json',
    });
    expectCapturedScenario({
      id: 'product-variant-update-compatibility-evidence',
      operationName: 'productVariantUpdate',
      captureFiles: [
        'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-variants-bulk-update-parity.json',
        'pending/product-variant-compatibility-live-schema-blocker.md',
      ],
      paritySpecPath: 'config/parity-specs/productVariantUpdate-parity-plan.json',
    });
    expectCapturedScenario({
      id: 'product-variant-delete-compatibility-evidence',
      operationName: 'productVariantDelete',
      captureFiles: [
        'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-variants-bulk-delete-parity.json',
        'pending/product-variant-compatibility-live-schema-blocker.md',
      ],
      paritySpecPath: 'config/parity-specs/productVariantDelete-parity-plan.json',
    });

    expectCoveredOperation({
      name: 'productVariantCreate',
      scenarioId: 'product-variant-create-compatibility-evidence',
    });
    expectCoveredOperation({
      name: 'productVariantUpdate',
      scenarioId: 'product-variant-update-compatibility-evidence',
    });
    expectCoveredOperation({
      name: 'productVariantDelete',
      scenarioId: 'product-variant-delete-compatibility-evidence',
    });
  });
});
