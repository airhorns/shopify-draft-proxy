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
});
