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
    expect(script).toContain('product-variant-compatibility-live-schema-blocker.md');
  });

  it('records a blocker note with the exact missing live mutation roots', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const blockerPath = resolve(repoRoot, 'pending/product-variant-compatibility-live-schema-blocker.md');

    expect(existsSync(blockerPath)).toBe(true);

    const blockerNote = readFileSync(blockerPath, 'utf8');
    expect(blockerNote).toContain('`productVariantCreate`');
    expect(blockerNote).toContain('`productVariantUpdate`');
    expect(blockerNote).toContain('`productVariantDelete`');
    expect(blockerNote).toContain("Field 'productVariantCreate' doesn't exist on type 'Mutation'");
    expect(blockerNote).toContain("Field 'productVariantUpdate' doesn't exist on type 'Mutation'");
    expect(blockerNote).toContain("Field 'productVariantDelete' doesn't exist on type 'Mutation'");
    expect(blockerNote).toContain('productVariantsBulkCreate');
    expect(blockerNote).toContain('productVariantsBulkUpdate');
    expect(blockerNote).toContain('productVariantsBulkDelete');
  });
});
