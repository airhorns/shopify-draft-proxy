import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  classifyParityScenarioState,
  validateComparisonContract,
  type ParitySpec,
} from '../../scripts/conformance-parity-lib.js';
import { loadConformanceScenarios } from '../../scripts/conformance-scenario-registry.js';

const repoRoot = resolve(import.meta.dirname, '../..');
const compatibilityRootToBulkCapture = new Map([
  ['productVariantCreate', 'product-variants-bulk-create-parity.json'],
  ['productVariantUpdate', 'product-variants-bulk-update-parity.json'],
  ['productVariantDelete', 'product-variants-bulk-delete-parity.json'],
]);

function readParitySpec(relativePath: string): ParitySpec {
  return JSON.parse(readFileSync(resolve(repoRoot, relativePath), 'utf8')) as ParitySpec;
}

describe('product variant compatibility evidence', () => {
  it('keeps the dedicated live schema probe executable through tsx', () => {
    const scriptPath = resolve(repoRoot, 'scripts/probe-product-variant-compatibility-roots.mts');

    expect(existsSync(scriptPath)).toBe(true);
    const script = readFileSync(scriptPath, 'utf8');
    expect(script).toContain('mutation ProductVariantCreateCompatibilityProbe');
    expect(script).toContain('productVariantCreate(input: $input)');
    expect(script).toContain('productVariantUpdate(input: $input)');
    expect(script).toContain('productVariantDelete(id: $id)');
    expect(script).toContain("from './conformance-graphql-client.js'");
    expect(script).toContain('legacyRootsExposed: false');
    expect(script).not.toContain('Why this blocks closure');
    expect(script).toContain('HAR-189');
  });

  it('discovers the legacy roots as runnable compatibility-wrapper parity backed by bulk captures', () => {
    const scenarios = loadConformanceScenarios(repoRoot);

    for (const [operationName, expectedBulkCapture] of compatibilityRootToBulkCapture) {
      const scenario = scenarios.find((candidate) => candidate.operationNames.includes(operationName));
      expect(scenario, `${operationName} should have a discovered parity scenario`).toBeDefined();

      if (!scenario) {
        continue;
      }

      const paritySpec = readParitySpec(scenario.paritySpecPath);
      expect(classifyParityScenarioState(scenario, paritySpec)).toBe('ready-for-comparison');
      expect(validateComparisonContract(paritySpec.comparison)).toEqual([]);
      expect(paritySpec.comparisonMode).toBe('captured-compatibility-wrapper');
      expect(paritySpec.blocker, `${operationName} should not depend on blocked pending-doc evidence`).toBeUndefined();
      expect(scenario.captureFiles.some((file) => file.endsWith(expectedBulkCapture))).toBe(true);
      expect(JSON.stringify(paritySpec)).not.toContain('pending/');
    }
  });
});
