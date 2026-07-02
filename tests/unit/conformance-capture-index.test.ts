import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  conformanceCaptureIndex,
  loadConformanceCaptureScriptPaths,
  profileConformanceFixtureProvenance,
  renderCaptureIndexMarkdown,
  validateCaptureIndexAgainstScriptFiles,
} from '../../scripts/conformance-capture-index.js';
import { isGraphqlDocumentText } from '../../scripts/parity-cassette.js';
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

  it('rejects descriptor and local-runtime products evidence in products parity specs', () => {
    const spec = {
      scenarioStatus: 'captured',
      comparisonMode: 'captured-vs-proxy-request',
      liveCaptureFiles: [
        'fixtures/conformance/local-runtime/2026-04/products/product-feed-lifecycle-local-runtime.json',
        'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json',
      ],
    };

    const failures = validateProductsParitySpecEvidence('config/parity-specs/products/example.json', spec, () => ({
      upstreamCalls: [
        {
          operationName: 'ProductsHydrateNodes',
          variables: { ids: ['gid://shopify/Product/1'] },
          query: 'hand-synthesized from a setup product',
          response: { status: 200, body: { data: { nodes: [] } } },
        },
      ],
    }));

    expect(failures.map((failure) => failure.message)).toEqual([
      'products/store-properties parity spec references local-runtime fixture fixtures/conformance/local-runtime/2026-04/products/product-feed-lifecycle-local-runtime.json; remove the synthetic fixture/spec from parity evidence or replace it with live Shopify capture',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json: upstreamCalls[0].query is not a valid GraphQL document: "hand-synthesized from a setup product"',
    ]);
  });

  it('rejects local-runtime products evidence even when labeled captured-fixture', () => {
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
        throw new Error('fixture loader should not be called for local-runtime products evidence');
      },
    );

    expect(failures.map((failure) => failure.message)).toEqual([
      'products/store-properties parity spec references local-runtime fixture fixtures/conformance/local-runtime/2026-04/products/product-feed-lifecycle-local-runtime.json; remove the synthetic fixture/spec from parity evidence or replace it with live Shopify capture',
    ]);
  });

  it('keeps checked-in products strict parity free of local-runtime and descriptor cassettes', () => {
    expect(findProductsProvenanceFailures(repoRoot)).toEqual([]);
  });

  it('rejects synthetic store-properties parity evidence', () => {
    const specRoot = path.join(repoRoot, 'config/parity-specs/store-properties');
    const descriptorPattern =
      /\b(hand-synthesized|cassette-backed|recorded by scripts|local-runtime)\b|^sha(?:256)?:/iu;

    for (const filename of readdirSync(specRoot).filter((entry) => entry.endsWith('.json'))) {
      const specPath = path.join(specRoot, filename);
      const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
        scenarioId?: string;
        comparisonMode?: string;
        liveCaptureFiles?: string[];
      };
      if (spec.comparisonMode !== 'captured-vs-proxy-request') {
        continue;
      }

      for (const capturePath of spec.liveCaptureFiles ?? []) {
        expect(capturePath, `${spec.scenarioId} must not use local-runtime parity evidence`).not.toContain(
          'fixtures/conformance/local-runtime/',
        );
        if (!capturePath.includes('/store-properties/')) {
          continue;
        }

        const capture = JSON.parse(readFileSync(path.join(repoRoot, capturePath), 'utf8')) as {
          upstreamCalls?: Array<{ query?: unknown }>;
        };
        for (const [index, call] of (capture.upstreamCalls ?? []).entries()) {
          expect(
            typeof call.query === 'string' && isGraphqlDocumentText(call.query),
            `${spec.scenarioId} upstreamCalls[${index}].query must be an exact GraphQL document`,
          ).toBe(true);
          expect(
            call.query,
            `${spec.scenarioId} upstreamCalls[${index}].query must not be a provenance descriptor`,
          ).not.toMatch(descriptorPattern);
        }
      }
    }
  });
});
