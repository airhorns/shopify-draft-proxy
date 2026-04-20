import { existsSync, readFileSync } from 'node:fs';
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

type InventoryLinkageFixture = {
  inventoryActivateUnknownLocation?: {
    response?: {
      data?: {
        inventoryActivate?: {
          inventoryLevel?: null;
          userErrors?: Array<{ field?: string[]; message?: string }>;
        };
      };
    };
  };
  inventoryBulkToggleUnknownLocation?: {
    response?: {
      data?: {
        inventoryBulkToggleActivation?: {
          inventoryItem?: null;
          inventoryLevels?: null;
          userErrors?: Array<{ field?: string[]; message?: string; code?: string | null }>;
        };
      };
    };
  };
};

describe('inventory linkage live conformance wiring', () => {
  it('wires a dedicated capture script for the inventory linkage mutation family', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const packageJson = JSON.parse(readFileSync(resolve(repoRoot, 'package.json'), 'utf8')) as PackageJson;
    const scriptPath = resolve(repoRoot, 'scripts/capture-inventory-linkage-mutation-conformance.mjs');

    expect(packageJson.scripts?.['conformance:capture-inventory-linkage-mutations']).toBe(
      'node ./scripts/capture-inventory-linkage-mutation-conformance.mjs',
    );
    expect(existsSync(scriptPath)).toBe(true);

    const script = readFileSync(scriptPath, 'utf8');
    expect(script).toContain('inventoryActivate(');
    expect(script).toContain('inventoryDeactivate(');
    expect(script).toContain('inventoryBulkToggleActivation(');
    expect(script).toContain('inventory-linkage-parity.json');
    expect(script).toContain('downstreamReadAfterInventoryActivateSecondLocation');
    expect(script).toContain('downstreamReadAfterInventoryBulkToggleDeactivateSecondLocation');
  });

  it('marks the inventory linkage family covered by captured live success scenarios without the old blocker companion', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const scenarios = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/conformance-scenarios.json'), 'utf8'),
    ) as ConformanceScenario[];

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'inventoryActivate',
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: ['inventory-activate-live-parity'],
        }),
      }),
    );
    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'inventoryDeactivate',
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: ['inventory-deactivate-live-parity'],
        }),
      }),
    );
    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'inventoryBulkToggleActivation',
        conformance: expect.objectContaining({
          status: 'covered',
          scenarioIds: ['inventory-bulk-toggle-activation-live-parity'],
        }),
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'inventory-activate-live-parity',
        status: 'captured',
        paritySpecPath: 'config/parity-specs/inventoryActivate-parity-plan.json',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/inventory-linkage-parity.json'],
      }),
    );
    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'inventory-deactivate-live-parity',
        status: 'captured',
        paritySpecPath: 'config/parity-specs/inventoryDeactivate-parity-plan.json',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/inventory-linkage-parity.json'],
      }),
    );
    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: 'inventory-bulk-toggle-activation-live-parity',
        status: 'captured',
        paritySpecPath: 'config/parity-specs/inventoryBulkToggleActivation-parity-plan.json',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/inventory-linkage-parity.json'],
      }),
    );
    expect(scenarios.find((scenario) => scenario.id === 'inventory-linkage-multi-location-blocker')).toBeUndefined();
  });

  it('keeps the captured location-not-found validation probes for inventoryActivate and inventoryBulkToggleActivation', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const fixture = JSON.parse(
      readFileSync(
        resolve(
          repoRoot,
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/inventory-linkage-parity.json',
        ),
        'utf8',
      ),
    ) as InventoryLinkageFixture;

    expect(fixture.inventoryActivateUnknownLocation).toMatchObject({
      response: {
        data: {
          inventoryActivate: {
            inventoryLevel: null,
            userErrors: [
              {
                field: ['locationId'],
                message: "The product couldn't be stocked because the location wasn't found.",
              },
            ],
          },
        },
      },
    });

    expect(fixture.inventoryBulkToggleUnknownLocation).toMatchObject({
      response: {
        data: {
          inventoryBulkToggleActivation: {
            inventoryItem: null,
            inventoryLevels: null,
            userErrors: [
              {
                field: ['inventoryItemUpdates', '0', 'locationId'],
                message: "The quantity couldn't be updated because the location was not found.",
                code: 'LOCATION_NOT_FOUND',
              },
            ],
          },
        },
      },
    });
  });

  it('captures the promoted multi-location success slices and clears the old blocker note', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const fixture = JSON.parse(
      readFileSync(
        resolve(
          repoRoot,
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/inventory-linkage-parity.json',
        ),
        'utf8',
      ),
    ) as Record<string, unknown>;

    expect(existsSync(resolve(repoRoot, 'pending/inventory-linkage-single-location-blocker.md'))).toBe(false);
    expect(fixture['inventoryActivateSecondLocation']).toMatchObject({
      response: {
        data: {
          inventoryActivate: {
            inventoryLevel: {
              location: {
                id: 'gid://shopify/Location/89026920681',
                name: 'Hermes Conformance Annex',
              },
            },
            userErrors: [],
          },
        },
      },
    });
    expect(fixture['inventoryDeactivateWithAlternateLocation']).toMatchObject({
      response: {
        data: {
          inventoryDeactivate: {
            userErrors: [],
          },
        },
      },
    });
    expect(fixture['inventoryBulkToggleActivateSecondLocation']).toMatchObject({
      response: {
        data: {
          inventoryBulkToggleActivation: {
            userErrors: [],
            inventoryLevels: [
              {
                location: {
                  id: 'gid://shopify/Location/89026920681',
                },
              },
            ],
          },
        },
      },
    });
    expect(fixture['inventoryBulkToggleDeactivateSecondLocation']).toMatchObject({
      response: {
        data: {
          inventoryBulkToggleActivation: {
            userErrors: [],
            inventoryLevels: [],
          },
        },
      },
    });
  });
});
