import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { parse as parseGraphql } from 'graphql';
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
const appsParitySpecPrefix = 'config/parity-specs/apps/';
const descriptorCassetteQueryPattern =
  /^\s*(?:hand-synthesized|sha:|cassette-backed|recorded by scripts|local-runtime)/iu;
const descriptorLikeUpstreamQuery = /(?:hand-synthesized|sha:|cassette-backed|recorded by scripts|local-runtime)/u;
const removedRuntimeTestExtension = String.fromCharCode(46, 103, 108, 101, 97, 109);

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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function collectUpstreamCallQueries(value: unknown, jsonPath = '$'): Array<{ path: string; query: unknown }> {
  if (Array.isArray(value)) {
    return value.flatMap((entry, index) => collectUpstreamCallQueries(entry, `${jsonPath}[${index}]`));
  }

  if (!isRecord(value)) {
    return [];
  }

  const ownQueries = Array.isArray(value['upstreamCalls'])
    ? value['upstreamCalls'].map((entry, index) => ({
        path: `${jsonPath}.upstreamCalls[${index}].query`,
        query: isRecord(entry) ? entry['query'] : undefined,
      }))
    : [];

  const nestedQueries = Object.entries(value).flatMap(([key, entry]) =>
    collectUpstreamCallQueries(entry, `${jsonPath}.${key}`),
  );

  return [...ownQueries, ...nestedQueries];
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

  it('keeps metafields captured proxy parity free of local-runtime and descriptor cassette evidence', () => {
    const descriptorPattern = /hand-synthesized|cassette-backed|recorded by scripts|sha:|local-runtime/u;
    const metafieldsCapturedProxySpecs = scenarios.filter((scenario) => {
      if (!scenario.paritySpecPath.startsWith('config/parity-specs/metafields/')) return false;
      if (scenario.status !== 'captured') return false;
      const paritySpec = readJson<ParitySpec>(scenario.paritySpecPath);
      return paritySpec.comparisonMode === 'captured-vs-proxy-request';
    });

    expect(metafieldsCapturedProxySpecs.length).toBeGreaterThan(0);

    for (const scenario of metafieldsCapturedProxySpecs) {
      const paritySpec = readJson<ParitySpec>(scenario.paritySpecPath);
      expect(paritySpec.assertionKinds ?? [], scenario.id).not.toContain('local-runtime-backed');
      for (const captureFile of paritySpec.liveCaptureFiles ?? []) {
        expect(captureFile, scenario.id).not.toContain('fixtures/conformance/local-runtime/');
        expect(captureFile, scenario.id).not.toMatch(descriptorPattern);

        const fixture = readJson<Record<string, unknown>>(captureFile);
        const upstreamCalls = Array.isArray(fixture['upstreamCalls']) ? fixture['upstreamCalls'] : [];
        expect(validateRecordedUpstreamCalls(upstreamCalls), captureFile).toEqual([]);
        for (const call of upstreamCalls) {
          if (typeof call === 'object' && call !== null && 'query' in call) {
            expect(String((call as { query?: unknown }).query), captureFile).not.toMatch(descriptorPattern);
          }
        }
      }
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
      for (const runtimeTestFile of paritySpec.runtimeTestFiles ?? []) {
        expect(runtimeTestFile.endsWith(removedRuntimeTestExtension), `${runtimeTestFile} should be current`).toBe(
          false,
        );
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

  it('keeps segment strict parity evidence backed by live Shopify fixtures and exact upstream calls', () => {
    for (const paritySpecPath of paritySpecPaths.filter((path) => path.startsWith('config/parity-specs/segments/'))) {
      const paritySpec = readJson<ParitySpec>(paritySpecPath);
      if (paritySpec.scenarioStatus !== 'captured' || paritySpec.comparisonMode !== 'captured-vs-proxy-request') {
        continue;
      }

      for (const captureFile of paritySpec.liveCaptureFiles ?? []) {
        expect(
          captureFile.startsWith('fixtures/conformance/local-runtime/'),
          `${paritySpec.scenarioId} must not use local-runtime fixtures as strict segment parity evidence`,
        ).toBe(false);

        const fixture = readJson<{ upstreamCalls?: unknown[] }>(captureFile);
        if (Array.isArray(fixture.upstreamCalls)) {
          expect(
            validateRecordedUpstreamCalls(fixture.upstreamCalls as RecordedUpstreamCall[]),
            `${captureFile} upstream calls`,
          ).toEqual([]);
        }
      }
    }
  });

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

  it('keeps marketing captured parity evidence backed by live Shopify fixture paths', () => {
    const offenders = paritySpecPaths.flatMap((paritySpecPath) => {
      if (!paritySpecPath.startsWith('config/parity-specs/marketing/')) {
        return [];
      }

      const paritySpec = readJson<ParitySpec>(paritySpecPath);
      if (paritySpec.scenarioStatus !== 'captured') {
        return [];
      }

      const capturePathOffenders = (paritySpec.liveCaptureFiles ?? [])
        .filter((captureFile) => captureFile.startsWith('fixtures/conformance/local-runtime/'))
        .map((captureFile) => `${paritySpec.scenarioId}: local-runtime liveCaptureFiles entry ${captureFile}`);
      const assertionKindOffenders = (paritySpec.assertionKinds ?? [])
        .filter((assertionKind) => assertionKind === 'local-runtime-backed')
        .map((assertionKind) => `${paritySpec.scenarioId}: captured spec keeps ${assertionKind}`);
      const descriptorOffenders = (paritySpec.liveCaptureFiles ?? []).flatMap((captureFile) => {
        const absolutePath = resolve(repoRoot, captureFile);
        if (!existsSync(absolutePath) || !captureFile.endsWith('.json')) {
          return [];
        }

        const fixture = readJson<{ upstreamCalls?: RecordedUpstreamCall[] }>(captureFile);
        return (fixture.upstreamCalls ?? []).flatMap((call, index) =>
          typeof call.query === 'string' && descriptorLikeUpstreamQuery.test(call.query)
            ? [`${paritySpec.scenarioId}: ${captureFile} upstreamCalls[${index}].query is descriptor-like`]
            : [],
        );
      });

      return [...capturePathOffenders, ...assertionKindOffenders, ...descriptorOffenders];
    });

    expect(offenders).toEqual([]);
  });

  it('keeps apps captured parity evidence tied to live Shopify captures', () => {
    const appScenarios = scenarios.filter((scenario) => scenario.paritySpecPath.startsWith(appsParitySpecPrefix));

    for (const scenario of appScenarios) {
      const paritySpec = readJson<ParitySpec>(scenario.paritySpecPath);
      if (paritySpec.scenarioStatus !== 'captured') {
        continue;
      }

      for (const captureFile of paritySpec.liveCaptureFiles ?? []) {
        expect(
          captureFile.includes('fixtures/conformance/local-runtime/'),
          `${scenario.id} must not use local-runtime fixtures as captured apps parity evidence`,
        ).toBe(false);

        const fixture = readJson<Record<string, unknown>>(captureFile);
        const upstreamCalls = Array.isArray(fixture['upstreamCalls']) ? fixture['upstreamCalls'] : [];

        for (const [index, upstreamCall] of upstreamCalls.entries()) {
          const query =
            typeof (upstreamCall as { query?: unknown })['query'] === 'string'
              ? (upstreamCall as { query: string })['query']
              : null;
          if (!query) {
            continue;
          }

          expect(
            query,
            `${scenario.id} upstreamCalls[${index}].query must be the exact GraphQL document, not a provenance descriptor`,
          ).not.toMatch(descriptorCassetteQueryPattern);
        }
      }
    }
  });

  it.each(
    scenarios.flatMap((scenario) =>
      scenario.operationNames.map((operationName) => [`${scenario.id} -> ${operationName}`, operationName] as const),
    ),
  )('keeps discovered scenario operation reachable from the operation registry: %s', (_label, operationName) => {
    expect(operationRegistry.some((entry) => entry.name === operationName)).toBe(true);
  });

  it('keeps media parity evidence on live captures with exact GraphQL cassette queries', () => {
    const mediaParitySpecs = paritySpecPaths.filter((paritySpecPath) =>
      paritySpecPath.startsWith('config/parity-specs/media/'),
    );
    const mediaCaptureFiles = new Set<string>();

    for (const paritySpecPath of mediaParitySpecs) {
      const paritySpec = readJson<ParitySpec>(paritySpecPath);
      for (const captureFile of paritySpec.liveCaptureFiles ?? []) {
        expect(
          captureFile.startsWith('fixtures/conformance/local-runtime/'),
          `${paritySpec.scenarioId ?? paritySpecPath} must not use local-runtime media parity evidence`,
        ).toBe(false);
        mediaCaptureFiles.add(captureFile);
      }
    }

    expect(
      listJsonFiles('fixtures/conformance/local-runtime').filter((fixturePath) => fixturePath.includes('/media/')),
      'media local-runtime fixtures must be replaced by live captures or Rust integration tests',
    ).toEqual([]);

    for (const captureFile of mediaCaptureFiles) {
      if (!captureFile.includes('/media/')) {
        continue;
      }
      const capture = readJson<{ upstreamCalls?: RecordedUpstreamCall[] }>(captureFile);
      expect(validateRecordedUpstreamCalls(capture.upstreamCalls ?? []), captureFile).toEqual([]);
    }
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

  it('keeps online-store captured parity evidence backed by live Shopify fixture paths', () => {
    const errors: string[] = [];
    const onlineStoreScenarios = scenarios.filter((scenario) =>
      scenario.paritySpecPath.startsWith('config/parity-specs/online-store/'),
    );

    for (const scenario of onlineStoreScenarios) {
      const paritySpec = readJson<ParitySpec>(scenario.paritySpecPath);
      if (paritySpec.scenarioStatus !== 'captured') {
        continue;
      }

      for (const captureFile of paritySpec.liveCaptureFiles ?? []) {
        if (captureFile.startsWith('fixtures/conformance/local-runtime/')) {
          errors.push(`${scenario.id}: liveCaptureFiles must not point at local-runtime evidence: ${captureFile}`);
          continue;
        }

        const fixture = readJson<{ upstreamCalls?: RecordedUpstreamCall[] }>(captureFile);
        const upstreamCalls = Array.isArray(fixture.upstreamCalls) ? fixture.upstreamCalls : [];
        for (const error of validateRecordedUpstreamCalls(upstreamCalls)) {
          errors.push(`${scenario.id} ${captureFile}: ${error}`);
        }
        for (const [index, call] of upstreamCalls.entries()) {
          if (typeof call.query === 'string' && descriptorLikeUpstreamQuery.test(call.query)) {
            errors.push(`${scenario.id}: ${captureFile} upstreamCalls[${index}].query is descriptor-like`);
          }
        }
      }
    }

    expect(errors).toEqual([]);
  });

  it('keeps functions parity evidence free of synthetic provenance markers', () => {
    const syntheticEvidencePattern = /hand-synthesized|cassette-backed|local-runtime|sha:|recorded by scripts\//u;
    const failures: string[] = [];

    for (const specPath of paritySpecPaths.filter((candidate) =>
      candidate.startsWith('config/parity-specs/functions/'),
    )) {
      const specText = readFileSync(resolve(repoRoot, specPath), 'utf8');
      if (syntheticEvidencePattern.test(specText)) {
        failures.push(`${specPath} contains a synthetic provenance marker`);
      }

      const paritySpec = readJson<ParitySpec>(specPath);
      for (const captureFile of paritySpec.liveCaptureFiles ?? []) {
        if (captureFile.includes('/local-runtime/')) {
          failures.push(`${specPath} references local-runtime capture ${captureFile}`);
          continue;
        }

        const fixtureText = readFileSync(resolve(repoRoot, captureFile), 'utf8');
        if (syntheticEvidencePattern.test(fixtureText)) {
          failures.push(`${captureFile} contains a synthetic provenance marker`);
        }

        const fixture = JSON.parse(fixtureText) as { upstreamCalls?: RecordedUpstreamCall[] };
        for (const error of validateRecordedUpstreamCalls(fixture.upstreamCalls ?? [])) {
          failures.push(`${captureFile}: ${error}`);
        }
      }
    }

    expect(failures).toEqual([]);
  });

  it('builds conformance status from discovered parity specs', () => {
    const status = buildConformanceStatusDocument(repoRoot);
    const implementedStorefrontRoots = [
      'article',
      'articles',
      'blog',
      'blogByHandle',
      'blogs',
      'cart',
      'cartAttributesUpdate',
      'cartCreate',
      'cartLinesAdd',
      'cartLinesRemove',
      'cartLinesUpdate',
      'cartNoteUpdate',
      'customer',
      'customerAccessTokenCreate',
      'customerAccessTokenCreateWithMultipass',
      'customerAccessTokenDelete',
      'customerAccessTokenRenew',
      'customerActivate',
      'customerActivateByUrl',
      'customerCreate',
      'customerRecover',
      'customerReset',
      'customerResetByUrl',
      'localization',
      'locations',
      'menu',
      'metaobject',
      'metaobjects',
      'page',
      'pageByHandle',
      'pages',
      'paymentSettings',
      'product',
      'productByHandle',
      'products',
      'publicApiVersions',
      'shop',
      'sitemap',
      'urlRedirects',
    ];

    expect(status.implementedOperations.length).toBeGreaterThan(0);
    expect(status.apiSurfaceSummaries.admin.implementedOperations).toBeGreaterThan(0);
    expect(status.apiSurfaceSummaries.storefront.implementedOperations).toBeGreaterThanOrEqual(
      implementedStorefrontRoots.length,
    );
    expect(status.apiSurfaceSummaries.storefront.coveredOperationNames).toEqual(
      expect.arrayContaining(implementedStorefrontRoots),
    );
    expect(status.apiSurfaceSummaries.aggregate.implementedOperations).toBe(status.implementedOperations.length);
    expect(status.capturedScenarioIds).toContain('product-create-live-parity');
    expect(status.capturedScenarioIds).toContain('product-duplicate-live-parity');
    expect(status.strictComparisonScenarioIds).toContain('product-create-live-parity');
    expect(status.strictComparisonScenarioIds).toContain('customer-address-lifecycle-parity');
    expect(status.captureOnlyScenarioIds).toHaveLength(0);
    expect(status.captureOnlyScenarioIds).not.toContain('product-create-live-parity');
    expect(status.implementedOperations.every((entry) => entry.scenarioIds.length > 0)).toBe(true);
  });

  it('keeps Admin and Storefront operation coverage identities separate', () => {
    const status = buildConformanceStatusDocument(repoRoot);
    const adminShop = status.implementedOperations.find(
      (entry) => entry.apiSurface === 'admin' && entry.name === 'shop',
    );
    const storefrontShopScenario = scenarios.find((scenario) => scenario.id === 'storefront-shop-name-proxy-parity');
    const storefrontFirstSliceScenario = scenarios.find((scenario) => scenario.id === 'storefront-first-slice-default');

    expect(storefrontShopScenario?.apiSurface).toBe('storefront');
    expect(adminShop?.scenarioIds).not.toContain('storefront-shop-name-proxy-parity');
    expect(storefrontFirstSliceScenario?.apiSurface).toBe('storefront');
    expect(adminShop?.scenarioIds).not.toContain('storefront-first-slice-default');
    expect(status.apiSurfaceSummaries.storefront.coveredOperationNames).toContain('shop');
  });

  it('blocks synthetic metaobjects parity evidence from captured scenarios', () => {
    const failures: string[] = [];
    const metaobjectScenarios = scenarios.filter((scenario) =>
      scenario.paritySpecPath.startsWith('config/parity-specs/metaobjects/'),
    );

    for (const scenario of metaobjectScenarios) {
      const paritySpec = readJson<ParitySpec>(scenario.paritySpecPath);
      if (paritySpec.scenarioStatus !== 'captured') {
        continue;
      }

      for (const captureFile of paritySpec.liveCaptureFiles ?? []) {
        if (captureFile.includes('/local-runtime/')) {
          failures.push(`${scenario.id}: ${captureFile} must not be used as captured metaobjects parity evidence`);
          continue;
        }

        const fixture = readJson<unknown>(captureFile);
        for (const { path, query } of collectUpstreamCallQueries(fixture)) {
          if (typeof query !== 'string') {
            failures.push(`${scenario.id}: ${captureFile} ${path} must be an exact GraphQL document string`);
            continue;
          }

          try {
            parseGraphql(query);
          } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            failures.push(`${scenario.id}: ${captureFile} ${path} is not parseable GraphQL: ${message}`);
          }
        }
      }
    }

    expect(failures).toEqual([]);
  });
});
