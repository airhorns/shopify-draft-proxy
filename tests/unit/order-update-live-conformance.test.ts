import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import type { ParitySpec } from '../../scripts/conformance-parity-lib.js';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

type OperationRegistryEntry = {
  name: string;
  domain: string;
  execution: string;
  implemented: boolean;
  runtimeTests?: string[];
};

type ConformanceScenario = {
  id: string;
  operationNames: string[];
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
};

describe('orderUpdate live conformance', () => {
  it('tracks the first evidence-backed order editing slices as a covered order-domain operation', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderUpdate',
        domain: 'orders',
        execution: 'stage-locally',
        implemented: true,
        runtimeTests: ['tests/integration/order-draft-flow.test.ts', 'tests/integration/order-edit-flow.test.ts'],
      }),
    );
  });

  it('registers captured parity scenarios and executable request scaffolds for the safe orderUpdate validation slices', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];
    const unknownIdSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/orderUpdate-parity-plan.json'), 'utf8'),
    ) as ParitySpec;
    const missingIdSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/orderUpdate-missing-id-parity.json'), 'utf8'),
    ) as ParitySpec;
    const inlineMissingIdSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/orderUpdate-inline-missing-id-parity.json'), 'utf8'),
    ) as ParitySpec;
    const inlineNullIdSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/orderUpdate-inline-null-id-parity.json'), 'utf8'),
    ) as ParitySpec;
    const document = readFileSync(resolve(repoRoot, 'config/parity-requests/orderUpdate-parity-plan.graphql'), 'utf8');
    const inlineMissingIdDocument = readFileSync(
      resolve(repoRoot, 'config/parity-requests/orderUpdate-inline-missing-id-parity.graphql'),
      'utf8',
    );
    const inlineNullIdDocument = readFileSync(
      resolve(repoRoot, 'config/parity-requests/orderUpdate-inline-null-id-parity.graphql'),
      'utf8',
    );
    const unknownIdVariables = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-requests/orderUpdate-parity-plan.variables.json'), 'utf8'),
    ) as Record<string, unknown>;
    const missingIdVariables = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-requests/orderUpdate-missing-id-parity.variables.json'), 'utf8'),
    ) as Record<string, unknown>;
    const inlineMissingIdVariables = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'config/parity-requests/orderUpdate-inline-missing-id-parity.variables.json'),
        'utf8',
      ),
    ) as Record<string, unknown>;
    const inlineNullIdVariables = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'config/parity-requests/orderUpdate-inline-null-id-parity.variables.json'),
        'utf8',
      ),
    ) as Record<string, unknown>;
    const unknownIdFixture = JSON.parse(
      readFileSync(
        resolve(
          repoRoot,
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-unknown-id.json',
        ),
        'utf8',
      ),
    ) as Record<string, unknown>;
    const missingIdFixture = JSON.parse(
      readFileSync(
        resolve(
          repoRoot,
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-missing-id.json',
        ),
        'utf8',
      ),
    ) as Record<string, unknown>;
    const inlineMissingIdFixture = JSON.parse(
      readFileSync(
        resolve(
          repoRoot,
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-inline-missing-id.json',
        ),
        'utf8',
      ),
    ) as Record<string, unknown>;
    const inlineNullIdFixture = JSON.parse(
      readFileSync(
        resolve(
          repoRoot,
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-inline-null-id.json',
        ),
        'utf8',
      ),
    ) as Record<string, unknown>;
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'order-update-inline-missing-id-argument-error',
        operationNames: ['orderUpdate'],
        status: 'captured',
        captureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-inline-missing-id.json',
        ],
        paritySpecPath: 'config/parity-specs/orderUpdate-inline-missing-id-parity.json',
      }),
    );
    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'order-update-inline-null-id-argument-error',
        operationNames: ['orderUpdate'],
        status: 'captured',
        captureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-inline-null-id.json',
        ],
        paritySpecPath: 'config/parity-specs/orderUpdate-inline-null-id-parity.json',
      }),
    );
    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'order-update-unknown-id-parity',
        operationNames: ['orderUpdate'],
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-unknown-id.json'],
        paritySpecPath: 'config/parity-specs/orderUpdate-parity-plan.json',
      }),
    );
    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'order-update-missing-id-invalid-variable',
        operationNames: ['orderUpdate'],
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-missing-id.json'],
        paritySpecPath: 'config/parity-specs/orderUpdate-missing-id-parity.json',
      }),
    );

    expect(inlineMissingIdSpec).toEqual(
      expect.objectContaining({
        scenarioId: 'order-update-inline-missing-id-argument-error',
        scenarioStatus: 'captured',
        liveCaptureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-inline-missing-id.json',
        ],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: 'config/parity-requests/orderUpdate-inline-missing-id-parity.graphql',
          variablesPath: 'config/parity-requests/orderUpdate-inline-missing-id-parity.variables.json',
        },
      }),
    );
    expect(inlineNullIdSpec).toEqual(
      expect.objectContaining({
        scenarioId: 'order-update-inline-null-id-argument-error',
        scenarioStatus: 'captured',
        liveCaptureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-inline-null-id.json',
        ],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: 'config/parity-requests/orderUpdate-inline-null-id-parity.graphql',
          variablesPath: 'config/parity-requests/orderUpdate-inline-null-id-parity.variables.json',
        },
      }),
    );
    expect(unknownIdSpec).toEqual(
      expect.objectContaining({
        scenarioId: 'order-update-unknown-id-parity',
        scenarioStatus: 'captured',
        liveCaptureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-unknown-id.json',
        ],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: 'config/parity-requests/orderUpdate-parity-plan.graphql',
          variablesPath: 'config/parity-requests/orderUpdate-parity-plan.variables.json',
        },
      }),
    );
    expect(missingIdSpec).toEqual(
      expect.objectContaining({
        scenarioId: 'order-update-missing-id-invalid-variable',
        scenarioStatus: 'captured',
        liveCaptureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-missing-id.json',
        ],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: 'config/parity-requests/orderUpdate-parity-plan.graphql',
          variablesPath: 'config/parity-requests/orderUpdate-missing-id-parity.variables.json',
        },
      }),
    );
    expect(document).toContain('mutation OrderUpdateParityPlan($input: OrderInput!)');
    expect(document).toContain('orderUpdate(input: $input)');
    expect(document).toContain('userErrors');
    expect(document).toContain('updatedAt');
    expect(inlineMissingIdDocument).toContain('mutation OrderUpdateInlineMissingIdParityPlan');
    expect(inlineMissingIdDocument).toContain('orderUpdate(');
    expect(inlineMissingIdDocument).toContain('note: "order update inline missing-id parity plan"');
    expect(inlineNullIdDocument).toContain('mutation OrderUpdateInlineNullIdParityPlan');
    expect(inlineNullIdDocument).toContain('id: null');
    expect(unknownIdVariables).toEqual({
      input: {
        id: 'gid://shopify/Order/0',
        note: 'order update parity plan',
        tags: ['parity-plan', 'order-update'],
      },
    });
    expect(missingIdVariables).toEqual({
      input: {
        note: 'order update missing-id parity plan',
        tags: ['parity-plan', 'order-update', 'missing-id'],
      },
    });
    expect(inlineMissingIdVariables).toEqual({});
    expect(inlineNullIdVariables).toEqual({});
    expect(inlineMissingIdFixture).toMatchObject({
      mutation: {
        response: {
          errors: [
            {
              message: "Argument 'id' on InputObject 'OrderInput' is required. Expected type ID!",
              extensions: {
                code: 'missingRequiredInputObjectAttribute',
                argumentName: 'id',
                argumentType: 'ID!',
                inputObjectType: 'OrderInput',
              },
            },
          ],
        },
      },
    });
    expect(inlineNullIdFixture).toMatchObject({
      mutation: {
        response: {
          errors: [
            {
              message: "Argument 'id' on InputObject 'OrderInput' has an invalid value (null). Expected type 'ID!'.",
              extensions: {
                code: 'argumentLiteralsIncompatible',
                typeName: 'InputObject',
                argumentName: 'id',
              },
            },
          ],
        },
      },
    });
    expect(unknownIdFixture).toMatchObject({
      mutation: {
        response: {
          data: {
            orderUpdate: {
              order: null,
              userErrors: [{ field: ['id'], message: 'Order does not exist' }],
            },
          },
        },
      },
    });
    expect(missingIdFixture).toMatchObject({
      mutation: {
        response: {
          errors: [
            {
              message:
                'Variable $input of type OrderInput! was provided invalid value for id (Expected value to not be null)',
              extensions: {
                code: 'INVALID_VARIABLE',
                problems: [{ path: ['id'], explanation: 'Expected value to not be null' }],
              },
            },
          ],
        },
      },
    });
    expect(weirdNotes).toContain('`orderUpdate`');
    expect(weirdNotes).toContain('missingRequiredInputObjectAttribute');
    expect(weirdNotes).toContain('argumentLiteralsIncompatible');
    expect(weirdNotes).toContain('INVALID_VARIABLE');
    expect(weirdNotes).toContain('already-known');
    expect(weirdNotes).toContain('resolves the edit against the effective known-order view');
    expect(weirdNotes).toContain('updates `note` and `tags`');
  });

  it('captures the first live happy-path orderUpdate slice and keeps the parity/reporting artifacts aligned with the narrow local runtime', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];
    const spec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/orderUpdate-live-parity.json'), 'utf8'),
    ) as ParitySpec;
    const document = readFileSync(resolve(repoRoot, 'config/parity-requests/orderUpdate-live-parity.graphql'), 'utf8');
    const variables = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-requests/orderUpdate-live-parity.variables.json'), 'utf8'),
    ) as Record<string, unknown>;
    const fixture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-parity.json'),
        'utf8',
      ),
    ) as Record<string, unknown>;
    const captureScript = readFileSync(resolve(repoRoot, 'scripts/capture-order-conformance.mts'), 'utf8');
    const weirdNotes = readFileSync(resolve(repoRoot, 'docs/hard-and-weird-notes.md'), 'utf8');

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'orderUpdate',
      }),
    );
    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'order-update-live-parity',
        operationNames: ['orderUpdate'],
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-parity.json'],
        paritySpecPath: 'config/parity-specs/orderUpdate-live-parity.json',
      }),
    );
    expect(spec).toEqual(
      expect.objectContaining({
        scenarioId: 'order-update-live-parity',
        scenarioStatus: 'captured',
        liveCaptureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/order-update-parity.json'],
        comparisonMode: 'captured-vs-proxy-request',
        proxyRequest: {
          documentPath: 'config/parity-requests/orderUpdate-live-parity.graphql',
          variablesPath: 'config/parity-requests/orderUpdate-live-parity.variables.json',
        },
      }),
    );
    expect(document).toContain('mutation OrderUpdateLiveParity($input: OrderInput!)');
    expect(document).toContain('orderUpdate(input: $input)');
    expect(document).toContain('updatedAt');
    expect(document).toContain('note');
    expect(document).toContain('tags');
    expect(variables).toEqual({
      input: {
        id: 'gid://shopify/Order/1234567890',
        note: 'order update live parity plan',
        tags: ['live-parity', 'order-update'],
      },
    });
    expect(fixture).toMatchObject({
      mutation: {
        response: {
          data: {
            orderUpdate: {
              order: {
                id: expect.stringMatching(/^gid:\/\/shopify\/Order\//),
                note: 'order update live parity captured note',
                tags: ['live-parity', 'order-update'],
              },
              userErrors: [],
            },
          },
        },
      },
      downstreamRead: {
        response: {
          data: {
            order: {
              id: expect.stringMatching(/^gid:\/\/shopify\/Order\//),
              note: 'order update live parity captured note',
              tags: ['live-parity', 'order-update'],
            },
          },
        },
      },
    });
    expect(captureScript).toContain('order-update-parity.json');
    expect(captureScript).toContain('orderUpdateLiveParity');
    expect(weirdNotes).toContain('immediate downstream `order(id:)` read kept the updated `note` / `tags` visible');
  });
});
