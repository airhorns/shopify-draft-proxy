import { describe, expect, it } from 'vitest';

// scripts/ is intentionally outside tsconfig's checked sources; runtime coverage here verifies the JS helper.
// @ts-expect-error local .mjs helper is exercised via Vitest rather than TS declarations
import { classifyParityScenarioState, compareJson } from '../../scripts/conformance-parity-lib.mjs';

describe('classifyParityScenarioState', () => {
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

  it('requires captured scenarios to declare executable comparison contracts', () => {
    expect(
      classifyParityScenarioState(
        { status: 'captured' },
        {
          proxyRequest: {
            documentPath: 'config/parity-requests/productCreate.graphql',
          },
        },
      ),
    ).toBe('invalid-missing-comparison-contract');

    expect(
      classifyParityScenarioState(
        { status: 'captured' },
        {
          comparisonMode: 'captured-vs-proxy-request',
          comparisons: [{ name: 'mutation payload' }],
        },
      ),
    ).toBe('ready-for-comparison');
  });
});

describe('compareJson', () => {
  it('fails strict payload differences unless they are path-scoped as allowed', () => {
    expect(compareJson({ data: { product: { id: 'gid://shopify/Product/1', title: 'Hat' } } }, { data: { product: { id: 'gid://shopify/Product/2', title: 'Hat' } } }).pass).toBe(false);

    expect(
      compareJson(
        { data: { product: { id: 'gid://shopify/Product/1', title: 'Hat' } } },
        { data: { product: { id: 'gid://shopify/Product/2', title: 'Hat' } } },
        { allowedDifferencePaths: ['$.data.product.id'] },
      ).pass,
    ).toBe(true);
  });
});
