import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('inventoryAdjustQuantities parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged available-quantity adjustment slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/inventoryAdjustQuantities-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      blocker?: unknown;
      proxyRequest?: { documentPath?: string | null; variablesCapturePath?: string | null };
      comparison?: {
        targets?: Array<{
          name?: string;
          proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
        }>;
      };
    };

    expect(spec.proxyRequest?.documentPath).toBe(
      'config/parity-requests/inventoryAdjustQuantities-parity-plan.graphql',
    );
    expect(spec.proxyRequest?.variablesCapturePath).toBe('$.mutation.variables');
    expect(spec.blocker).toBeUndefined();

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);

    expect(existsSync(documentPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');

    expect(document).toContain('mutation InventoryAdjustQuantitiesParityPlan($input: InventoryAdjustQuantitiesInput!)');
    expect(document).toContain('inventoryAdjustQuantities(input: $input)');
    expect(document).toContain('inventoryAdjustmentGroup {');
    expect(document).toContain('id');
    expect(document).toContain('changes {');
    expect(document).toContain('quantityAfterChange');
    expect(document).toContain('ledgerDocumentUri');
    expect(document).toContain('item {');
    expect(document).toContain('location {');
    expect(document).toContain('name');
    expect(document).toContain('userErrors {');

    expect(spec.comparison?.targets?.map((target) => target.name)).toEqual([
      'mutation-user-errors',
      'mutation-reason',
      'mutation-reference-document-uri',
      'mutation-changes',
      'downstream-read-data',
    ]);

    const downstreamTarget = spec.comparison?.targets?.find((target) => target.name === 'downstream-read-data');
    expect(downstreamTarget?.proxyRequest).toEqual({
      documentPath: 'config/parity-requests/inventoryAdjustQuantities-downstream-read.graphql',
      variablesPath: 'config/parity-requests/inventoryAdjustQuantities-downstream-read.variables.json',
    });
    expect(existsSync(resolve(repoRoot, downstreamTarget!.proxyRequest!.documentPath!))).toBe(true);
    expect(existsSync(resolve(repoRoot, downstreamTarget!.proxyRequest!.variablesPath!))).toBe(true);
  });
});
