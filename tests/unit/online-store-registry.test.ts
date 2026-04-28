import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import {
  buildConformanceStatusDocument,
  loadConformanceScenarios,
} from '../../scripts/conformance-scenario-registry.js';
import { operationRegistrySchema } from '../../src/json-schemas.js';
import { getOperationCapability } from '../../src/proxy/capabilities.js';

const repoRoot = resolve(import.meta.dirname, '../..');

const onlineStoreQueryRoots = [
  'menu',
  'menus',
  'theme',
  'themes',
  'scriptTag',
  'scriptTags',
  'webPixel',
  'serverPixel',
  'mobilePlatformApplication',
  'mobilePlatformApplications',
] as const;

const unsupportedOnlineStoreQueryRoots = ['menu', 'menus'] as const;
const unsupportedOnlineStoreMutationRoots = ['menuCreate', 'menuUpdate', 'menuDelete'] as const;
const unsupportedOnlineStoreRoots = [...unsupportedOnlineStoreQueryRoots, ...unsupportedOnlineStoreMutationRoots] as const;

const supportedOnlineStoreQueryRoots = [
  'theme',
  'themes',
  'scriptTag',
  'scriptTags',
  'webPixel',
  'serverPixel',
  'mobilePlatformApplication',
  'mobilePlatformApplications',
] as const;

const onlineStoreMutationRoots = [
  'menuCreate',
  'menuUpdate',
  'menuDelete',
  'themeCreate',
  'themeUpdate',
  'themeDelete',
  'themePublish',
  'themeFilesCopy',
  'themeFilesUpsert',
  'themeFilesDelete',
  'scriptTagCreate',
  'scriptTagUpdate',
  'scriptTagDelete',
  'webPixelCreate',
  'webPixelUpdate',
  'webPixelDelete',
  'serverPixelCreate',
  'serverPixelDelete',
  'eventBridgeServerPixelUpdate',
  'pubSubServerPixelUpdate',
  'storefrontAccessTokenCreate',
  'storefrontAccessTokenDelete',
  'mobilePlatformApplicationCreate',
  'mobilePlatformApplicationUpdate',
  'mobilePlatformApplicationDelete',
] as const;

const supportedOnlineStoreMutationRoots = [
  'themeCreate',
  'themeUpdate',
  'themeDelete',
  'themePublish',
  'themeFilesCopy',
  'themeFilesUpsert',
  'themeFilesDelete',
  'scriptTagCreate',
  'scriptTagUpdate',
  'scriptTagDelete',
  'webPixelCreate',
  'webPixelUpdate',
  'webPixelDelete',
  'serverPixelCreate',
  'serverPixelDelete',
  'eventBridgeServerPixelUpdate',
  'pubSubServerPixelUpdate',
  'storefrontAccessTokenCreate',
  'storefrontAccessTokenDelete',
  'mobilePlatformApplicationCreate',
  'mobilePlatformApplicationUpdate',
  'mobilePlatformApplicationDelete',
] as const;

const rootOperationIntrospectionFixtureSchema = z.object({
  introspection: z.object({
    data: z.object({
      queryRoot: z.object({
        fields: z.array(z.strictObject({ name: z.string().min(1) })),
      }),
      mutationRoot: z.object({
        fields: z.array(z.strictObject({ name: z.string().min(1) })),
      }),
    }),
  }),
});

function readText(relativePath: string): string {
  return readFileSync(resolve(repoRoot, relativePath), 'utf8');
}

function readRegistry() {
  return operationRegistrySchema.parse(JSON.parse(readText('config/operation-registry.json')));
}

function readIntrospectionRoots() {
  const fixture = rootOperationIntrospectionFixtureSchema.parse(
    JSON.parse(
      readText(
        'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json',
      ),
    ),
  );

  return {
    queryRoots: new Set(fixture.introspection.data.queryRoot.fields.map((field) => field.name)),
    mutationRoots: new Set(fixture.introspection.data.mutationRoot.fields.map((field) => field.name)),
  };
}

describe('Online store registry scaffold', () => {
  it('declares storefront read roots separately from side-effect mutation roots', () => {
    const registry = readRegistry();
    const entriesByName = new Map(registry.map((entry) => [entry.name, entry]));
    const { queryRoots, mutationRoots } = readIntrospectionRoots();

    for (const root of onlineStoreQueryRoots) {
      expect(queryRoots.has(root), `${root} should exist in the checked-in query-root introspection`).toBe(true);
      expect(entriesByName.get(root)).toMatchObject({
        domain: 'online-store',
        execution: 'overlay-read',
      });
    }

    for (const root of onlineStoreMutationRoots) {
      expect(mutationRoots.has(root), `${root} should exist in the checked-in mutation-root introspection`).toBe(true);
      expect(entriesByName.get(root)).toMatchObject({
        domain: 'online-store',
        execution: 'stage-locally',
      });
    }
  });

  it('keeps unsupported navigation roots out of supported runtime capability routing', () => {
    const registry = readRegistry();
    const entriesByName = new Map(registry.map((entry) => [entry.name, entry]));

    for (const root of unsupportedOnlineStoreRoots) {
      expect(entriesByName.get(root)).toMatchObject({
        domain: 'online-store',
        implemented: false,
        runtimeTests: [],
      });
    }

    for (const root of unsupportedOnlineStoreQueryRoots) {
      expect(getOperationCapability({ type: 'query', name: root, rootFields: [root] })).toEqual({
        domain: 'unknown',
        execution: 'passthrough',
        operationName: root,
        type: 'query',
      });
    }

    for (const root of unsupportedOnlineStoreMutationRoots) {
      expect(getOperationCapability({ type: 'mutation', name: root, rootFields: [root] })).toEqual({
        domain: 'unknown',
        execution: 'passthrough',
        operationName: root,
        type: 'mutation',
      });
    }
  });

  it('routes supported presentation and integration roots to local online-store handlers', () => {
    const registry = readRegistry();
    const entriesByName = new Map(registry.map((entry) => [entry.name, entry]));

    for (const root of supportedOnlineStoreQueryRoots) {
      expect(entriesByName.get(root)).toMatchObject({
        implemented: true,
        runtimeTests: ['tests/integration/online-store-integrations-flow.test.ts'],
      });
      expect(getOperationCapability({ type: 'query', name: root, rootFields: [root] })).toEqual({
        domain: 'online-store',
        execution: 'overlay-read',
        operationName: root,
        type: 'query',
      });
    }

    for (const root of supportedOnlineStoreMutationRoots) {
      expect(entriesByName.get(root)).toMatchObject({
        implemented: true,
        runtimeTests: ['tests/integration/online-store-integrations-flow.test.ts'],
      });
      expect(getOperationCapability({ type: 'mutation', name: root, rootFields: [root] })).toEqual({
        domain: 'online-store',
        execution: 'stage-locally',
        operationName: root,
        type: 'mutation',
      });
    }
  });

  it('does not create planned-only parity scenarios for storefront gaps', () => {
    const scenarios = loadConformanceScenarios(repoRoot);
    const scenarioOperations = new Set(scenarios.flatMap((scenario) => scenario.operationNames));
    const statusDocument = buildConformanceStatusDocument(repoRoot);

    for (const root of unsupportedOnlineStoreRoots) {
      expect(scenarioOperations.has(root), `${root} should wait for captured executable evidence`).toBe(false);
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(false);
    }
  });
});
