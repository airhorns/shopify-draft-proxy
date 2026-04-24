import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('product variant compatibility live schema blocker', () => {
  it('keeps a dedicated schema probe script for the single-variant compatibility family', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const scriptPath = resolve(repoRoot, 'scripts/probe-product-variant-compatibility-roots.mts');

    expect(existsSync(scriptPath)).toBe(true);

    const script = readFileSync(scriptPath, 'utf8');
    expect(script).toContain('mutation ProductVariantCreateCompatibilityProbe');
    expect(script).toContain('productVariantCreate(input: $input)');
    expect(script).toContain('productVariantUpdate(input: $input)');
    expect(script).toContain('productVariantDelete(id: $id)');
    expect(script).toContain('HAR-189');
  });

  it('keeps compatibility evidence in parity metadata', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPaths = [
      'config/parity-specs/productVariantCreate-parity-plan.json',
      'config/parity-specs/productVariantUpdate-parity-plan.json',
      'config/parity-specs/productVariantDelete-parity-plan.json',
    ];

    const specs = specPaths.map((specPath) => readFileSync(resolve(repoRoot, specPath), 'utf8'));
    expect(specs.join('\n')).toContain('HAR-189');
    expect(specs.join('\n')).toContain('productVariantsBulkCreate');
    expect(specs.join('\n')).toContain('productVariantsBulkUpdate');
    expect(specs.join('\n')).toContain('productVariantsBulkDelete');
    for (const spec of specs) {
      expect(spec).not.toContain('pending/');
    }
  });
});
