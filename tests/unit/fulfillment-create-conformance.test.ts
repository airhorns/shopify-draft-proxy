import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

type OperationRegistryEntry = {
  name: string;
  domain: string;
  execution: string;
  implemented: boolean;
  runtimeTests?: string[];
  conformance?: {
    status?: string;
    scenarioIds?: string[];
  };
};

type ConformanceScenario = {
  id: string;
  operationNames: string[];
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
};

type ParitySpec = {
  scenarioId: string;
  scenarioStatus: string;
  liveCaptureFiles: string[];
  comparisonMode: string;
  proxyRequest?: {
    documentPath?: string | null;
    variablesPath?: string | null;
  };
};

describe('fulfillmentCreate conformance coverage', () => {
  it('registers fulfillmentCreate as the first evidence-backed fulfillment root under the orders domain', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'fulfillmentCreate',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['tests/integration/order-fulfillment-flow.test.ts'],
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: ['fulfillment-create-invalid-id-parity'],
        }),
      }),
    );
  });

  it('tracks the captured invalid-fulfillment-order parity scenario with executable request artifacts', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const scenarios = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/conformance-scenarios.json'), 'utf8'),
    ) as ConformanceScenario[];
    const spec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/fulfillmentCreate-invalid-id-parity.json'), 'utf8'),
    ) as ParitySpec;
    const document = readFileSync(
      resolve(repoRoot, 'config/parity-requests/fulfillmentCreate-invalid-id-parity.graphql'),
      'utf8',
    );
    const variables = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'config/parity-requests/fulfillmentCreate-invalid-id-parity.variables.json'),
        'utf8',
      ),
    ) as Record<string, unknown>;
    const worklist = readFileSync(resolve(repoRoot, 'docs/shopify-admin-worklist.md'), 'utf8');
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'fulfillment-create-invalid-id-parity',
        operationNames: ['fulfillmentCreate'],
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/fulfillment-create-invalid-id.json'],
        paritySpecPath: 'config/parity-specs/fulfillmentCreate-invalid-id-parity.json',
      }),
    );

    expect(spec).toEqual(
      expect.objectContaining({
        scenarioId: 'fulfillment-create-invalid-id-parity',
        scenarioStatus: 'captured',
        liveCaptureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/fulfillment-create-invalid-id.json'],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: 'config/parity-requests/fulfillmentCreate-invalid-id-parity.graphql',
          variablesPath: 'config/parity-requests/fulfillmentCreate-invalid-id-parity.variables.json',
        },
      }),
    );

    expect(document).toContain('mutation FulfillmentCreateInvalidIdParity');
    expect(document).toContain('fulfillmentCreate');
    expect(document).toContain('userErrors');
    expect(document).toContain('trackingInfo');
    expect(variables).toMatchObject({
      fulfillment: {
        lineItemsByFulfillmentOrder: [{ fulfillmentOrderId: 'gid://shopify/FulfillmentOrder/0' }],
      },
    });
    expect(worklist).toContain('`fulfillmentCreate`');
    expect(worklist).toContain('invalid fulfillment-order id');
    expect(weirdNotes).toContain('fulfillmentCreate');
    expect(weirdNotes).toContain('invalid id');
  });
});
