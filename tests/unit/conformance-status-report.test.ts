import { describe, expect, it } from 'vitest';

// @ts-ignore scripts/ is intentionally outside tsconfig's checked sources; runtime coverage here verifies the JS helper.
import * as conformanceStatusReport from '../../scripts/conformance-status-report.mjs';

const { buildConformanceReport, compareConformanceSummaries, renderConformanceComment, summarizeConformanceStatus } =
  conformanceStatusReport as {
    buildConformanceReport: (input: {
      status: typeof status;
      baseline?: typeof status | null;
      commit?: string | null;
      refName?: string | null;
      runId?: string | null;
    }) => {
      conformance: ReturnType<typeof summarizeConformanceStatus>;
      baseline: ReturnType<typeof summarizeConformanceStatus> | null;
      delta: ReturnType<typeof compareConformanceSummaries>;
      commit: string | null;
    };
    compareConformanceSummaries: (
      current: ReturnType<typeof summarizeConformanceStatus>,
      baseline: ReturnType<typeof summarizeConformanceStatus> | null,
    ) => {
      conformingScenarios: number;
      totalScenarios: number;
      conformanceRatio: number;
      coveredOperations: number;
      implementedOperations: number;
      declaredGapOperations: number;
    } | null;
    renderConformanceComment: (report: ReturnType<typeof buildConformanceReport>) => string;
    summarizeConformanceStatus: (input: typeof status) => {
      generatedAt: string | null;
      conformingScenarios: number;
      totalScenarios: number;
      pendingScenarios: number;
      conformanceRatio: number;
      coveredOperations: number;
      implementedOperations: number;
      declaredGapOperations: number;
      operationCoverageRatio: number;
    };
  };

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
