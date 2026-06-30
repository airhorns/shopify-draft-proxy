import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';
import { parse as parseGraphql } from 'graphql';

import { validateComparisonContract, type ParitySpec } from '../../scripts/conformance-parity-spec.js';
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

function listJsonFiles(relativeDirectory: string): string[] {
  const absoluteDirectory = resolve(repoRoot, relativeDirectory);
  if (!existsSync(absoluteDirectory)) {
    return [];
  }

  return readdirSync(absoluteDirectory, { withFileTypes: true }).flatMap((entry) => {
    const relativePath = `${relativeDirectory}/${entry.name}`;
    if (entry.isDirectory()) {
      return listJsonFiles(relativePath);
    }

    return entry.isFile() && entry.name.endsWith('.json') ? [relativePath] : [];
  });
}

function getRecordProperty(value: unknown, key: string): unknown {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)[key]
    : undefined;
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

  it('keeps orders parity evidence out of local-runtime and descriptor cassettes', () => {
    const specsWithLocalRuntimeOrdersCapture = listJsonFiles('config/parity-specs')
      .map((paritySpecPath) => {
        const spec = readJson<ParitySpec>(paritySpecPath);
        const localRuntimeCaptureFiles = (spec.liveCaptureFiles ?? []).filter(
          (captureFile) =>
            captureFile.startsWith('fixtures/conformance/local-runtime/') && captureFile.includes('/orders/'),
        );
        return localRuntimeCaptureFiles.length > 0 ? { paritySpecPath, localRuntimeCaptureFiles } : null;
      })
      .filter((entry): entry is { paritySpecPath: string; localRuntimeCaptureFiles: string[] } => entry !== null);

    expect(specsWithLocalRuntimeOrdersCapture).toEqual([]);

    const descriptorPattern = /^(?:hand-synthesized|sha:|cassette-backed|recorded by scripts\/)/u;
    const badOrderCassetteQueries = listJsonFiles('fixtures/conformance')
      .filter((fixturePath) => fixturePath.includes('/orders/'))
      .flatMap((fixturePath) => {
        const fixture = readJson<Record<string, unknown>>(fixturePath);
        const upstreamCalls = getRecordProperty(fixture, 'upstreamCalls');
        if (!Array.isArray(upstreamCalls)) {
          return [];
        }

        return upstreamCalls.flatMap((call, index) => {
          const query = getRecordProperty(call, 'query');
          if (typeof query !== 'string' || query.trim().length === 0) {
            return [`${fixturePath}: upstreamCalls[${index}].query is empty or missing`];
          }
          if (descriptorPattern.test(query)) {
            return [`${fixturePath}: upstreamCalls[${index}].query is a descriptor: ${query}`];
          }
          try {
            parseGraphql(query);
          } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            return [`${fixturePath}: upstreamCalls[${index}].query is not valid GraphQL: ${message}`];
          }
          return [];
        });
      });

    expect(badOrderCassetteQueries).toEqual([]);
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
