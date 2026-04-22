import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  classifyParityScenarioState,
  compareJsonPayloads,
  executeParityScenario,
  summarizeParityResults,
  validateComparisonContract,
} from '../../scripts/conformance-parity-lib.js';

describe('classifyParityScenarioState', () => {
  it('marks captured scenarios invalid until they have a strict comparison contract and proxy request', () => {
    expect(
      classifyParityScenarioState(
        { status: 'captured' },
        {
          proxyRequest: {
            documentPath: 'config/parity-requests/productCreate.graphql',
            variablesPath: 'config/parity-requests/productCreate.json',
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
          documentPath: 'config/parity-requests/productCreate.graphql',
          variablesPath: 'config/parity-requests/productCreate.json',
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
            documentPath: 'config/parity-requests/productCreate.graphql',
            variablesPath: 'config/parity-requests/productCreate.json',
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
            documentPath: 'config/parity-requests/productCreate.graphql',
            variablesPath: 'config/parity-requests/productCreate.json',
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

  it('keeps planned scenarios as not-yet-implemented even when a proxy request scaffold exists', () => {
    const state = classifyParityScenarioState(
      { status: 'planned' },
      {
        proxyRequest: {
          documentPath: 'config/parity-requests/productCreate.graphql',
          variablesPath: 'config/parity-requests/productCreate.json',
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
      { state: 'invalid-missing-comparison-contract' },
      { state: 'invalid-missing-comparison-contract' },
      { state: 'not-yet-implemented' },
    ]);

    expect(summary.readyForComparison).toBe(1);
    expect(summary.pending).toBe(3);
    expect(summary.statusCounts).toEqual({
      readyForComparison: 1,
      invalidMissingComparisonContract: 2,
      notYetImplemented: 1,
    });
    expect(summary.statusNote).toContain('notYetImplemented');
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

  it('requires every repository ignore rule to be explicitly regrettable', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const paritySpecRoot = resolve(repoRoot, 'config/parity-specs');
    const unmarkedIgnores: string[] = [];

    for (const fileName of readdirSync(paritySpecRoot)
      .filter((name) => name.endsWith('.json'))
      .sort()) {
      const spec = JSON.parse(readFileSync(resolve(paritySpecRoot, fileName), 'utf8')) as {
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
          unmarkedIgnores.push(`${fileName}:${difference.path ?? '<missing path>'}`);
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

describe('executeParityScenario', () => {
  it('executes the promoted productCreate captured scenario against the local proxy harness', async () => {
    const repoRoot = new URL('../..', import.meta.url).pathname;
    const result = await executeParityScenario({
      repoRoot,
      scenario: {
        id: 'product-create-live-parity',
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-create-parity.json'],
      },
      paritySpec: {
        proxyRequest: {
          documentPath: 'config/parity-requests/productCreate-parity-plan.graphql',
          variablesPath: 'config/parity-requests/productCreate-parity-plan.variables.json',
          variablesCapturePath: '$.mutation.variables',
        },
        comparison: {
          mode: 'strict-json',
          expectedDifferences: [
            {
              path: '$.productCreate.product.id',
              matcher: 'shopify-gid:Product',
              reason: 'Synthetic local product id.',
            },
            {
              path: '$.product.id',
              matcher: 'shopify-gid:Product',
              reason: 'Synthetic local product id.',
            },
          ],
          targets: [
            {
              name: 'mutation-data',
              capturePath: '$.mutation.response.data',
              proxyPath: '$.data',
            },
            {
              name: 'downstream-read-data',
              capturePath: '$.downstreamRead.data',
              proxyRequest: {
                documentPath: 'config/parity-requests/productCreate-downstream-read.graphql',
                variables: {
                  id: {
                    fromPrimaryProxyPath: '$.data.productCreate.product.id',
                  },
                },
              },
              proxyPath: '$.data',
            },
          ],
        },
      },
    });

    expect(result.ok).toBe(true);
    expect(result.primaryProxyStatus).toBe(200);
    expect(result.comparisons.map((comparison) => comparison.name)).toEqual(['mutation-data', 'downstream-read-data']);
  });

  it('executes the promoted tagsAdd captured scenario with immediate tag-search lag', async () => {
    const repoRoot = new URL('../..', import.meta.url).pathname;
    const paritySpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/tagsAdd-parity-plan.json'), 'utf8'),
    ) as Parameters<typeof executeParityScenario>[0]['paritySpec'];

    expect(classifyParityScenarioState({ status: 'captured' }, paritySpec)).toBe('ready-for-comparison');

    const result = await executeParityScenario({
      repoRoot,
      scenario: {
        id: 'tags-add-live-parity',
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/tags-add-parity.json'],
      },
      paritySpec,
    });

    expect(result.ok).toBe(true);
    expect(result.primaryProxyStatus).toBe(200);
    expect(result.comparisons).toEqual([
      {
        name: 'mutation-data',
        ok: true,
        differences: [],
      },
      {
        name: 'downstream-read-data',
        ok: true,
        differences: [],
      },
    ]);
  });

  it('executes the promoted productVariantCreate compatibility scenario against the local proxy harness', async () => {
    const repoRoot = new URL('../..', import.meta.url).pathname;
    const paritySpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/productVariantCreate-parity-plan.json'), 'utf8'),
    ) as Parameters<typeof executeParityScenario>[0]['paritySpec'];

    expect(classifyParityScenarioState({ status: 'captured' }, paritySpec)).toBe('ready-for-comparison');

    const result = await executeParityScenario({
      repoRoot,
      scenario: {
        id: 'product-variant-create-compatibility-evidence',
        status: 'captured',
        captureFiles: [
          'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-variants-bulk-create-parity.json',
          'pending/product-variant-compatibility-live-schema-blocker.md',
        ],
      },
      paritySpec,
    });

    expect(result.ok).toBe(true);
    expect(result.primaryProxyStatus).toBe(200);
    expect(result.comparisons.map((comparison) => comparison.name)).toEqual([
      'variant-payload',
      'product-id',
      'product-totalInventory',
      'product-tracksInventory',
      'user-errors',
      'downstream-product-id',
      'downstream-product-totalInventory',
      'downstream-product-tracksInventory',
    ]);
  });

  it('executes the promoted metafieldsSet captured scenario against the local proxy harness', async () => {
    const repoRoot = new URL('../..', import.meta.url).pathname;
    const paritySpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/metafieldsSet-parity-plan.json'), 'utf8'),
    ) as Parameters<typeof executeParityScenario>[0]['paritySpec'];

    expect(classifyParityScenarioState({ status: 'captured' }, paritySpec)).toBe('ready-for-comparison');

    const result = await executeParityScenario({
      repoRoot,
      scenario: {
        id: 'metafields-set-live-parity',
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/metafields-set-parity.json'],
      },
      paritySpec,
    });

    expect(result.ok).toBe(true);
    expect(result.primaryProxyStatus).toBe(200);
    expect(result.comparisons.map((comparison) => comparison.name)).toEqual(['mutation-data', 'downstream-read-data']);
  });

  it('returns captured upstream payloads for no-write overlay reads', async () => {
    const repoRoot = new URL('../..', import.meta.url).pathname;
    const result = await executeParityScenario({
      repoRoot,
      scenario: {
        id: 'product-detail-read',
        status: 'captured',
        captureFiles: ['fixtures/conformance/very-big-test-store.myshopify.com/2025-01/product-detail.json'],
      },
      paritySpec: {
        proxyRequest: {
          documentPath: 'config/parity-requests/product-detail-read.graphql',
          variablesPath: 'config/parity-requests/product-detail-read.variables.json',
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

  it('marks blocked planned scenarios with a proxy request distinctly from generic planned work', () => {
    const state = classifyParityScenarioState(
      { status: 'planned' },
      {
        proxyRequest: {
          documentPath: 'config/parity-requests/productPublish.graphql',
          variablesPath: 'config/parity-requests/productPublish.json',
        },
        blocker: {
          kind: 'missing-live-scopes',
          blockerPath: 'pending/product-publication-conformance-scope-blocker.md',
        },
      },
    );

    expect(state).toBe('blocked-with-proxy-request');
  });

  it('keeps captured scenarios with blockers out of ready-for-comparison counts', () => {
    const state = classifyParityScenarioState(
      { status: 'captured' },
      {
        proxyRequest: {
          documentPath: 'config/parity-requests/productPublish.graphql',
          variablesPath: 'config/parity-requests/productPublish.json',
        },
        blocker: {
          kind: 'missing-publication-target',
          blockerPath: 'pending/product-publication-conformance-scope-blocker.md',
        },
      },
    );

    expect(state).toBe('blocked-with-proxy-request');
  });
});
