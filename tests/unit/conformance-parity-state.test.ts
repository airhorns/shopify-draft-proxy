import { describe, expect, it } from 'vitest';

// scripts/ is intentionally outside tsconfig's checked sources; runtime coverage here verifies the JS helper.
// @ts-expect-error local .mjs helper is exercised via Vitest rather than TS declarations
import { classifyParityScenarioState } from '../../scripts/conformance-parity-lib.mjs';

describe('classifyParityScenarioState', () => {
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
