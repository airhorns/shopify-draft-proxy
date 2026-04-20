import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('inventory linkage parity plan scaffolds', () => {
  it('declares concrete proxy request scaffolds for the captured minimal inventoryActivate, inventoryDeactivate, and inventoryBulkToggleActivation slices', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    const activateSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/inventoryActivate-parity-plan.json'), 'utf8'),
    ) as { proxyRequest?: { documentPath?: string | null; variablesPath?: string | null } };
    const deactivateSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/inventoryDeactivate-parity-plan.json'), 'utf8'),
    ) as { proxyRequest?: { documentPath?: string | null; variablesPath?: string | null } };
    const bulkSpec = JSON.parse(
      readFileSync(resolve(repoRoot, 'config/parity-specs/inventoryBulkToggleActivation-parity-plan.json'), 'utf8'),
    ) as { proxyRequest?: { documentPath?: string | null; variablesPath?: string | null } };

    expect(activateSpec.proxyRequest?.documentPath).toBe(
      'config/parity-requests/inventoryActivate-parity-plan.graphql',
    );
    expect(activateSpec.proxyRequest?.variablesPath).toBe(
      'config/parity-requests/inventoryActivate-parity-plan.variables.json',
    );
    expect(deactivateSpec.proxyRequest?.documentPath).toBe(
      'config/parity-requests/inventoryDeactivate-parity-plan.graphql',
    );
    expect(deactivateSpec.proxyRequest?.variablesPath).toBe(
      'config/parity-requests/inventoryDeactivate-parity-plan.variables.json',
    );
    expect(bulkSpec.proxyRequest?.documentPath).toBe(
      'config/parity-requests/inventoryBulkToggleActivation-parity-plan.graphql',
    );
    expect(bulkSpec.proxyRequest?.variablesPath).toBe(
      'config/parity-requests/inventoryBulkToggleActivation-parity-plan.variables.json',
    );

    const activateDocumentPath = resolve(repoRoot, activateSpec.proxyRequest!.documentPath!);
    const activateVariablesPath = resolve(repoRoot, activateSpec.proxyRequest!.variablesPath!);
    const deactivateDocumentPath = resolve(repoRoot, deactivateSpec.proxyRequest!.documentPath!);
    const bulkDocumentPath = resolve(repoRoot, bulkSpec.proxyRequest!.documentPath!);

    expect(existsSync(activateDocumentPath)).toBe(true);
    expect(existsSync(activateVariablesPath)).toBe(true);
    expect(existsSync(deactivateDocumentPath)).toBe(true);
    expect(existsSync(bulkDocumentPath)).toBe(true);

    const activateDocument = readFileSync(activateDocumentPath, 'utf8');
    const activateVariables = JSON.parse(readFileSync(activateVariablesPath, 'utf8')) as {
      inventoryItemId?: string;
      locationId?: string;
    };
    const deactivateDocument = readFileSync(deactivateDocumentPath, 'utf8');
    const bulkDocument = readFileSync(bulkDocumentPath, 'utf8');

    expect(activateDocument).toContain('mutation InventoryActivateParityPlan($inventoryItemId: ID!, $locationId: ID!)');
    expect(activateDocument).toContain('inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId)');
    expect(activateDocument).toContain('inventoryLevel {');
    expect(activateDocument).toContain('item {');
    expect(activateDocument).toContain('userErrors {');
    expect(activateVariables).toEqual({
      inventoryItemId: 'gid://shopify/InventoryItem/8001',
      locationId: 'gid://shopify/Location/1',
    });

    expect(deactivateDocument).toContain('mutation InventoryDeactivateParityPlan($inventoryLevelId: ID!)');
    expect(deactivateDocument).toContain('inventoryDeactivate(inventoryLevelId: $inventoryLevelId)');
    expect(deactivateDocument).toContain('userErrors {');

    expect(bulkDocument).toContain('mutation InventoryBulkToggleActivationParityPlan');
    expect(bulkDocument).toContain('inventoryBulkToggleActivation(');
    expect(bulkDocument).toContain('inventoryItem {');
    expect(bulkDocument).toContain('inventoryLevels {');
    expect(bulkDocument).toContain('userErrors {');
    expect(bulkDocument).toContain('code');
  });

  it('does not leave the old multi-location blocker scaffold wired in once live success capture exists', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');

    expect(existsSync(resolve(repoRoot, 'config/parity-specs/inventory-linkage-multi-location-blocker.json'))).toBe(
      false,
    );
    expect(
      existsSync(resolve(repoRoot, 'config/parity-requests/inventory-linkage-multi-location-blocker.graphql')),
    ).toBe(false);
    expect(
      existsSync(resolve(repoRoot, 'config/parity-requests/inventory-linkage-multi-location-blocker.variables.json')),
    ).toBe(false);
    expect(existsSync(resolve(repoRoot, 'pending/inventory-linkage-single-location-blocker.md'))).toBe(false);
  });
});
