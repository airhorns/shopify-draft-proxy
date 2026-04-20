import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('inventoryItemUpdate parity plan scaffold', () => {
  it('declares a concrete proxy request scaffold for the staged inventory item metadata slice', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const specPath = resolve(repoRoot, 'config/parity-specs/inventoryItemUpdate-parity-plan.json');
    const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
      proxyRequest?: { documentPath?: string | null; variablesPath?: string | null };
    };

    expect(spec.proxyRequest?.documentPath).toBe('config/parity-requests/inventoryItemUpdate-parity-plan.graphql');
    expect(spec.proxyRequest?.variablesPath).toBe('config/parity-requests/inventoryItemUpdate-parity-plan.variables.json');

    const documentPath = resolve(repoRoot, spec.proxyRequest!.documentPath!);
    const variablesPath = resolve(repoRoot, spec.proxyRequest!.variablesPath!);
    const document = readFileSync(documentPath, 'utf8');
    const variables = JSON.parse(readFileSync(variablesPath, 'utf8')) as {
      id?: string;
      input?: {
        tracked?: boolean;
        requiresShipping?: boolean;
        countryCodeOfOrigin?: string;
        provinceCodeOfOrigin?: string;
        harmonizedSystemCode?: string;
        measurement?: { weight?: { unit?: string; value?: number } };
      };
    };

    expect(document).toContain('mutation InventoryItemUpdateParityPlan($id: ID!, $input: InventoryItemInput!)');
    expect(document).toContain('inventoryItemUpdate(id: $id, input: $input)');
    expect(document).toContain('tracked');
    expect(document).toContain('requiresShipping');
    expect(document).toContain('countryCodeOfOrigin');
    expect(document).toContain('provinceCodeOfOrigin');
    expect(document).toContain('harmonizedSystemCode');
    expect(document).toContain('measurement {');
    expect(document).toContain('weight {');
    expect(document).toContain('variant {');
    expect(document).toContain('userErrors { field message }');

    expect(variables).toEqual({
      id: 'gid://shopify/InventoryItem/8061',
      input: {
        tracked: true,
        requiresShipping: false,
        countryCodeOfOrigin: 'CA',
        provinceCodeOfOrigin: 'ON',
        harmonizedSystemCode: '620343',
        measurement: {
          weight: {
            unit: 'KILOGRAMS',
            value: 2.5,
          },
        },
      },
    });
  });
});
