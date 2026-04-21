import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

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
  notes?: string;
};

type InventoryItemUpdateCapture = {
  mutation?: {
    response?: {
      data?: {
        inventoryItemUpdate?: {
          inventoryItem?: {
            tracked?: boolean;
            requiresShipping?: boolean;
            countryCodeOfOrigin?: string;
            provinceCodeOfOrigin?: string;
            harmonizedSystemCode?: string;
            measurement?: {
              weight?: {
                unit?: string;
                value?: number;
              };
            };
          } | null;
          userErrors?: Array<{ field?: string[] | null; message?: string }>;
        };
      };
    };
    downstreamRead?: {
      data?: {
        inventoryItem?: {
          tracked?: boolean;
          requiresShipping?: boolean;
          countryCodeOfOrigin?: string;
          provinceCodeOfOrigin?: string;
          harmonizedSystemCode?: string;
          measurement?: {
            weight?: {
              unit?: string;
              value?: number;
            };
          };
        };
      };
    };
  };
  validation?: {
    response?: {
      data?: {
        inventoryItemUpdate?: {
          inventoryItem?: null;
          userErrors?: Array<{ field?: string[] | null; message?: string }>;
        };
      };
    };
  };
};

const repoRoot = resolve(import.meta.dirname, '../..');
const scenarioId = 'inventory-item-update-live-parity';
const paritySpecPath = 'config/parity-specs/inventoryItemUpdate-parity-plan.json';
const captureFile = 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/inventory-item-update-parity.json';

describe('inventoryItemUpdate live conformance wiring', () => {
  it('marks inventoryItemUpdate covered by a captured live scenario', () => {
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];

    expect(registry).toContainEqual(
      expect.objectContaining({
        name: 'inventoryItemUpdate',
      }),
    );

    expect(scenarios).toContainEqual(
      expect.objectContaining({
        id: scenarioId,
        status: 'captured',
        paritySpecPath,
        captureFiles: [captureFile],
      }),
    );

    expect(existsSync(resolve(repoRoot, captureFile))).toBe(true);
  });

  it('upgrades the inventoryItemUpdate parity spec to captured-vs-proxy-request mode', () => {
    const spec = JSON.parse(readFileSync(resolve(repoRoot, paritySpecPath), 'utf8')) as ParitySpec;
    expect(spec).toEqual(
      expect.objectContaining({
        scenarioId,
        scenarioStatus: 'captured',
        liveCaptureFiles: [captureFile],
        comparisonMode: 'captured-vs-proxy-request',
      }),
    );
    expect(spec.notes).toContain('tracked');
    expect(spec.notes).toContain('does not exist');
  });

  it('preserves both the happy-path inventory item mutation payload and the unknown-id validation slice', () => {
    const fixture = JSON.parse(readFileSync(resolve(repoRoot, captureFile), 'utf8')) as InventoryItemUpdateCapture;

    expect(fixture.mutation).toMatchObject({
      response: {
        data: {
          inventoryItemUpdate: {
            inventoryItem: {
              tracked: true,
              requiresShipping: false,
              countryCodeOfOrigin: 'CA',
              provinceCodeOfOrigin: 'ON',
              harmonizedSystemCode: '620343',
              measurement: {
                weight: {
                  unit: 'KILOGRAMS',
                  value: 2.5,
                },
              },
            },
            userErrors: [],
          },
        },
      },
      downstreamRead: {
        data: {
          inventoryItem: {
            tracked: true,
            requiresShipping: false,
            countryCodeOfOrigin: 'CA',
            provinceCodeOfOrigin: 'ON',
            harmonizedSystemCode: '620343',
            measurement: {
              weight: {
                unit: 'KILOGRAMS',
                value: 2.5,
              },
            },
          },
        },
      },
    });

    expect(fixture.validation).toMatchObject({
      response: {
        data: {
          inventoryItemUpdate: {
            inventoryItem: null,
            userErrors: [{ field: ['id'], message: "The product couldn't be updated because it does not exist." }],
          },
        },
      },
    });
  });
});
