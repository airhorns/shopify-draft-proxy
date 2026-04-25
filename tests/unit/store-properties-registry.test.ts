import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  buildConformanceStatusDocument,
  loadConformanceScenarios,
} from '../../scripts/conformance-scenario-registry.js';
import { operationRegistrySchema } from '../../src/json-schemas.js';
import { getOperationCapability } from '../../src/proxy/capabilities.js';

const repoRoot = resolve(import.meta.dirname, '../..');

const storePropertiesQueryRoots = ['location', 'locationByIdentifier', 'cashManagementLocationSummary'] as const;

const implementedStorePropertiesQueryRoots = ['shop', 'businessEntities', 'businessEntity'] as const;

const storePropertiesMutationRoots = [
  'locationAdd',
  'locationEdit',
  'locationActivate',
  'locationDeactivate',
  'locationDelete',
  'publishablePublish',
  'publishablePublishToCurrentChannel',
  'publishableUnpublish',
  'publishableUnpublishToCurrentChannel',
  'shopPolicyUpdate',
] as const;

const storePropertiesLocalStagingMutationRoots = [
  'locationAdd',
  'locationEdit',
  'locationActivate',
  'locationDeactivate',
  'locationDelete',
] as const;

const implementedStorePropertiesLocalStagingMutationRoots = ['shopPolicyUpdate'] as const;

const genericPublishableMutationRoots = [
  'publishablePublish',
  'publishablePublishToCurrentChannel',
  'publishableUnpublish',
  'publishableUnpublishToCurrentChannel',
] as const;

const storePropertiesRoots = [
  ...storePropertiesQueryRoots,
  ...implementedStorePropertiesQueryRoots,
  ...storePropertiesMutationRoots,
] as const;
const collectionSupportedPublishableRoots = ['publishablePublish', 'publishableUnpublish'] as const;

function readText(relativePath: string): string {
  return readFileSync(resolve(repoRoot, relativePath), 'utf8');
}

function readRegistry() {
  return operationRegistrySchema.parse(JSON.parse(readText('config/operation-registry.json')));
}

describe('Store properties registry scaffold', () => {
  it('tracks Store properties roots without enabling broad runtime support prematurely', () => {
    const registry = readRegistry();
    const entriesByName = new Map(registry.map((entry) => [entry.name, entry]));

    for (const root of storePropertiesRoots) {
      const entry = entriesByName.get(root);
      expect(entry, `${root} should be declared in the operation registry`).toBeDefined();
      expect(entry?.domain, `${root} should be grouped under Store properties`).toBe('store-properties');
    }

    for (const root of [...storePropertiesQueryRoots, ...storePropertiesLocalStagingMutationRoots]) {
      const entry = entriesByName.get(root);
      expect(entry?.implemented, `${root} should remain scaffold-only`).toBe(false);
      expect(entry?.runtimeTests, `${root} should not claim runtime coverage`).toEqual([]);
    }

    for (const root of implementedStorePropertiesQueryRoots) {
      const entry = entriesByName.get(root);
      expect(entry?.implemented, `${root} should now have runtime support`).toBe(true);
      expect(entry?.runtimeTests.length, `${root} should declare targeted integration coverage`).toBeGreaterThan(0);
    }

    for (const root of storePropertiesQueryRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should be a planned overlay read`).toBe('overlay-read');
      expect(entriesByName.get(root)?.implemented, `${root} should remain scaffold-only`).toBe(false);
      expect(entriesByName.get(root)?.runtimeTests, `${root} should not claim runtime coverage`).toEqual([]);
    }

    for (const root of storePropertiesLocalStagingMutationRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should be planned for local staging before support`).toBe(
        'stage-locally',
      );
      expect(entriesByName.get(root)?.implemented, `${root} should remain scaffold-only`).toBe(false);
      expect(entriesByName.get(root)?.runtimeTests, `${root} should not claim runtime coverage`).toEqual([]);
    }

    for (const root of implementedStorePropertiesLocalStagingMutationRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should be a supported local staging mutation`).toBe(
        'stage-locally',
      );
      expect(entriesByName.get(root)?.implemented, `${root} should now have runtime support`).toBe(true);
      expect(entriesByName.get(root)?.runtimeTests, `${root} should declare targeted integration coverage`).toContain(
        'tests/integration/shop-policy-update-flow.test.ts',
      );
    }

    for (const root of implementedStorePropertiesQueryRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should be a supported overlay read`).toBe('overlay-read');
    }

    for (const root of genericPublishableMutationRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should stage locally for product targets`).toBe(
        'stage-locally',
      );
      expect(entriesByName.get(root)?.implemented, `${root} should expose the product-scoped slice`).toBe(true);
      expect(entriesByName.get(root)?.runtimeTests, `${root} should declare runtime coverage`).toContain(
        'tests/integration/product-draft-flow.test.ts',
      );
      expect(entriesByName.get(root)?.supportNotes, `${root} should explain the product-scoped support`).toEqual(
        expect.stringContaining('Product'),
      );
    }

    for (const root of collectionSupportedPublishableRoots) {
      expect(entriesByName.get(root)?.runtimeTests, `${root} should declare collection runtime coverage`).toContain(
        'tests/integration/collection-draft-flow.test.ts',
      );
      expect(entriesByName.get(root)?.supportNotes, `${root} should explain the collection-scoped support`).toEqual(
        expect.stringContaining('Collection'),
      );
    }
  });

  it('does not register permanent passthrough capabilities', () => {
    const registry = JSON.parse(readText('config/operation-registry.json')) as Array<{ execution?: string }>;
    expect(registry.some((entry) => entry.execution === 'passthrough')).toBe(false);
  });

  it('keeps planned local-staging scaffolds out of capability routing until they are implemented', () => {
    expect(getOperationCapability({ type: 'query', name: 'Shop', rootFields: ['shop'] })).toEqual({
      domain: 'store-properties',
      execution: 'overlay-read',
      operationName: 'Shop',
      type: 'query',
    });

    expect(
      getOperationCapability({ type: 'mutation', name: 'LocationDelete', rootFields: ['locationDelete'] }),
    ).toEqual({
      domain: 'unknown',
      execution: 'passthrough',
      operationName: 'LocationDelete',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: 'LocationAdd', rootFields: ['locationAdd'] })).toEqual({
      domain: 'unknown',
      execution: 'passthrough',
      operationName: 'LocationAdd',
      type: 'mutation',
    });
  });

  it('routes generic publishable roots as Store properties local staging', () => {
    for (const root of genericPublishableMutationRoots) {
      expect(getOperationCapability({ type: 'mutation', name: root, rootFields: [root] })).toEqual({
        domain: 'store-properties',
        execution: 'stage-locally',
        operationName: root,
        type: 'mutation',
      });
    }
  });

  it('routes implemented shopPolicyUpdate as Store properties local staging', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'ShopPolicyUpdate', rootFields: ['shopPolicyUpdate'] }),
    ).toEqual({
      domain: 'store-properties',
      execution: 'stage-locally',
      operationName: 'ShopPolicyUpdate',
      type: 'mutation',
    });
  });

  it('routes implemented Store properties reads through the overlay', () => {
    expect(getOperationCapability({ type: 'query', name: 'Shop', rootFields: ['shop'] })).toEqual({
      domain: 'store-properties',
      execution: 'overlay-read',
      operationName: 'Shop',
      type: 'query',
    });

    expect(
      getOperationCapability({ type: 'query', name: 'BusinessEntities', rootFields: ['businessEntities'] }),
    ).toEqual({
      domain: 'store-properties',
      execution: 'overlay-read',
      operationName: 'BusinessEntities',
      type: 'query',
    });

    expect(getOperationCapability({ type: 'query', name: 'BusinessEntity', rootFields: ['businessEntity'] })).toEqual({
      domain: 'store-properties',
      execution: 'overlay-read',
      operationName: 'BusinessEntity',
      type: 'query',
    });
  });

  it('does not create planned-only parity scenarios for scaffold-only Store properties roots', () => {
    const scenarios = loadConformanceScenarios(repoRoot);
    const scenarioOperations = new Set(scenarios.flatMap((scenario) => scenario.operationNames));
    const statusDocument = buildConformanceStatusDocument(repoRoot);

    for (const root of [...storePropertiesQueryRoots, ...storePropertiesLocalStagingMutationRoots]) {
      expect(scenarioOperations.has(root), `${root} should wait for captured evidence or executable comparison`).toBe(
        false,
      );
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(false);
    }

    for (const root of implementedStorePropertiesQueryRoots) {
      expect(scenarioOperations.has(root), `${root} should now have captured business entity evidence`).toBe(true);
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(true);
    }

    for (const root of implementedStorePropertiesLocalStagingMutationRoots) {
      expect(scenarioOperations.has(root), `${root} should now have captured mutation evidence`).toBe(true);
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(true);
    }

    for (const root of genericPublishableMutationRoots) {
      expect(scenarioOperations.has(root), `${root} should now have product-scoped publication evidence`).toBe(true);
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(true);
    }
  });

  it('wires the Store properties capture path through the canonical conformance auth helper', () => {
    const packageJson = JSON.parse(readText('package.json')) as { scripts?: Record<string, string> };
    expect(packageJson.scripts?.['conformance:capture-store-properties']).toBe(
      'tsx ./scripts/capture-location-conformance.mts',
    );

    const captureScript = readText('scripts/capture-location-conformance.mts');
    expect(captureScript).toContain('getValidConformanceAccessToken');
    expect(captureScript).toContain('buildAdminAuthHeaders');
    expect(captureScript).not.toContain('SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN');
  });
});
