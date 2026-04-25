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
  'businessEntities',
  'businessEntity',
  'cashManagementLocationSummary',
] as const;

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
  'shopPolicyUpdate',
] as const;

const genericPublishableMutationRoots = [
  'publishablePublish',
  'publishablePublishToCurrentChannel',
  'publishableUnpublish',
  'publishableUnpublishToCurrentChannel',
] as const;

const storePropertiesRoots = [...storePropertiesQueryRoots, ...storePropertiesMutationRoots] as const;

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
      expect(entry?.implemented, `${root} should remain scaffold-only`).toBe(false);
      expect(entry?.runtimeTests, `${root} should not claim runtime coverage`).toEqual([]);
    }

    for (const root of storePropertiesQueryRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should be a planned overlay read`).toBe('overlay-read');
    }

    for (const root of storePropertiesLocalStagingMutationRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should be planned for local staging before support`).toBe(
        'stage-locally',
      );
    }

    for (const root of genericPublishableMutationRoots) {
      expect(entriesByName.get(root)?.execution, `${root} should be explicit tracked passthrough`).toBe('passthrough');
    }
  });

  it('keeps planned local-staging scaffolds out of capability routing until they are implemented', () => {
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

  it('routes generic publishable roots as observable Store properties passthrough', () => {
    for (const root of genericPublishableMutationRoots) {
      expect(getOperationCapability({ type: 'mutation', name: root, rootFields: [root] })).toEqual({
        domain: 'store-properties',
        execution: 'passthrough',
        operationName: root,
        type: 'mutation',
      });
    }
  });

  it('does not create planned-only parity scenarios for scaffold-only Store properties roots', () => {
    const scenarios = loadConformanceScenarios(repoRoot);
    const scenarioOperations = new Set(scenarios.flatMap((scenario) => scenario.operationNames));
    const statusDocument = buildConformanceStatusDocument(repoRoot);

    for (const root of storePropertiesRoots) {
      expect(scenarioOperations.has(root), `${root} should wait for captured evidence or executable comparison`).toBe(
        false,
      );
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(false);
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
