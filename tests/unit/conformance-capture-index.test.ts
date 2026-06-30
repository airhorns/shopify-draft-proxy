import { describe, expect, it } from 'vitest';

import {
  conformanceCaptureIndex,
  loadConformanceCaptureScriptPaths,
  profileConformanceFixtureProvenance,
  renderCaptureIndexMarkdown,
  validateCaptureIndexAgainstScriptFiles,
} from '../../scripts/conformance-capture-index.js';
import {
  findProductsProvenanceFailures,
  validateProductsParitySpecEvidence,
} from '../../scripts/protected-evidence-invariants.js';

const repoRoot = new URL('../..', import.meta.url).pathname;

describe('conformance capture index', () => {
  it('indexes every conformance capture script', () => {
    const validation = validateCaptureIndexAgainstScriptFiles(
      conformanceCaptureIndex,
      loadConformanceCaptureScriptPaths(repoRoot),
    );

    expect(validation).toEqual({
      duplicateCaptureIds: [],
      missingFromIndex: [],
      missingFromDisk: [],
    });
  });

  it('keeps entries actionable without opening the capture scripts', () => {
    for (const entry of conformanceCaptureIndex) {
      expect(entry.domain.length, entry.captureId).toBeGreaterThan(0);
      expect(entry.captureId, entry.scriptPath).toMatch(/^[a-z0-9][a-z0-9-]*$/u);
      expect(entry.scriptPath, entry.captureId).toMatch(/^scripts\/.+\.(ts|mts)$/u);
      expect(entry.purpose.length, entry.captureId).toBeGreaterThan(0);
      expect(entry.requiredAuthScopes.length, entry.captureId).toBeGreaterThan(0);
      expect(entry.fixtureOutputs.length, entry.captureId).toBeGreaterThan(0);
      for (const output of entry.fixtureOutputs) {
        expect(output, entry.captureId).toMatch(
          /^(fixtures\/conformance\/|config\/parity-specs\/|config\/parity-requests\/|config\/|src\/)/u,
        );
        expect(output, entry.captureId).not.toContain('*');
      }
      expect(entry.cleanupBehavior.length, entry.captureId).toBeGreaterThan(0);
      expect(entry.expectedStatusChecks.length, entry.captureId).toBeGreaterThan(0);
    }
  });

  it('renders a domain-filterable command table', () => {
    const markdown = renderCaptureIndexMarkdown(conformanceCaptureIndex.filter((entry) => entry.domain === 'products'));

    expect(markdown).toContain('## products');
    expect(markdown).toContain('corepack pnpm conformance:capture -- --run product-mutations');
    expect(markdown).toContain('corepack pnpm exec tsx ./scripts/capture-product-mutation-conformance.mts');
    expect(markdown).toContain('Required auth/scopes');
    expect(markdown).toContain('Cleanup');
    expect(markdown).not.toContain('## customers');
  });

  it('enforces recorder-declared outputs for every checked-in live Shopify fixture', () => {
    const profile = profileConformanceFixtureProvenance(repoRoot);

    expect(profile.fixtureCount).toBeGreaterThan(0);
    expect(profile.liveShopifyFixtureCount).toBeGreaterThan(0);
    expect(profile.localRuntimeFixtureCount).toBeGreaterThan(0);
    expect(profile.indexedFixtureOutputPatterns.length).toBeGreaterThan(0);
    expect(
      profile.orphanedFixturePaths.filter((fixturePath) => fixturePath.includes('/local-runtime/')),
      'local-runtime fixtures are executable runtime evidence and must remain exempt from live Shopify recorder-output enforcement',
    ).toEqual([]);
    expect(
      profile.orphanedFixturePaths,
      'every checked-in live Shopify fixture under fixtures/conformance/**/*.json must be declared by a capture index fixtureOutputs entry',
    ).toEqual([]);
  });

  it('rejects descriptor and local-runtime products evidence in strict parity specs', () => {
    const strictSpec = {
      scenarioStatus: 'captured',
      comparisonMode: 'captured-vs-proxy-request',
      liveCaptureFiles: [
        'fixtures/conformance/local-runtime/2026-04/products/product-feed-lifecycle-local-runtime.json',
        'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json',
      ],
    };

    const failures = validateProductsParitySpecEvidence(
      'config/parity-specs/products/example.json',
      strictSpec,
      () => ({
        upstreamCalls: [
          {
            operationName: 'ProductsHydrateNodes',
            variables: { ids: ['gid://shopify/Product/1'] },
            query: 'hand-synthesized from a setup product',
            response: { status: 200, body: { data: { nodes: [] } } },
          },
        ],
      }),
    );

    expect(failures.map((failure) => failure.message)).toEqual([
      'strict products parity spec references local-runtime fixture fixtures/conformance/local-runtime/2026-04/products/product-feed-lifecycle-local-runtime.json; use captured-fixture/local-runtime-backed metadata instead',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json: upstreamCalls[0].query is not a valid GraphQL document: "hand-synthesized from a setup product"',
    ]);
  });

  it('allows local-runtime products evidence when the spec is explicitly runtime-fixture backed', () => {
    const failures = validateProductsParitySpecEvidence(
      'config/parity-specs/products/example.json',
      {
        scenarioStatus: 'captured',
        comparisonMode: 'captured-fixture',
        liveCaptureFiles: [
          'fixtures/conformance/local-runtime/2026-04/products/product-feed-lifecycle-local-runtime.json',
        ],
      },
      () => {
        throw new Error('fixture loader should not be called for captured-fixture specs');
      },
    );

    expect(failures).toEqual([]);
  });

  it('keeps checked-in products strict parity free of local-runtime and descriptor cassettes', () => {
    expect(findProductsProvenanceFailures(repoRoot)).toEqual([]);
  });
});
