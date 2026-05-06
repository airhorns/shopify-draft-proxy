import { describe, expect, it } from 'vitest';

import {
  conformanceCaptureIndex,
  loadConformanceCaptureScriptPaths,
  profileConformanceFixtureProvenance,
  renderCaptureIndexMarkdown,
  validateCaptureIndexAgainstScriptFiles,
} from '../../scripts/conformance-capture-index.js';

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

  it('profiles checked-in live Shopify fixtures without recorder-declared outputs', () => {
    const profile = profileConformanceFixtureProvenance(repoRoot);

    expect(profile.fixtureCount).toBeGreaterThan(0);
    expect(profile.liveShopifyFixtureCount).toBeGreaterThan(0);
    expect(profile.localRuntimeFixtureCount).toBeGreaterThan(0);
    expect(profile.indexedFixtureOutputPatterns.length).toBeGreaterThan(0);
    expect(profile.orphanedFixturePaths).not.toContainEqual(expect.stringContaining('/local-runtime/'));
    expect(profile.orphanedFixturePaths).toMatchSnapshot();
  });
});
