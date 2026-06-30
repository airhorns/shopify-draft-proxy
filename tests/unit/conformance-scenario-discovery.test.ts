import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { validateComparisonContract, type ParitySpec } from '../../scripts/conformance-parity-spec.js';
import { validateRecordedUpstreamCalls, type RecordedUpstreamCall } from '../../scripts/parity-cassette.js';
import {
  buildConformanceStatusDocument,
  listConformanceParitySpecPaths,
  loadOperationRegistry,
  loadConformanceScenarioOverrides,
  loadConformanceScenarios,
} from '../../scripts/conformance-scenario-registry.js';

const repoRoot = resolve(import.meta.dirname, '../..');
const allowedScenarioStatuses = new Set(['captured', 'planned']);

function readJson<T>(relativePath: string): T {
  return JSON.parse(readFileSync(resolve(repoRoot, relativePath), 'utf8')) as T;
}

describe('conformance scenario discovery', () => {
  const paritySpecPaths = listConformanceParitySpecPaths(repoRoot);
  const scenarioOverrides = loadConformanceScenarioOverrides(repoRoot);
  const scenarios = loadConformanceScenarios(repoRoot);
  const operationRegistry = loadOperationRegistry(repoRoot);

  it('uses parity specs as the scenario convention instead of generated or central manifests', () => {
    expect(existsSync(resolve(repoRoot, 'config/conformance-scenarios.json'))).toBe(false);
    expect(existsSync(resolve(repoRoot, 'docs/generated'))).toBe(false);

    expect(paritySpecPaths.length).toBeGreaterThan(0);
    expect(scenarios.map((scenario) => scenario.paritySpecPath)).toEqual(paritySpecPaths);
  });

  it('keeps discovered scenario ids unique and structurally complete', () => {
    const scenarioIds = scenarios.map((scenario) => scenario.id);
    expect(new Set(scenarioIds).size).toBe(scenarioIds.length);

    for (const scenario of scenarios) {
      expect(scenario.id.length, `${scenario.paritySpecPath} should declare scenarioId`).toBeGreaterThan(0);
      expect(scenario.operationNames.length, `${scenario.id} should declare operationNames`).toBeGreaterThan(0);
      expect(allowedScenarioStatuses.has(scenario.status), `${scenario.id} has invalid status`).toBe(true);
      expect(scenario.assertionKinds.length, `${scenario.id} should declare assertionKinds`).toBeGreaterThan(0);
      if (scenario.status === 'captured') {
        expect(scenario.captureFiles.length, `${scenario.id} should reference capture files`).toBeGreaterThan(0);
      }
    }

    for (const scenarioId of scenarioOverrides.keys()) {
      expect(scenarioIds).toContain(scenarioId);
    }
  });

  it.each(scenarios.map((scenario) => [scenario.id, scenario] as const))(
    'keeps parity spec file references present on disk for %s',
    (_scenarioId, scenario) => {
      const paritySpec = readJson<ParitySpec>(scenario.paritySpecPath);
      expect(paritySpec.scenarioId).toBe(scenario.id);
      expect(paritySpec.operationNames).toEqual(scenario.operationNames);
      expect(paritySpec.scenarioStatus).toBe(scenario.status);
      expect(paritySpec.assertionKinds).toEqual(scenario.assertionKinds);
      expect(paritySpec.liveCaptureFiles).toEqual(scenario.captureFiles);
      expect(paritySpec.runtimeTestFiles ?? []).toEqual(scenario.runtimeTestFiles);

      for (const captureFile of paritySpec.liveCaptureFiles ?? []) {
        expect(existsSync(resolve(repoRoot, captureFile)), `${captureFile} should exist`).toBe(true);
      }

      if (paritySpec.proxyRequest?.documentPath) {
        expect(existsSync(resolve(repoRoot, paritySpec.proxyRequest.documentPath))).toBe(true);
      }
      if (paritySpec.proxyRequest?.variablesPath) {
        expect(existsSync(resolve(repoRoot, paritySpec.proxyRequest.variablesPath))).toBe(true);
      }

      if (scenario.status === 'captured' && paritySpec.comparisonMode === 'captured-fixture') {
        expect(paritySpec.runtimeTestFiles?.length ?? 0, `${scenario.id} runtime test files`).toBeGreaterThan(0);
      } else if (scenario.status === 'captured') {
        expect(validateComparisonContract(paritySpec.comparison), `${scenario.id} comparison contract`).toEqual([]);
      } else if (paritySpec.comparison) {
        expect(validateComparisonContract(paritySpec.comparison)).not.toEqual([]);
      }
    },
  );

  it('keeps discounts parity evidence free of local-runtime captures and descriptor upstream cassettes', () => {
    const errors: string[] = [];
    const discountScenarios = scenarios.filter((scenario) =>
      scenario.paritySpecPath.startsWith('config/parity-specs/discounts/'),
    );

    for (const scenario of discountScenarios) {
      const paritySpec = readJson<ParitySpec>(scenario.paritySpecPath);
      for (const captureFile of paritySpec.liveCaptureFiles ?? []) {
        if (captureFile.includes('/local-runtime/')) {
          errors.push(`${scenario.id}: liveCaptureFiles must not point at local-runtime evidence: ${captureFile}`);
          continue;
        }

        const capture = readJson<{ upstreamCalls?: RecordedUpstreamCall[] }>(captureFile);
        const upstreamCalls = Array.isArray(capture.upstreamCalls) ? capture.upstreamCalls : [];
        for (const error of validateRecordedUpstreamCalls(upstreamCalls)) {
          errors.push(`${scenario.id} ${captureFile}: ${error}`);
        }
      }
    }

    expect(errors).toEqual([]);
  });

  it.each(
    scenarios.flatMap((scenario) =>
      scenario.operationNames.map((operationName) => [`${scenario.id} -> ${operationName}`, operationName] as const),
    ),
  )('keeps discovered scenario operation reachable from the operation registry: %s', (_label, operationName) => {
    expect(operationRegistry.some((entry) => entry.name === operationName)).toBe(true);
  });

  it('keeps every runtime-tested operation covered by at least one discovered scenario', () => {
    const statusDocument = buildConformanceStatusDocument(repoRoot);
    const coveredOperationNames = new Set(statusDocument.coveredOperationNames);

    // `implemented` now spans the full locally-handled surface; conformance coverage is owed only
    // by operations that declare runtime tests (the uniform table-dispatch set).
    for (const entry of operationRegistry.filter((candidate) => (candidate.runtimeTests?.length ?? 0) > 0)) {
      expect(coveredOperationNames.has(entry.name), `${entry.name} should have scenario or runtime-test coverage`).toBe(
        true,
      );
    }
  });

  it('builds conformance status from discovered parity specs', () => {
    const status = buildConformanceStatusDocument(repoRoot);

    expect(status.implementedOperations.length).toBeGreaterThan(0);
    expect(status.capturedScenarioIds).toContain('product-create-live-parity');
    expect(status.capturedScenarioIds).toContain('product-duplicate-live-parity');
    expect(status.strictComparisonScenarioIds).toContain('product-create-live-parity');
    expect(status.strictComparisonScenarioIds).toContain('customer-address-lifecycle-parity');
    expect(status.captureOnlyScenarioIds).toHaveLength(0);
    expect(status.captureOnlyScenarioIds).not.toContain('product-create-live-parity');
    expect(status.implementedOperations.every((entry) => entry.scenarioIds.length > 0)).toBe(true);
  });
});
