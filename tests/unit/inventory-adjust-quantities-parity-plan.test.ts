import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('inventoryAdjustQuantities parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged available-quantity adjustment slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/inventoryAdjustQuantities-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/inventoryAdjustQuantities-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/inventoryAdjustQuantities-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);

    expect(existsSync(documentPath)).toBe(true);
    expect(existsSync(variablesPath)).toBe(true);

    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      input?: {
        name?: string;
        reason?: string;
        referenceDocumentUri?: string;
        changes?: Array<Record<string, unknown>>;
      };
    };

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

    expect(variables.input).toMatchObject({
      name: 'available',
      reason: 'correction',
      referenceDocumentUri: 'logistics://cycle-count/2026-04-15',
      changes: [
        {
          inventoryItemId: 'gid://shopify/InventoryItem/8001',
          locationId: 'gid://shopify/Location/1',
          delta: -2,
        },
        {
          inventoryItemId: 'gid://shopify/InventoryItem/8002',
          locationId: 'gid://shopify/Location/1',
          delta: 4,
        },
      ],
    });
  });
});
