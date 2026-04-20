import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

type PackageJson = {
  scripts?: Record<string, string>;
};

type ScenarioRegistryEntry = {
  id: string;
  operationNames: string[];
  status: string;
  captureFiles: string[];
  paritySpecPath: string;
};

type OperationRegistryEntry = {
  name: string;
  conformance?: {
    status?: string;
    scenarioIds?: string[];
  };
};

type ParitySpec = {
  scenarioId: string;
  scenarioStatus: string;
  liveCaptureFiles: string[];
  comparisonMode: string;
};

type ProductGraphCapture = {
  handleParity?: {
    duplicateCollision?: {
      firstDuplicate?: {
        data?: {
          productDuplicate?: {
            newProduct?: {
              handle?: string;
            };
          };
        };
      };
      secondDuplicate?: {
        data?: {
          productDuplicate?: {
            newProduct?: {
              handle?: string;
            };
          };
        };
      };
    };
    productSetCreateCollision?: {
      firstCreate?: {
        data?: {
          productSet?: {
            product?: {
              handle?: string;
            };
          };
        };
      };
      secondCreate?: {
        data?: {
          productSet?: {
            product?: {
              handle?: string;
            };
          };
        };
      };
    };
    productSetExplicitNormalization?: {
      variables?: Record<string, unknown>;
      response?: {
        data?: {
          productSet?: {
            product?: {
              id?: string;
              handle?: string;
            };
            userErrors?: unknown[];
          };
        };
      };
    };
  };
};

const expectedLiveFamilies = [
  {
    operationName: 'productDuplicate',
    scenarioId: 'product-duplicate-live-parity',
    paritySpecPath: 'config/parity-specs/productDuplicate-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-duplicate-parity.json',
  },
  {
    operationName: 'productSet',
    scenarioId: 'product-set-live-parity',
    paritySpecPath: 'config/parity-specs/productSet-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-set-parity.json',
  },
] as const;

const repoRoot = resolve(import.meta.dirname, '../..');
const packageJson = JSON.parse(readFileSync(resolve(repoRoot, 'package.json'), 'utf8')) as PackageJson;
const scenarioRegistry = JSON.parse(
  readFileSync(resolve(repoRoot, 'config/conformance-scenarios.json'), 'utf8'),
) as ScenarioRegistryEntry[];
const operationRegistry = JSON.parse(
  readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
) as OperationRegistryEntry[];

function expectCapturedScenario(options: {
  id: string;
  operationName: string;
  captureFile: string;
  paritySpecPath: string;
}) {
  const scenario = scenarioRegistry.find((entry) => entry.id === options.id);
  expect(scenario).toBeDefined();
  expect(scenario?.operationNames).toEqual([options.operationName]);
  expect(scenario?.status).toBe('captured');
  expect(scenario?.paritySpecPath).toBe(options.paritySpecPath);
  expect(scenario?.captureFiles).toEqual([options.captureFile]);
  expect(existsSync(resolve(repoRoot, options.captureFile))).toBe(true);
}

function expectCoveredOperation(options: { name: string; scenarioId: string }) {
  const operation = operationRegistry.find((entry) => entry.name === options.name);
  expect(operation).toBeDefined();
  expect(operation?.conformance?.status).toBe('covered');
  expect(operation?.conformance?.scenarioIds).toEqual([options.scenarioId]);
}

describe('product graph live conformance coverage', () => {
  it('adds a runnable capture script for the duplicate + set mutation family', () => {
    expect(packageJson.scripts?.['conformance:capture-product-graph-mutations']).toBe(
      'node ./scripts/capture-product-graph-mutation-conformance.mjs',
    );
  });

  it('promotes productDuplicate and productSet to captured live parity', () => {
    for (const expected of expectedLiveFamilies) {
      expectCapturedScenario({
        id: expected.scenarioId,
        operationName: expected.operationName,
        captureFile: expected.captureFile,
        paritySpecPath: expected.paritySpecPath,
      });
      expectCoveredOperation({
        name: expected.operationName,
        scenarioId: expected.scenarioId,
      });
    }
  });

  it('upgrades the duplicate + set parity specs to captured-vs-proxy-request mode', () => {
    for (const expected of expectedLiveFamilies) {
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

  it('captures live handle de-duplication for duplicate and productSet create collisions', () => {
    const duplicateCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-duplicate-parity.json'),
        'utf8',
      ),
    ) as ProductGraphCapture;
    const setCapture = JSON.parse(
      readFileSync(resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-set-parity.json'), 'utf8'),
    ) as ProductGraphCapture;

    expect(duplicateCapture.handleParity?.duplicateCollision).toMatchObject({
      firstDuplicate: {
        data: {
          productDuplicate: {
            newProduct: {
              handle: expect.any(String),
            },
          },
        },
      },
      secondDuplicate: {
        data: {
          productDuplicate: {
            newProduct: {
              handle: expect.stringMatching(/-1$|\d+$/),
            },
          },
        },
      },
    });
    expect(
      duplicateCapture.handleParity?.duplicateCollision?.firstDuplicate?.data?.productDuplicate?.newProduct?.handle,
    ).not.toBe(
      duplicateCapture.handleParity?.duplicateCollision?.secondDuplicate?.data?.productDuplicate?.newProduct?.handle,
    );

    expect(setCapture.handleParity?.productSetCreateCollision).toMatchObject({
      firstCreate: {
        data: {
          productSet: {
            product: {
              handle: expect.any(String),
            },
          },
        },
      },
      secondCreate: {
        data: {
          productSet: {
            product: {
              handle: expect.stringMatching(/-1$|\d+$/),
            },
          },
        },
      },
    });
    expect(setCapture.handleParity?.productSetCreateCollision?.firstCreate?.data?.productSet?.product?.handle).not.toBe(
      setCapture.handleParity?.productSetCreateCollision?.secondCreate?.data?.productSet?.product?.handle,
    );
  });

  it('captures explicit productSet handle normalization', () => {
    const setCapture = JSON.parse(
      readFileSync(resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-set-parity.json'), 'utf8'),
    ) as ProductGraphCapture;

    expect(setCapture.handleParity?.productSetExplicitNormalization).toMatchObject({
      variables: {
        identifier: {
          id: expect.any(String),
        },
        input: {
          title: expect.any(String),
          handle: '  Another Weird/Handle 300 % ',
        },
      },
      response: {
        data: {
          productSet: {
            product: {
              id: expect.any(String),
              handle: 'another-weird-handle-300',
            },
            userErrors: [],
          },
        },
      },
    });
  });
});
