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
};

type ParitySpec = {
  scenarioId: string;
  scenarioStatus: string;
  liveCaptureFiles: string[];
  comparisonMode: string;
};

type ProductMutationValidationCapture = {
  variables?: Record<string, unknown>;
  response?: {
    data?: Record<string, unknown>;
  };
};

type ProductDeleteInvalidInputCapture = {
  variableMissingId?: {
    variables?: Record<string, unknown>;
    response?: {
      errors?: Array<{
        message?: string;
        extensions?: {
          code?: string;
          value?: Record<string, unknown>;
          problems?: Array<{
            path?: Array<string | number>;
            explanation?: string;
          }>;
        };
      }>;
    };
  };
  inlineMissingId?: {
    response?: {
      errors?: Array<{
        message?: string;
        path?: string[];
        extensions?: {
          code?: string;
          argumentName?: string;
          argumentType?: string;
          inputObjectType?: string;
        };
      }>;
    };
  };
  inlineNullId?: {
    response?: {
      errors?: Array<{
        message?: string;
        path?: string[];
        extensions?: {
          code?: string;
          argumentName?: string;
          typeName?: string;
        };
      }>;
    };
  };
};

type ProductMutationCapture = {
  validation?:
    | ProductMutationValidationCapture
    | {
        unknownId?: ProductMutationValidationCapture;
        blankTitle?: ProductMutationValidationCapture;
        missingId?: ProductMutationValidationCapture;
      };
  invalidInput?: ProductDeleteInvalidInputCapture;
  handleValidation?: {
    createCollision?: {
      variables?: Record<string, unknown>;
      response?: {
        data?: Record<string, unknown>;
      };
    };
    updateCollision?: {
      variables?: Record<string, unknown>;
      response?: {
        data?: Record<string, unknown>;
      };
    };
    createNormalization?: {
      variables?: Record<string, unknown>;
      response?: {
        data?: Record<string, unknown>;
      };
    };
    createWhitespaceNormalization?: {
      variables?: Record<string, unknown>;
      response?: {
        data?: Record<string, unknown>;
      };
    };
    createPunctuationNormalization?: {
      variables?: Record<string, unknown>;
      response?: {
        data?: Record<string, unknown>;
      };
    };
    updateNormalization?: {
      variables?: Record<string, unknown>;
      response?: {
        data?: Record<string, unknown>;
      };
    };
    updateWhitespacePreservesHandle?: {
      variables?: Record<string, unknown>;
      response?: {
        data?: Record<string, unknown>;
      };
    };
    updatePunctuationNormalization?: {
      variables?: Record<string, unknown>;
      response?: {
        data?: Record<string, unknown>;
      };
    };
    updateTitleOnlyPreservesHandle?: {
      variables?: Record<string, unknown>;
      response?: {
        data?: Record<string, unknown>;
      };
    };
  };
};

const expectedLiveFamilies = [
  {
    operationName: 'productCreate',
    scenarioId: 'product-create-live-parity',
    paritySpecPath: 'config/parity-specs/productCreate-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-create-parity.json',
  },
  {
    operationName: 'productUpdate',
    scenarioId: 'product-update-live-parity',
    paritySpecPath: 'config/parity-specs/productUpdate-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-update-parity.json',
  },
  {
    operationName: 'productDelete',
    scenarioId: 'product-delete-live-parity',
    paritySpecPath: 'config/parity-specs/productDelete-parity-plan.json',
    captureFile: 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-delete-parity.json',
  },
] as const;

describe('product mutation live conformance wiring', () => {
  it('exposes a package script for the product mutation capture harness', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const packageJson = JSON.parse(readFileSync(resolve(repoRoot, 'package.json'), 'utf8')) as PackageJson;

    expect(packageJson.scripts?.['conformance:capture-product-mutations']).toBe(
      'node ./scripts/capture-product-mutation-conformance.mjs',
    );
  });

  it('marks the product mutation family covered by captured live scenarios', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const registry = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/operation-registry.json'), 'utf8'),
    ) as OperationRegistryEntry[];
    const scenarios = loadConformanceScenarios(repoRoot) as ConformanceScenario[];

    for (const expected of expectedLiveFamilies) {
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
    }
  });

  it('upgrades the product mutation parity specs to captured-vs-proxy-request mode', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

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

  it('preserves live validation userErrors alongside the happy-path product mutation captures', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const createCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-create-parity.json'),
        'utf8',
      ),
    ) as ProductMutationCapture;
    const updateCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-update-parity.json'),
        'utf8',
      ),
    ) as ProductMutationCapture;
    const deleteCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-delete-parity.json'),
        'utf8',
      ),
    ) as ProductMutationCapture;

    expect(createCapture.validation).toMatchObject({
      variables: { product: { title: '' } },
      response: {
        data: {
          productCreate: {
            product: null,
            userErrors: [{ field: ['title'], message: "Title can't be blank" }],
          },
        },
      },
    });

    expect(updateCapture.validation).toMatchObject({
      unknownId: {
        variables: { product: { id: 'gid://shopify/Product/999999999999999', title: 'Ghost Product' } },
        response: {
          data: {
            productUpdate: {
              product: null,
              userErrors: [{ field: ['id'], message: 'Product does not exist' }],
            },
          },
        },
      },
    });

    expect(deleteCapture.validation).toMatchObject({
      variables: { input: { id: 'gid://shopify/Product/999999999999999' } },
      response: {
        data: {
          productDelete: {
            deletedProductId: null,
            userErrors: [{ field: ['id'], message: 'Product does not exist' }],
          },
        },
      },
    });
  });

  it('captures the live blank-title update validation slice separately from the unknown-id branch', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const updateCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-update-parity.json'),
        'utf8',
      ),
    ) as ProductMutationCapture;

    const validation = updateCapture.validation as {
      unknownId?: ProductMutationValidationCapture;
      blankTitle?: ProductMutationValidationCapture;
      missingId?: ProductMutationValidationCapture;
    };

    expect(validation.blankTitle).toMatchObject({
      variables: {
        product: {
          id: expect.any(String),
          title: '',
        },
      },
      response: {
        data: {
          productUpdate: {
            product: {
              id: expect.any(String),
              title: expect.any(String),
              handle: expect.any(String),
            },
            userErrors: [{ field: ['title'], message: "Title can't be blank" }],
          },
        },
      },
    });

    expect(validation.missingId).toMatchObject({
      variables: {
        product: {
          title: 'Ghost Product Missing Id',
        },
      },
      response: {
        data: {
          productUpdate: {
            product: null,
            userErrors: [{ field: ['id'], message: 'Product does not exist' }],
          },
        },
      },
    });
  });

  it('captures productDelete required-id validation as GraphQL-level errors instead of mutation userErrors', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const deleteCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-delete-parity.json'),
        'utf8',
      ),
    ) as ProductMutationCapture;

    expect(deleteCapture.invalidInput?.variableMissingId).toMatchObject({
      variables: { input: {} },
      response: {
        errors: [
          {
            message:
              'Variable $input of type ProductDeleteInput! was provided invalid value for id (Expected value to not be null)',
            extensions: {
              code: 'INVALID_VARIABLE',
              value: {},
              problems: [{ path: ['id'], explanation: 'Expected value to not be null' }],
            },
          },
        ],
      },
    });

    expect(deleteCapture.invalidInput?.inlineMissingId).toMatchObject({
      response: {
        errors: [
          {
            message: "Argument 'id' on InputObject 'ProductDeleteInput' is required. Expected type ID!",
            path: ['mutation', 'productDelete', 'input', 'id'],
            extensions: {
              code: 'missingRequiredInputObjectAttribute',
              argumentName: 'id',
              argumentType: 'ID!',
              inputObjectType: 'ProductDeleteInput',
            },
          },
        ],
      },
    });

    expect(deleteCapture.invalidInput?.inlineNullId).toMatchObject({
      response: {
        errors: [
          {
            message:
              "Argument 'id' on InputObject 'ProductDeleteInput' has an invalid value (null). Expected type 'ID!'.",
            path: ['mutation', 'productDelete', 'input', 'id'],
            extensions: {
              code: 'argumentLiteralsIncompatible',
              argumentName: 'id',
              typeName: 'InputObject',
            },
          },
        ],
      },
    });
  });

  it('captures explicit product handle collision validation for create and update', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const createCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-create-parity.json'),
        'utf8',
      ),
    ) as ProductMutationCapture;
    const updateCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-update-parity.json'),
        'utf8',
      ),
    ) as ProductMutationCapture;

    expect(createCapture.handleValidation?.createCollision).toMatchObject({
      variables: {
        product: {
          title: expect.any(String),
          handle: expect.any(String),
        },
      },
      response: {
        data: {
          productCreate: {
            product: null,
            userErrors: [
              {
                field: ['input', 'handle'],
                message: expect.stringContaining('already in use. Please provide a new handle.'),
              },
            ],
          },
        },
      },
    });

    expect(updateCapture.handleValidation?.updateCollision).toMatchObject({
      variables: {
        product: {
          id: expect.any(String),
          handle: expect.any(String),
        },
      },
      response: {
        data: {
          productUpdate: {
            product: {
              id: expect.any(String),
              handle: expect.any(String),
            },
            userErrors: [
              {
                field: ['input', 'handle'],
                message: expect.stringContaining('already in use. Please provide a new handle.'),
              },
            ],
          },
        },
      },
    });
  });

  it('captures explicit product handle normalization for create and update, including whitespace-only fallbacks', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const createCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-create-parity.json'),
        'utf8',
      ),
    ) as ProductMutationCapture;
    const updateCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-update-parity.json'),
        'utf8',
      ),
    ) as ProductMutationCapture;

    expect(createCapture.handleValidation?.createNormalization).toMatchObject({
      variables: {
        product: {
          title: expect.any(String),
          handle: '  Weird Handle / 100%  ',
        },
      },
      response: {
        data: {
          productCreate: {
            product: {
              handle: 'weird-handle-100',
            },
            userErrors: [],
          },
        },
      },
    });

    expect(createCapture.handleValidation?.createWhitespaceNormalization).toMatchObject({
      variables: {
        product: {
          title: expect.any(String),
          handle: '   ',
        },
      },
      response: {
        data: {
          productCreate: {
            product: {
              handle: expect.stringContaining('whitespace-handle-probe-'),
            },
            userErrors: [],
          },
        },
      },
    });

    expect(createCapture.handleValidation?.createPunctuationNormalization).toMatchObject({
      variables: {
        product: {
          title: expect.any(String),
          handle: '%%%',
        },
      },
      response: {
        data: {
          productCreate: {
            product: {
              handle: expect.stringMatching(/^product(?:-\d+)?$/),
            },
            userErrors: [],
          },
        },
      },
    });

    expect(updateCapture.handleValidation?.updateNormalization).toMatchObject({
      variables: {
        product: {
          id: expect.any(String),
          handle: '  Mixed CASE/ Weird 200 % ',
        },
      },
      response: {
        data: {
          productUpdate: {
            product: {
              id: expect.any(String),
              handle: 'mixed-case-weird-200',
            },
            userErrors: [],
          },
        },
      },
    });

    expect(updateCapture.handleValidation?.updateWhitespacePreservesHandle).toMatchObject({
      variables: {
        product: {
          id: expect.any(String),
          handle: '   ',
        },
      },
      response: {
        data: {
          productUpdate: {
            product: {
              id: expect.any(String),
              handle: expect.stringContaining('whitespace-handle-probe-'),
            },
            userErrors: [],
          },
        },
      },
    });

    expect(updateCapture.handleValidation?.updatePunctuationNormalization).toMatchObject({
      variables: {
        product: {
          id: expect.any(String),
          handle: '%%%',
        },
      },
      response: {
        data: {
          productUpdate: {
            product: {
              id: expect.any(String),
              handle: expect.stringMatching(/^product(?:-\d+)?$/),
            },
            userErrors: [],
          },
        },
      },
    });
  });

  it('captures the live title-only update slice keeping the current handle stable', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const updateCapture = JSON.parse(
      readFileSync(
        resolve(repoRoot, 'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-update-parity.json'),
        'utf8',
      ),
    ) as ProductMutationCapture;

    expect(updateCapture.handleValidation?.updateTitleOnlyPreservesHandle).toMatchObject({
      variables: {
        product: {
          id: expect.any(String),
          title: expect.stringContaining('Title Only'),
        },
      },
      response: {
        data: {
          productUpdate: {
            product: {
              id: expect.any(String),
              title: expect.stringContaining('Updated'),
              handle: expect.stringMatching(/^title-only-handle-probe-\d+$/),
            },
            userErrors: [],
          },
        },
      },
    });
  });
});
