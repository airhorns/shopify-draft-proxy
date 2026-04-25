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

const storePropertiesQueryRoots = [
  'shop',
  'location',
  'locationByIdentifier',
  'cashManagementLocationSummary',
] as const;

const implementedStorePropertiesQueryRoots = ['businessEntities', 'businessEntity'] as const;

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

const storePropertiesRoots = [
  ...storePropertiesQueryRoots,
  ...implementedStorePropertiesQueryRoots,
  ...storePropertiesMutationRoots,
] as const;
const locallySupportedPublishableRoots = ['publishablePublish', 'publishableUnpublish'] as const;
const scaffoldOnlyStorePropertiesMutationRoots = storePropertiesMutationRoots.filter(
  (root) => !(locallySupportedPublishableRoots as readonly string[]).includes(root),
);

function readText(relativePath: string): string {
  return readFileSync(resolve(repoRoot, relativePath), 'utf8');
}

function readRegistry() {
  return operationRegistrySchema.parse(JSON.parse(readText('config/operation-registry.json')));
}

describe('Store properties registry scaffold', () => {
  it('tracks Store properties roots without enabling runtime support prematurely', () => {
    const registry = readRegistry();
    const entriesByName = new Map(registry.map((entry) => [entry.name, entry]));

    for (const root of storePropertiesRoots) {
      const entry = entriesByName.get(root);
      expect(entry, `${root} should be declared in the operation registry`).toBeDefined();
      expect(entry?.domain, `${root} should be grouped under Store properties`).toBe('store-properties');
    }

    for (const root of [...storePropertiesQueryRoots, ...scaffoldOnlyStorePropertiesMutationRoots]) {
      const entry = entriesByName.get(root);
      expect(entry?.implemented, `${root} should remain scaffold-only`).toBe(false);
      expect(entry?.runtimeTests, `${root} should not claim runtime coverage`).toEqual([]);
    }

    for (const root of implementedStorePropertiesQueryRoots) {
      const entry = entriesByName.get(root);
      expect(entry?.implemented, `${root} should now have runtime support`).toBe(true);
      expect(entry?.runtimeTests, `${root} should declare its targeted integration coverage`).toEqual([
        'tests/integration/business-entity-query-shapes.test.ts',
      ]);
    }

    for (const root of locallySupportedPublishableRoots) {
      const entry = entriesByName.get(root);
      expect(entry?.implemented, `${root} should be locally supported for Product/Collection publishables`).toBe(true);
      expect(entry?.runtimeTests, `${root} should declare runtime coverage`).toEqual([
        'tests/integration/collection-draft-flow.test.ts',
      ]);
    }

    for (const root of storePropertiesQueryRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should be a planned overlay read`).toBe('overlay-read');
    }

    for (const root of implementedStorePropertiesQueryRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should be a supported overlay read`).toBe('overlay-read');
    }

    for (const root of storePropertiesMutationRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should be planned for local staging before support`).toBe(
        'stage-locally',
      );
    }
  });

  it('keeps scaffolded roots out of capability routing until they are implemented', () => {
    expect(getOperationCapability({ type: 'query', name: 'Shop', rootFields: ['shop'] })).toEqual({
      domain: 'unknown',
      execution: 'passthrough',
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

    expect(
      getOperationCapability({ type: 'mutation', name: 'ShopPolicyUpdate', rootFields: ['shopPolicyUpdate'] }),
    ).toEqual({
      domain: 'unknown',
      execution: 'passthrough',
      operationName: 'ShopPolicyUpdate',
      type: 'mutation',
    });
  });

  it('routes the Product/Collection publishable roots once local staging exists', () => {
    expect(
      getOperationCapability({ type: 'mutation', name: 'PublishablePublish', rootFields: ['publishablePublish'] }),
    ).toEqual({
      domain: 'store-properties',
      execution: 'stage-locally',
      operationName: 'PublishablePublish',
      type: 'mutation',
    });

    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'PublishableUnpublish',
        rootFields: ['publishableUnpublish'],
      }),
    ).toEqual({
      domain: 'store-properties',
      execution: 'stage-locally',
      operationName: 'PublishableUnpublish',
      type: 'mutation',
    });
  });

  it('routes implemented business entity reads through the Store properties overlay', () => {
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

    for (const root of [...storePropertiesQueryRoots, ...scaffoldOnlyStorePropertiesMutationRoots]) {
      expect(scenarioOperations.has(root), `${root} should wait for captured evidence or executable comparison`).toBe(
        false,
      );
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(false);
    }

    for (const root of implementedStorePropertiesQueryRoots) {
      expect(scenarioOperations.has(root), `${root} should now have captured business entity evidence`).toBe(true);
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
