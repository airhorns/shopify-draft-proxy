import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

type OperationRegistryEntry = {
  name: string;
};

type ScenarioRegistryEntry = {
  id: string;
  operationNames: string[];
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
  notes?: string;
};

type ParitySpec = {
  scenarioId: string;
  scenarioStatus: string;
  proxyRequest?: {
    documentPath?: string | null;
    variablesPath?: string | null;
  };
  blocker?: {
    kind?: string;
    blockerPath?: string;
    details?: {
      requiredApproval?: string;
      probeRoots?: string[];
      blockedFields?: string[];
      failingMessage?: string;
      docsUrl?: string;
    };
  };
  comparisonMode?: string;
  notes?: string;
};

const repoRoot = resolve(import.meta.dirname, '../..');
const operationRegistry = JSON.parse(
  readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
) as OperationRegistryEntry[];
const scenarioRegistry = loadConformanceScenarios(repoRoot) as ScenarioRegistryEntry[];
const expectedScenarios = [
  {
    operationName: 'customer',
    scenarioId: 'customer-detail-parity-plan',
    specPath: 'config/parity-specs/customer-detail-parity-plan.json',
    documentPath: 'config/parity-requests/customer-detail-parity-plan.graphql',
    variablesPath: 'config/parity-requests/customer-detail-parity-plan.variables.json',
    fixturePath: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customer-detail.json',
  },
  {
    operationName: 'customers',
    scenarioId: 'customers-catalog-parity-plan',
    specPath: 'config/parity-specs/customers-catalog-parity-plan.json',
    documentPath: 'config/parity-requests/customers-catalog-parity-plan.graphql',
    variablesPath: 'config/parity-requests/customers-catalog-parity-plan.variables.json',
    fixturePath: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers-catalog.json',
  },
  {
    operationName: 'customers',
    scenarioId: 'customers-search-read',
    specPath: 'config/parity-specs/customers-search-read.json',
    documentPath: 'config/parity-requests/customers-search-read.graphql',
    variablesPath: 'config/parity-requests/customers-search-read.variables.json',
    fixturePath: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers-search.json',
  },
  {
    operationName: 'customersCount',
    scenarioId: 'customers-count-read',
    specPath: 'config/parity-specs/customers-count-read.json',
    documentPath: 'config/parity-requests/customers-count-read.graphql',
    variablesPath: 'config/parity-requests/customers-count-read.variables.json',
    fixturePath: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers-count.json',
  },
] as const;

describe('customer conformance coverage state', () => {
  it('promotes customer reads to covered with captured live fixtures and reusable proxy requests', () => {
    for (const expected of expectedScenarios) {
      const operation = operationRegistry.find((entry) => entry.name === expected.operationName);
      expect(operation).toBeDefined();

      expect(scenarioRegistry).toContainEqual(
        expect.objectContaining({
          id: expected.scenarioId,
          operationNames: [expected.operationName],
          status: 'captured',
          captureFiles: [expected.fixturePath],
          paritySpecPath: expected.specPath,
          notes: expect.stringContaining('Live'),
        }),
      );

      const spec = JSON.parse(readFileSync(resolve(repoRoot, expected.specPath), 'utf8')) as ParitySpec;
      expect(spec).toMatchObject({
        scenarioId: expected.scenarioId,
        scenarioStatus: 'captured',
        liveCaptureFiles: [expected.fixturePath],
        comparisonMode: 'capture-only',
        proxyRequest: {
          documentPath: expected.documentPath,
          variablesPath: expected.variablesPath,
        },
      });
      expect(spec.blocker).toEqual({
        kind: 'explicit-comparison-targets-needed',
        blockerPath: null,
      });

      expect(existsSync(resolve(repoRoot, expected.documentPath))).toBe(true);
      expect(existsSync(resolve(repoRoot, expected.variablesPath))).toBe(true);
      expect(existsSync(resolve(repoRoot, expected.fixturePath))).toBe(true);
    }
  });

  it('removes the stale protected-data blocker note after successful customer capture', () => {
    const blockerPath = resolve(repoRoot, 'pending/customer-conformance-protected-data-blocker.md');
    expect(existsSync(blockerPath)).toBe(false);
  });
});
