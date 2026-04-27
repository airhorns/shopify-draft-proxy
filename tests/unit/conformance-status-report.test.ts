import { describe, expect, it } from 'vitest';

import {
  buildConformanceReport,
  compareConformanceSummaries,
  renderConformanceComment,
  summarizeConformanceStatus,
  type ConformanceStatusDocument,
} from '../../scripts/conformance-status-report.js';

const status = {
  generatedAt: '2026-04-19T00:00:00.000Z',
  implementedOperations: [{ name: 'product' }, { name: 'productCreate' }, { name: 'productUpdate' }],
  coveredOperationNames: ['product', 'productCreate'],
  declaredGapOperationNames: ['productUpdate'],
  capturedScenarioIds: ['product-detail-read', 'product-create-live-parity'],
  strictComparisonScenarioIds: ['product-detail-read'],
  captureOnlyScenarioIds: ['product-create-live-parity'],
  plannedScenarioIds: ['productUpdate-parity-plan'],
  regrettableDivergences: [
    {
      scenarioId: 'product-create-live-parity',
      paritySpecPath: 'config/parity-specs/productCreate-parity-plan.json',
      expectedDifferenceIndex: 0,
      path: '$.data.productCreate.product.id',
      reason: 'Shopify and the proxy allocate different product ids.',
      matcher: 'shopify-gid:Product',
      ignored: false,
    },
  ],
} satisfies ConformanceStatusDocument;

describe('conformance status reporting', () => {
  it('summarizes strict comparison scenarios separately from capture-only scenarios', () => {
    expect(summarizeConformanceStatus(status)).toMatchObject({
      conformingScenarios: 1,
      totalScenarios: 3,
      captureOnlyScenarios: 1,
      pendingScenarios: 1,
      coveredOperations: 2,
      implementedOperations: 3,
      declaredGapOperations: 1,
      regrettableDivergences: 1,
      regrettableDivergenceScenarios: 1,
    });
  });

  it('computes improvement against the main baseline', () => {
    const current = summarizeConformanceStatus(status);
    const baseline = {
      ...current,
      conformingScenarios: 1,
      totalScenarios: 3,
      conformanceRatio: 1 / 3,
      captureOnlyScenarios: 0,
      coveredOperations: 1,
      declaredGapOperations: 2,
      regrettableDivergences: 0,
      regrettableDivergenceScenarios: 0,
    };

    expect(compareConformanceSummaries(current, baseline)).toMatchObject({
      conformingScenarios: 0,
      totalScenarios: 0,
      captureOnlyScenarios: 1,
      captureOnlyScenariosKnown: true,
      coveredOperations: 1,
      implementedOperations: 0,
      declaredGapOperations: -1,
      regrettableDivergences: 1,
      regrettableDivergenceScenarios: 1,
    });
  });

  it('renders a stable marker-delimited PR comment', () => {
    const report = buildConformanceReport({
      status,
      baseline: {
        ...status,
        capturedScenarioIds: ['product-detail-read'],
        strictComparisonScenarioIds: ['product-detail-read'],
        captureOnlyScenarioIds: [],
        plannedScenarioIds: ['product-create-live-parity', 'productUpdate-parity-plan'],
        coveredOperationNames: ['product'],
        declaredGapOperationNames: ['productCreate', 'productUpdate'],
      },
      commit: '1234567890abcdef',
      refName: 'example',
      runId: '42',
    });

    expect(renderConformanceComment(report)).toContain('<!-- shopify-draft-proxy-conformance-status -->');
    expect(renderConformanceComment(report)).toContain('Current branch: 1 / 3 scenarios prove strict proxy parity');
    expect(renderConformanceComment(report)).toContain('Improvement over main: 0 strict parity scenarios');
    expect(renderConformanceComment(report)).toContain(
      'Capture-only scenarios: 1 not counted as strict parity (main: 0, delta: +1)',
    );
    expect(renderConformanceComment(report)).toContain('ALARM: capture-only parity specs increased by +1 vs main.');
    expect(renderConformanceComment(report)).toContain('`product-create-live-parity`');
    expect(renderConformanceComment(report)).toContain(
      'Regrettable divergences: 1 expected differences across 1 scenarios',
    );
  });

  it('does not alarm when the baseline predates capture-only breakdowns', () => {
    const report = buildConformanceReport({
      status,
      baseline: {
        conformingScenarios: 1,
        totalScenarios: 3,
        pendingScenarios: 2,
        conformanceRatio: 1 / 3,
        coveredOperations: 1,
        implementedOperations: 3,
        declaredGapOperations: 2,
        operationCoverageRatio: 1 / 3,
        regrettableDivergences: 0,
        regrettableDivergenceScenarios: 0,
        generatedAt: '2026-04-18T00:00:00.000Z',
      },
    });

    const comment = renderConformanceComment(report);

    expect(comment).toContain('Capture-only scenarios: 1 not counted as strict parity');
    expect(comment).toContain('main baseline predates this breakdown');
    expect(comment).not.toContain('ALARM:');
  });
});
