import { describe, expect, it } from 'vitest';

import {
  conformanceCaptureIndex,
  loadPackageCaptureScripts,
  renderCaptureIndexMarkdown,
  validateCaptureIndexAgainstPackageScripts,
} from '../../scripts/conformance-capture-index.js';

const repoRoot = new URL('../..', import.meta.url).pathname;

describe('conformance capture index', () => {
  it('indexes every packaged conformance capture command', () => {
    const validation = validateCaptureIndexAgainstPackageScripts(
      conformanceCaptureIndex,
      loadPackageCaptureScripts(repoRoot),
    );

    expect(validation).toEqual({
      missingFromIndex: [],
      missingFromPackage: [],
      scriptPathMismatches: [],
    });
  });

  it('keeps entries actionable without opening the capture scripts', () => {
    for (const entry of conformanceCaptureIndex) {
      expect(entry.domain.length, entry.packageScript).toBeGreaterThan(0);
      expect(entry.scriptPath, entry.packageScript).toMatch(/^scripts\/.+\.(ts|mts)$/u);
      expect(entry.purpose.length, entry.packageScript).toBeGreaterThan(0);
      expect(entry.requiredAuthScopes.length, entry.packageScript).toBeGreaterThan(0);
      expect(entry.fixtureOutputs.length, entry.packageScript).toBeGreaterThan(0);
      expect(entry.cleanupBehavior.length, entry.packageScript).toBeGreaterThan(0);
      expect(entry.expectedStatusChecks.length, entry.packageScript).toBeGreaterThan(0);
    }
  });

  it('renders a domain-filterable command table', () => {
    const markdown = renderCaptureIndexMarkdown(conformanceCaptureIndex.filter((entry) => entry.domain === 'products'));

    expect(markdown).toContain('## products');
    expect(markdown).toContain('corepack pnpm conformance:capture-product-mutations');
    expect(markdown).toContain('Required auth/scopes');
    expect(markdown).toContain('Cleanup');
    expect(markdown).not.toContain('## customers');
  });
});
