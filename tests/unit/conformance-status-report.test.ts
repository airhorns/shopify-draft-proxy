import { describe, expect, it } from 'vitest';

// scripts/ is intentionally outside tsconfig's checked sources; runtime coverage here verifies the JS helper.
// @ts-expect-error local .mjs helper is exercised via Vitest rather than TS declarations
import { buildConformanceReport, compareConformanceSummaries, renderConformanceComment, summarizeConformanceStatus } from '../../scripts/conformance-status-report.mjs';

const status = {
  generatedAt: '2026-04-19T00:00:00.000Z',
  implementedOperations: [{ name: 'product' }, { name: 'productCreate' }, { name: 'productUpdate' }],
  coveredOperationNames: ['product', 'productCreate'],
  declaredGapOperationNames: ['productUpdate'],
  capturedScenarioIds: ['product-detail-read', 'product-create-live-parity'],
  plannedScenarioIds: ['productUpdate-parity-plan'],
};

describe('conformance status reporting', () => {
  it('summarizes captured scenarios as conforming scenarios', () => {
    expect(summarizeConformanceStatus(status)).toMatchObject({
      conformingScenarios: 2,
      totalScenarios: 3,
      pendingScenarios: 1,
      coveredOperations: 2,
      implementedOperations: 3,
      declaredGapOperations: 1,
    });
  });

  it('computes improvement against the main baseline', () => {
    const current = summarizeConformanceStatus(status);
    const baseline = {
      ...current,
      conformingScenarios: 1,
      totalScenarios: 3,
      conformanceRatio: 1 / 3,
      coveredOperations: 1,
      declaredGapOperations: 2,
    };

    expect(compareConformanceSummaries(current, baseline)).toMatchObject({
      conformingScenarios: 1,
      totalScenarios: 0,
      coveredOperations: 1,
      implementedOperations: 0,
      declaredGapOperations: -1,
    });
  });

  it('renders a stable marker-delimited PR comment', () => {
    const report = buildConformanceReport({
      status,
      baseline: {
        ...status,
        capturedScenarioIds: ['product-detail-read'],
        plannedScenarioIds: ['product-create-live-parity', 'productUpdate-parity-plan'],
        coveredOperationNames: ['product'],
        declaredGapOperationNames: ['productCreate', 'productUpdate'],
      },
      commit: '1234567890abcdef',
      refName: 'example',
      runId: '42',
    });

    expect(renderConformanceComment(report)).toContain('<!-- shopify-draft-proxy-conformance-status -->');
    expect(renderConformanceComment(report)).toContain('Current branch: 2 / 3 scenarios conforming');
    expect(renderConformanceComment(report)).toContain('Improvement over main: +1 conforming scenarios');
  });
});
