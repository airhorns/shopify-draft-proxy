import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  classifyParityScenarioState,
  compareJsonPayloads,
  executeParityScenario,
  excludeComparisonPaths,
  summarizeParityResults,
  validateComparisonContract,
  validateParityScenarioInventoryEntry,
} from '../../scripts/conformance-parity-lib.js';
import { listConformanceParitySpecPaths } from '../../scripts/conformance-scenario-registry.js';

describe('classifyParityScenarioState', () => {
  it('marks captured scenarios invalid until they have a strict comparison contract and proxy request', () => {
    expect(
      classifyParityScenarioState(
        { status: 'captured' },
        {
          proxyRequest: {
            documentPath: 'config/parity-requests/products/productCreate.graphql',
            variablesPath: 'config/parity-requests/products/productCreate.json',
          },
        },
      ),
    ).toBe('invalid-missing-comparison-contract');

    expect(
      classifyParityScenarioState(
        { status: 'captured' },
        {
          comparison: {
            mode: 'strict-json',
            expectedDifferences: [],
          },
        },
      ),
    ).toBe('invalid-missing-comparison-contract');
  });

  it('marks captured scenarios with proxy requests and comparison contracts as ready', () => {
    const state = classifyParityScenarioState(
      { status: 'captured' },
      {
        proxyRequest: {
          documentPath: 'config/parity-requests/products/productCreate.graphql',
          variablesPath: 'config/parity-requests/products/productCreate.json',
        },
        comparison: {
          mode: 'strict-json',
          expectedDifferences: [],
          targets: [
            {
              name: 'mutation-data',
              capturePath: '$.mutation.response.data',
              proxyPath: '$.data',
            },
          ],
        },
      },
    );

    expect(state).toBe('ready-for-comparison');
  });

  it('marks captured scenarios without comparison targets as invalid even when the contract shape is valid', () => {
    expect(
      classifyParityScenarioState(
        { status: 'captured' },
        {
          proxyRequest: {
            documentPath: 'config/parity-requests/products/productCreate.graphql',
            variablesPath: 'config/parity-requests/products/productCreate.json',
          },
          comparison: {
            mode: 'strict-json',
            expectedDifferences: [],
          },
        },
      ),
    ).toBe('invalid-missing-comparison-contract');

    expect(
      classifyParityScenarioState(
        { status: 'captured' },
        {
          proxyRequest: {
            documentPath: 'config/parity-requests/products/productCreate.graphql',
            variablesPath: 'config/parity-requests/products/productCreate.json',
          },
          comparison: {
            mode: 'strict-json',
            expectedDifferences: [],
            targets: [],
          },
        },
      ),
    ).toBe('invalid-missing-comparison-contract');
  });

  it('classifies captured fixture scenarios as externally enforced evidence', () => {
    const state = classifyParityScenarioState(
      { status: 'captured' },
      {
        comparisonMode: 'captured-fixture',
        liveCaptureFiles: ['fixtures/conformance/example.json'],
        runtimeTestFiles: ['tests/integration/example.test.ts'],
        comparison: {
          mode: 'strict-json',
          expectedDifferences: [],
        },
      },
    );

    expect(state).toBe('enforced-by-fixture');
  });

  it('keeps planned scenarios as not-yet-implemented even when a proxy request scaffold exists', () => {
    const state = classifyParityScenarioState(
      { status: 'planned' },
      {
        proxyRequest: {
          documentPath: 'config/parity-requests/products/productCreate.graphql',
          variablesPath: 'config/parity-requests/products/productCreate.json',
        },
      },
    );

    expect(state).toBe('not-yet-implemented');
  });
});

describe('summarizeParityResults', () => {
  it('separates ready, invalid, and not-yet-implemented scenario states', () => {
    const summary = summarizeParityResults([
      { state: 'ready-for-comparison' },
      { state: 'enforced-by-fixture' },
      { state: 'invalid-missing-comparison-contract' },
      { state: 'invalid-missing-comparison-contract' },
      { state: 'not-yet-implemented' },
    ]);

    expect(summary.readyForComparison).toBe(1);
    expect(summary.pending).toBe(3);
    expect(summary.statusCounts).toEqual({
      readyForComparison: 1,
      enforcedByFixture: 1,
      invalidMissingComparisonContract: 2,
      notYetImplemented: 1,
    });
    expect(summary.statusNote).toContain('notYetImplemented');
  });
});

describe('validateParityScenarioInventoryEntry', () => {
  it('rejects captured scenarios that are checked in without executable comparison targets', () => {
    expect(
      validateParityScenarioInventoryEntry(
        {
          id: 'captured-without-targets',
          status: 'captured',
          captureFiles: ['fixtures/conformance/example.json'],
        },
        {
          comparisonMode: 'captured-vs-proxy-request',
          proxyRequest: {
            documentPath: 'config/parity-requests/products/example.graphql',
          },
          comparison: {
            mode: 'strict-json',
            expectedDifferences: [],
          },
        },
      ),
    ).toEqual(['Captured scenario captured-without-targets must declare at least one executable comparison target.']);
  });

  it('requires captured fixture scenarios to name their runtime enforcement', () => {
    expect(
      validateParityScenarioInventoryEntry(
        {
          id: 'captured-fixture-without-runtime-test',
          status: 'captured',
          captureFiles: ['fixtures/conformance/example.json'],
        },
        {
          comparisonMode: 'captured-fixture',
          liveCaptureFiles: ['fixtures/conformance/example.json'],
          comparison: {
            mode: 'strict-json',
            expectedDifferences: [],
          },
        },
      ),
    ).toEqual([
      'Captured fixture scenario captured-fixture-without-runtime-test must reference at least one runtime test file.',
    ]);
  });
});

describe('validateComparisonContract', () => {
  it('requires expected differences to be path-scoped, documented, and typed', () => {
    expect(
      validateComparisonContract({
        mode: 'strict-json',
        expectedDifferences: [
          {
            path: '$.data.product.id',
            matcher: 'shopify-gid:Product',
            reason: 'Product ids are allocated independently per parity run.',
          },
        ],
      }),
    ).toEqual([]);

    expect(
      validateComparisonContract({
        mode: 'strict-json',
        expectedDifferences: [
          {
            path: '$.data.product.tags',
            ignore: true,
            regrettable: true,
            reason: 'The proxy preserves tag membership but does not yet preserve Shopify tag ordering.',
          },
        ],
      }),
    ).toEqual([]);

    expect(
      validateComparisonContract({
        mode: 'strict-json',
        expectedDifferences: [
          {
            path: '$.data.product.id',
            matcher: 'everything',
          },
          {
            reason: 'This rule is missing a path and action.',
          },
          {
            path: '$.data.product.tags',
            ignore: true,
            reason: 'This ignored gap must be explicitly marked regrettable.',
          },
          {
            path: '$.data.product.options',
            ignore: true,
            regrettable: false,
            reason: 'Regrettable is only a positive marker.',
          },
        ],
      }),
    ).toEqual([
      'expectedDifferences[0] must document why the expected difference is accepted.',
      'expectedDifferences[0] declares unknown matcher `everything`.',
      'expectedDifferences[1] must declare a non-empty JSON path.',
      'expectedDifferences[1] must declare exactly one of `matcher` or `ignore: true`.',
      'expectedDifferences[2] with `ignore: true` must set `regrettable: true` for the parity gap.',
      'expectedDifferences[3] `regrettable`, when declared, must be true.',
      'expectedDifferences[3] with `ignore: true` must set `regrettable: true` for the parity gap.',
    ]);
  });

  it('rejects the legacy allowedDifferences contract key', () => {
    expect(
      validateComparisonContract({
        mode: 'strict-json',
        allowedDifferences: [],
      }),
    ).toEqual([
      'Comparison contract must use `expectedDifferences`; `allowedDifferences` is no longer supported.',
      'Comparison contract must declare an `expectedDifferences` array.',
    ]);
  });

  it('validates excluded comparison paths on comparison targets', () => {
    expect(
      validateComparisonContract({
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [
          {
            name: 'node',
            capturePath: '$.data.node',
            proxyPath: '$.data.node',
            excludedPaths: ['$.id', '$.createdAt'],
          },
        ],
      }),
    ).toEqual([]);

    expect(
      validateComparisonContract({
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [
          {
            name: 'node',
            capturePath: '$.data.node',
            proxyPath: '$.data.node',
            selectedPaths: ['$.name'],
            excludedPaths: [],
          },
        ],
      }),
    ).toEqual([
      'targets[0] excludedPaths, when declared, must be a non-empty array.',
      'targets[0] must not declare both selectedPaths and excludedPaths.',
    ]);
  });

  it('requires every repository ignore rule to be explicitly regrettable', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const unmarkedIgnores: string[] = [];

    for (const paritySpecPath of listConformanceParitySpecPaths(repoRoot)) {
      const spec = JSON.parse(readFileSync(resolve(repoRoot, paritySpecPath), 'utf8')) as {
        comparison?: {
          expectedDifferences?: Array<{
            path?: string;
            ignore?: boolean;
            regrettable?: true;
          }>;
        };
      };

      for (const difference of spec.comparison?.expectedDifferences ?? []) {
        if (difference.ignore === true && difference.regrettable !== true) {
          unmarkedIgnores.push(`${paritySpecPath}:${difference.path ?? '<missing path>'}`);
        }
      }
    }

    expect(unmarkedIgnores).toEqual([]);
  });
});

describe('compareJsonPayloads', () => {
  it('is strict by default for API-visible missing, extra, and changed fields', () => {
    const result = compareJsonPayloads(
      {
        data: {
          productCreate: {
            product: {
              id: 'gid://shopify/Product/1',
              title: 'Shopify title',
            },
            userErrors: [],
          },
        },
      },
      {
        data: {
          productCreate: {
            product: {
              id: 'gid://shopify/Product/2',
              handle: 'shopify-title',
              title: 'Proxy title',
            },
          },
        },
      },
    );

    expect(result.ok).toBe(false);
    expect(
      result.differences.map((difference: { path: string; message: string }) => [difference.path, difference.message]),
    ).toEqual([
      ['$.data.productCreate.product.handle', 'Unexpected field in actual payload.'],
      ['$.data.productCreate.product.id', 'Value differs.'],
      ['$.data.productCreate.product.title', 'Value differs.'],
      ['$.data.productCreate.userErrors', 'Missing field in actual payload.'],
    ]);
  });

  it('allows only path-scoped nondeterministic values that still match the declared type', () => {
    const expected = {
      data: {
        productCreate: {
          product: {
            id: 'gid://shopify/Product/123',
            title: 'Hat',
            createdAt: '2026-04-19T20:00:00.000Z',
          },
          userErrors: [],
        },
      },
      extensions: {
        cost: {
          throttleStatus: {
            currentlyAvailable: 1988,
          },
        },
      },
    };
    const actual = {
      data: {
        productCreate: {
          product: {
            id: 'gid://shopify/Product/999',
            title: 'Hat',
            createdAt: '2026-04-19T20:00:02.000Z',
          },
          userErrors: [],
        },
      },
      extensions: {
        cost: {
          throttleStatus: {
            currentlyAvailable: 42,
          },
        },
      },
    };

    expect(
      compareJsonPayloads(expected, actual, {
        expectedDifferences: [
          {
            path: '$.data.productCreate.product.id',
            matcher: 'shopify-gid:Product',
            reason: 'Shopify and the proxy allocate different product ids during isolated parity runs.',
          },
          {
            path: '$.data.productCreate.product.createdAt',
            matcher: 'iso-timestamp',
            reason: 'Creation timestamps are generated independently per parity run.',
          },
          {
            path: '$.extensions.cost.throttleStatus.currentlyAvailable',
            matcher: 'any-number',
            reason: 'Shopify throttle bucket availability depends on recent store traffic.',
          },
        ],
      }).ok,
    ).toBe(true);

    expect(
      compareJsonPayloads(
        expected,
        {
          ...actual,
          data: {
            productCreate: { product: { ...actual.data.productCreate.product, id: 'not-a-gid' }, userErrors: [] },
          },
        },
        {
          expectedDifferences: [{ path: '$.data.productCreate.product.id', matcher: 'shopify-gid:Product' }],
        },
      ).differences,
    ).toEqual([
      {
        path: '$.data.productCreate.product.createdAt',
        message: 'Value differs.',
        expected: '2026-04-19T20:00:00.000Z',
        actual: '2026-04-19T20:00:02.000Z',
      },
      {
        path: '$.data.productCreate.product.id',
        message: 'Value differs.',
        expected: 'gid://shopify/Product/123',
        actual: 'not-a-gid',
      },
      {
        path: '$.extensions.cost.throttleStatus.currentlyAvailable',
        message: 'Value differs.',
        expected: 1988,
        actual: 42,
      },
    ]);
  });

  it('fails when an expected difference is not observed', () => {
    const result = compareJsonPayloads(
      {
        data: {
          product: {
            id: 'gid://shopify/Product/123',
            title: 'Hat',
          },
        },
      },
      {
        data: {
          product: {
            id: 'gid://shopify/Product/123',
            title: 'Hat',
          },
        },
      },
      {
        expectedDifferences: [
          {
            path: '$.data.product.id',
            matcher: 'shopify-gid:Product',
            reason: 'Product ids should differ between Shopify and the proxy harness.',
          },
        ],
      },
    );

    expect(result.ok).toBe(false);
    expect(result.differences).toEqual([
      {
        path: '$.data.product.id',
        message: 'Expected difference was not observed.',
        expected: undefined,
        actual: undefined,
      },
    ]);
  });

  it('supports array wildcards for repeated generated ids without ignoring sibling fields', () => {
    const result = compareJsonPayloads(
      {
        data: {
          product: {
            options: [
              {
                id: 'gid://shopify/ProductOption/1',
                name: 'Color',
              },
            ],
          },
        },
      },
      {
        data: {
          product: {
            options: [
              {
                id: 'gid://shopify/ProductOption/2',
                name: 'Shade',
              },
            ],
          },
        },
      },
      {
        expectedDifferences: [
          {
            path: '$.data.product.options[*].id',
            matcher: 'shopify-gid:ProductOption',
            reason: 'Option ids are generated independently per parity run.',
          },
        ],
      },
    );

    expect(result.ok).toBe(false);
    expect(result.differences).toEqual([
      {
        path: '$.data.product.options[0].name',
        message: 'Value differs.',
        expected: 'Color',
        actual: 'Shade',
      },
    ]);
  });
});

describe('excludeComparisonPaths', () => {
  it('removes ignored fields from cloned payloads without mutating the source', () => {
    const payload = {
      data: {
        nodes: [
          { id: 'gid://shopify/Product/1', title: 'Hat', createdAt: '2026-04-27T00:00:00Z' },
          { id: 'gid://shopify/Product/2', title: 'Shirt', createdAt: '2026-04-27T00:00:01Z' },
        ],
      },
    };

    expect(excludeComparisonPaths(payload, ['$.data.nodes[*].id', '$.data.nodes[*].createdAt'])).toEqual({
      data: {
        nodes: [{ title: 'Hat' }, { title: 'Shirt' }],
      },
    });
    expect(payload.data.nodes[0]).toEqual({
      id: 'gid://shopify/Product/1',
      title: 'Hat',
      createdAt: '2026-04-27T00:00:00Z',
    });
  });
});

describe('executeParityScenario', () => {
  it('returns captured upstream payloads for no-write overlay reads', async () => {
    const repoRoot = new URL('../..', import.meta.url).pathname;
    const result = await executeParityScenario({
      repoRoot,
      scenario: {
        id: 'product-detail-read',
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-detail.json'],
      },
      paritySpec: {
        proxyRequest: {
          documentPath: 'config/parity-requests/products/product-detail-read.graphql',
          variablesPath: 'config/parity-requests/products/product-detail-read.variables.json',
        },
        comparison: {
          mode: 'strict-json',
          expectedDifferences: [],
          targets: [
            {
              name: 'read-data',
              capturePath: '$.data',
              proxyPath: '$.data',
              upstreamCapturePath: '$',
            },
          ],
        },
      },
    });

    expect(result.ok).toBe(true);
    expect(result.comparisons).toEqual([
      {
        name: 'read-data',
        ok: true,
        differences: [],
      },
    ]);
  });
});
