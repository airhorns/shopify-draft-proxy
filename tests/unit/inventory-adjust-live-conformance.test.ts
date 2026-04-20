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
  comparisonMode: string;
  notes?: string;
};

type InventoryAdjustFixture = {
  mutation?: {
    response?: {
      data?: {
        inventoryAdjustQuantities?: {
          inventoryAdjustmentGroup?: {
            app?: { title?: string | null; handle?: string | null } | null;
            changes?: Array<{ location?: { id?: string; name?: string | null } | null }>;
          } | null;
        };
      };
      errors?: Array<{ path?: Array<string | number>; extensions?: { code?: string; requiredAccess?: string } }>;
    };
  };
  nonAvailableMutation?: {
    variables?: {
      input?: {
        name?: string;
        changes?: Array<{ ledgerDocumentUri?: string }>;
      };
    };
    response?: {
      data?: {
        inventoryAdjustQuantities?: {
          inventoryAdjustmentGroup?: {
            id?: string;
            changes?: Array<{ name?: string; ledgerDocumentUri?: string | null }>;
          } | null;
          userErrors?: Array<{ field?: string[]; message?: string }>;
        };
      };
    };
    downstreamRead?: {
      data?: {
        firstVariant?: { inventoryQuantity?: number | null };
        firstInventoryItem?: {
          inventoryLevels?: {
            nodes?: Array<{
              quantities?: Array<{ name?: string; quantity?: number | null }>;
            }>;
          };
        };
      };
    };
  };
  invalidNameProbe?: {
    response?: {
      data?: {
        inventoryAdjustQuantities?: {
          inventoryAdjustmentGroup?: null;
          userErrors?: Array<{ field?: string[]; message?: string }>;
        };
      };
    };
  };
  missingRequiredFieldProbes?: {
    missingInventoryItemId?: {
      response?: {
        errors?: Array<{
          message?: string;
          extensions?: {
            code?: string;
            problems?: Array<{ path?: Array<string | number>; explanation?: string }>;
          };
        }>;
      };
    };
    missingDelta?: {
      response?: {
        errors?: Array<{
          message?: string;
          extensions?: {
            code?: string;
            problems?: Array<{ path?: Array<string | number>; explanation?: string }>;
          };
        }>;
      };
    };
    missingLocationId?: {
      response?: {
        errors?: Array<{
          message?: string;
          extensions?: {
            code?: string;
            problems?: Array<{ path?: Array<string | number>; explanation?: string }>;
          };
        }>;
      };
    };
    unknownInventoryItemId?: {
      response?: {
        data?: {
          inventoryAdjustQuantities?: {
            inventoryAdjustmentGroup?: null;
            userErrors?: Array<{ field?: string[]; message?: string }>;
          };
        };
      };
    };
    unknownLocationId?: {
      response?: {
        data?: {
          inventoryAdjustQuantities?: {
            inventoryAdjustmentGroup?: null;
            userErrors?: Array<{ field?: string[]; message?: string }>;
          };
        };
      };
    };
  };
};

describe('inventoryAdjustQuantities live conformance wiring', () => {
  it('exposes a package script for the inventory adjustment capture harness', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const packageJson = JSON.parse(readFileSync(resolve(repoRoot, 'package.json'), 'utf8')) as PackageJson;

    expect(packageJson.scripts?.['conformance:capture-inventory-adjustments']).toBe(
      'node ./scripts/capture-inventory-adjustment-conformance.mjs',
    );
  });

  it('marks inventoryAdjustQuantities covered by a captured live scenario', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const scenarios = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/conformance-scenarios.json'), 'utf8'),
    ) as ConformanceScenario[];

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'inventoryAdjustQuantities',
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: ['inventory-adjust-quantities-live-parity'],
        }),
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'inventory-adjust-quantities-live-parity',
        status: 'captured',
        paritySpecPath: 'config/parity-specs/inventoryAdjustQuantities-parity-plan.json',
        captureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/inventory-adjust-quantities-parity.json',
        ],
      }),
    );
  });

  it('upgrades the inventory adjustment parity spec to captured-vs-proxy-request mode', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const spec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/inventoryAdjustQuantities-parity-plan.json'), 'utf8'),
    ) as ParitySpec;

    expect(spec).toEqual(
      expect.objectContaining({
        scenarioId: 'inventory-adjust-quantities-live-parity',
        scenarioStatus: 'captured',
        liveCaptureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/inventory-adjust-quantities-parity.json',
        ],
        comparisonMode: 'captured-vs-proxy-request',
      }),
    );
    expect(spec.notes).toContain('incoming');
    expect(spec.notes).toContain('ledger document URI');
  });

  it('keeps richer live evidence for location names, full app identity, and the staffMember scope blocker', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const fixture = JSON.parse(
      readFileSync(
        resolve(
          repoRoot,
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/inventory-adjust-quantities-parity.json',
        ),
        'utf8',
      ),
    ) as InventoryAdjustFixture;

    expect(fixture.mutation?.response?.data?.inventoryAdjustQuantities?.inventoryAdjustmentGroup?.app).toMatchObject({
      id: 'gid://shopify/App/347082227713',
      title: 'hermes-conformance-products',
      apiKey: expect.stringMatching(/^0db6d7.*ed33$/),
      handle: 'hermes-conformance-products',
    });
    expect(fixture.mutation?.response?.data?.inventoryAdjustQuantities?.inventoryAdjustmentGroup?.changes?.[0]?.location).toMatchObject({
      id: expect.any(String),
      name: expect.any(String),
    });
    expect(fixture.mutation?.response?.errors).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          path: ['inventoryAdjustQuantities', 'inventoryAdjustmentGroup', 'staffMember'],
          extensions: expect.objectContaining({
            code: 'ACCESS_DENIED',
            requiredAccess: expect.stringContaining('read_users'),
          }),
        }),
      ]),
    );
  });

  it('keeps richer live evidence for non-available quantity names and invalid-name errors', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const fixture = JSON.parse(
      readFileSync(
        resolve(
          repoRoot,
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/inventory-adjust-quantities-parity.json',
        ),
        'utf8',
      ),
    ) as InventoryAdjustFixture;

    expect(fixture.nonAvailableMutation).toMatchObject({
      variables: {
        input: {
          name: 'incoming',
          changes: [
            {
              ledgerDocumentUri: expect.any(String),
            },
          ],
        },
      },
      response: {
        data: {
          inventoryAdjustQuantities: {
            inventoryAdjustmentGroup: {
              id: expect.any(String),
              changes: [
                {
                  name: 'incoming',
                  ledgerDocumentUri: expect.any(String),
                },
              ],
            },
            userErrors: [],
          },
        },
      },
      downstreamRead: {
        data: {
          firstVariant: { inventoryQuantity: 1 },
          firstInventoryItem: {
            inventoryLevels: {
              nodes: [
                {
                  quantities: expect.arrayContaining([
                    expect.objectContaining({ name: 'incoming', quantity: 2 }),
                  ]),
                },
              ],
            },
          },
        },
      },
    });

    expect(fixture.invalidNameProbe).toMatchObject({
      response: {
        data: {
          inventoryAdjustQuantities: {
            inventoryAdjustmentGroup: null,
            userErrors: [
              {
                field: ['input', 'name'],
                message:
                  'The specified quantity name is invalid. Valid values are: available, damaged, incoming, quality_control, reserved, safety_stock.',
              },
              {
                field: ['input', 'changes', '0', 'ledgerDocumentUri'],
                message: 'A ledger document URI is required except when adjusting available.',
              },
            ],
          },
        },
      },
    });

    expect(fixture.missingRequiredFieldProbes).toMatchObject({
      missingInventoryItemId: {
        response: {
          errors: [
            {
              message:
                'Variable $input of type InventoryAdjustQuantitiesInput! was provided invalid value for changes.0.inventoryItemId (Expected value to not be null)',
              extensions: {
                code: 'INVALID_VARIABLE',
                problems: [{ path: ['changes', 0, 'inventoryItemId'], explanation: 'Expected value to not be null' }],
              },
            },
          ],
        },
      },
      missingDelta: {
        response: {
          errors: [
            {
              message:
                'Variable $input of type InventoryAdjustQuantitiesInput! was provided invalid value for changes.0.delta (Expected value to not be null)',
              extensions: {
                code: 'INVALID_VARIABLE',
                problems: [{ path: ['changes', 0, 'delta'], explanation: 'Expected value to not be null' }],
              },
            },
          ],
        },
      },
      missingLocationId: {
        response: {
          errors: [
            {
              message:
                'Variable $input of type InventoryAdjustQuantitiesInput! was provided invalid value for changes.0.locationId (Expected value to not be null)',
              extensions: {
                code: 'INVALID_VARIABLE',
                problems: [{ path: ['changes', 0, 'locationId'], explanation: 'Expected value to not be null' }],
              },
            },
          ],
        },
      },
      unknownInventoryItemId: {
        response: {
          data: {
            inventoryAdjustQuantities: {
              inventoryAdjustmentGroup: null,
              userErrors: [
                {
                  field: ['input', 'changes', '0', 'inventoryItemId'],
                  message: 'The specified inventory item could not be found.',
                },
              ],
            },
          },
        },
      },
      unknownLocationId: {
        response: {
          data: {
            inventoryAdjustQuantities: {
              inventoryAdjustmentGroup: null,
              userErrors: [
                {
                  field: ['input', 'changes', '0', 'locationId'],
                  message: 'The specified location could not be found.',
                },
              ],
            },
          },
        },
      },
    });
  });
});
