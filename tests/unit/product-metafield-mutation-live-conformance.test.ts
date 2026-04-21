import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

type PackageJson = {
  scripts?: Record<string, string>;
};

type OperationRegistryEntry = {
  name: string;
};

type ConformanceScenario = {
  id: string;
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
  notes?: string;
};

type ParitySpec = {
  scenarioId: string;
  scenarioStatus: string;
  liveCaptureFiles: string[];
  comparisonMode: string;
  notes?: string;
};

describe('product metafield mutation conformance wiring', () => {
  it('exposes a package script for the product metafield mutation capture harness', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const packageJson = JSON.parse(readFileSync(resolve(repoRoot, 'package.json'), 'utf8')) as PackageJson;

    expect(packageJson.scripts?.['conformance:capture-product-metafield-mutations']).toBe(
      'tsx ./scripts/capture-product-metafield-mutation-conformance.mts',
    );
  });

  it('promotes metafieldsSet, metafieldsDelete, and the singular metafieldDelete compatibility alias to covered live parity', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'metafieldsSet',
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'metafieldsDelete',
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'metafields-set-live-parity',
        status: 'captured',
        paritySpecPath: 'config/parity-specs/metafieldsSet-parity-plan.json',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/metafields-set-parity.json'],
      }),
    );

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'metafieldDelete',
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'metafields-delete-live-parity',
        status: 'captured',
        paritySpecPath: 'config/parity-specs/metafieldsDelete-parity-plan.json',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/metafields-delete-parity.json'],
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'metafield-delete-compatibility-live-parity',
        status: 'captured',
        paritySpecPath: 'config/parity-specs/metafieldDelete-parity-plan.json',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/metafields-delete-parity.json'],
        notes: expect.stringContaining('compatibility alias'),
      }),
    );
  });

  it('upgrades the live-supported metafield parity specs to captured-vs-proxy-request mode', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    const setSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/metafieldsSet-parity-plan.json'), 'utf8'),
    ) as ParitySpec;
    expect(setSpec).toEqual(
      expect.objectContaining({
        scenarioId: 'metafields-set-live-parity',
        scenarioStatus: 'captured',
        liveCaptureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/metafields-set-parity.json'],
        comparisonMode: 'captured-vs-proxy-request',
      }),
    );

    const deleteSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/metafieldsDelete-parity-plan.json'), 'utf8'),
    ) as ParitySpec;
    expect(deleteSpec).toEqual(
      expect.objectContaining({
        scenarioId: 'metafields-delete-live-parity',
        scenarioStatus: 'captured',
        liveCaptureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/metafields-delete-parity.json',
        ],
        comparisonMode: 'captured-vs-proxy-request',
      }),
    );

    const compatibilityDeleteSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/metafieldDelete-parity-plan.json'), 'utf8'),
    ) as ParitySpec;
    expect(compatibilityDeleteSpec).toEqual(
      expect.objectContaining({
        scenarioId: 'metafield-delete-compatibility-live-parity',
        scenarioStatus: 'captured',
        liveCaptureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/metafields-delete-parity.json',
        ],
        comparisonMode: 'captured-vs-proxy-request',
        notes: expect.stringContaining('compatibility alias'),
      }),
    );
  });
});
