import { describe, expect, it } from 'vitest';

import {
  classifyParityScenarioState,
  compareJsonPayloads,
  summarizeParityResults,
  validateComparisonContract,
} from '../../scripts/conformance-parity-lib.mjs';

describe('classifyParityScenarioState', () => {
  it('does not mark captured scenarios ready until an explicit strict comparison contract exists', () => {
    const state = classifyParityScenarioState(
      { status: 'captured' },
      {
        proxyRequest: {
          documentPath: 'config/parity-requests/productCreate.graphql',
          variablesPath: 'config/parity-requests/productCreate.json',
        },
      },
    );

    expect(state).toBe('captured-awaiting-comparison-contract');
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
          allowedDifferences: [],
        },
      },
    );

    expect(state).toBe('ready-for-comparison');
  });

  it('does not mark captured scenarios ready when the comparison contract has undocumented allowances', () => {
    const state = classifyParityScenarioState(
      { status: 'captured' },
      {
        proxyRequest: {
          documentPath: 'config/parity-requests/productCreate.graphql',
        },
        comparison: {
          mode: 'strict-json',
          allowedDifferences: [
            {
              path: '$.data.productCreate.product.id',
              matcher: 'shopify-gid:Product',
            },
          ],
        },
      },
    );

    expect(state).toBe('captured-awaiting-comparison-contract');
  });

  it('marks planned scenarios with a proxy request as planned-with-proxy-request', () => {
    const state = classifyParityScenarioState(
      { status: 'planned' },
      {
        proxyRequest: {
          documentPath: 'config/parity-requests/productCreate.graphql',
          variablesPath: 'config/parity-requests/productCreate.json',
        },
      },
    );

    expect(state).toBe('planned-with-proxy-request');
  });
});

describe('summarizeParityResults', () => {
  it('separates strict comparison readiness from captured scenarios awaiting contracts', () => {
    const summary = summarizeParityResults([
      { state: 'ready-for-comparison' },
      { state: 'captured-awaiting-comparison-contract' },
      { state: 'captured-awaiting-comparison-contract' },
      { state: 'captured-awaiting-proxy-request' },
      { state: 'planned-with-proxy-request' },
      { state: 'planned' },
    ]);

    expect(summary.readyForComparison).toBe(1);
    expect(summary.pending).toBe(5);
    expect(summary.statusCounts).toEqual({
      readyForComparison: 1,
      capturedAwaitingComparisonContract: 2,
      capturedAwaitingProxyRequest: 1,
      plannedWithProxyRequest: 1,
      planned: 1,
    });
    expect(summary.statusNote).toContain('not parity failures');
  });
});

describe('validateComparisonContract', () => {
  it('requires allowed differences to be path-scoped, documented, and typed', () => {
    expect(
      validateComparisonContract({
        mode: 'strict-json',
        allowedDifferences: [
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
        allowedDifferences: [
          {
            path: '$.data.product.id',
            matcher: 'everything',
          },
          {
            reason: 'This rule is missing a path and action.',
          },
        ],
      }),
    ).toEqual([
      'allowedDifferences[0] must document why the difference is nondeterministic.',
      'allowedDifferences[0] declares unknown matcher `everything`.',
      'allowedDifferences[1] must declare a non-empty JSON path.',
      'allowedDifferences[1] must declare exactly one of `matcher` or `ignore: true`.',
    ]);
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
        allowedDifferences: [
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
          allowedDifferences: [{ path: '$.data.productCreate.product.id', matcher: 'shopify-gid:Product' }],
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
        allowedDifferences: [
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
