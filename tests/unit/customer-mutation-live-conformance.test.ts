import { existsSync, readFileSync } from 'node:fs';
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
};

type ParitySpec = {
  scenarioId: string;
  scenarioStatus: string;
  liveCaptureFiles: string[];
  comparisonMode: string;
};

type CustomerMutationCapture = {
  validation?: {
    variables?: Record<string, unknown>;
    response?: {
      data?: Record<string, unknown>;
    };
  };
};

const repoRoot = resolve(import.meta.dirname, '../..');

const expectedFamilies = [
  {
    operationName: 'customerCreate',
    scenarioId: 'customer-create-live-parity',
    paritySpecPath: 'config/parity-specs/customerCreate-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customer-create-parity.json',
  },
  {
    operationName: 'customerUpdate',
    scenarioId: 'customer-update-live-parity',
    paritySpecPath: 'config/parity-specs/customerUpdate-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customer-update-parity.json',
  },
  {
    operationName: 'customerDelete',
    scenarioId: 'customer-delete-live-parity',
    paritySpecPath: 'config/parity-specs/customerDelete-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customer-delete-parity.json',
  },
] as const;

describe('customer mutation live conformance wiring', () => {
  it('exposes a package script for the customer mutation capture harness', () => {
    const packageJson = JSON.parse(readFileSync(resolve(repoRoot, 'package.json'), 'utf8')) as PackageJson;
    expect(packageJson.scripts?.['conformance:capture-customer-mutations']).toBe(
      'tsx ./scripts/capture-customer-mutation-conformance.mts',
    );
  });

  it('marks the customer mutation family covered by captured live scenarios', () => {
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];

    for (const expected of expectedFamilies) {
      expect(registry).toContainEqual(
        expect.objectContaining({
          name: expected.operationName,
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

      expect(existsSync(resolve(repoRoot, expected.captureFile))).toBe(true);
    }
  });

  it('upgrades the customer mutation parity specs to captured-vs-proxy-request mode', () => {
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

  it('preserves live validation userErrors alongside the happy-path customer mutation captures', () => {
    const createCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customer-create-parity.json'),
        'utf8',
      ),
    ) as CustomerMutationCapture;
    const updateCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customer-update-parity.json'),
        'utf8',
      ),
    ) as CustomerMutationCapture;
    const deleteCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customer-delete-parity.json'),
        'utf8',
      ),
    ) as CustomerMutationCapture;

    expect(createCapture.validation).toMatchObject({
      variables: { input: { email: '' } },
      response: {
        data: {
          customerCreate: {
            customer: null,
            userErrors: [{ field: null, message: 'A name, phone number, or email address must be present' }],
          },
        },
      },
    });

    expect(updateCapture.validation).toMatchObject({
      variables: { input: { id: 'gid://shopify/Customer/999999999999999', firstName: 'Ghost' } },
      response: {
        data: {
          customerUpdate: {
            customer: null,
            userErrors: [{ field: ['id'], message: 'Customer does not exist' }],
          },
        },
      },
    });

    expect(deleteCapture.validation).toMatchObject({
      variables: { input: { id: 'gid://shopify/Customer/999999999999999' } },
      response: {
        data: {
          customerDelete: {
            deletedCustomerId: null,
            userErrors: [{ field: ['id'], message: "Customer can't be found" }],
          },
        },
      },
    });
  });
});
