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

const marketingQueryRoots = ['marketingActivities', 'marketingActivity', 'marketingEvent', 'marketingEvents'] as const;
const implementedMarketingQueryRoots = marketingQueryRoots;
const marketingMutationRoots = [
  'marketingActivityCreate',
  'marketingActivityUpdate',
  'marketingActivityCreateExternal',
  'marketingActivityUpdateExternal',
  'marketingActivityUpsertExternal',
  'marketingActivityDeleteExternal',
  'marketingActivitiesDeleteAllExternal',
  'marketingEngagementCreate',
  'marketingEngagementsDelete',
] as const;

const segmentQueryRoots = [
  'segment',
  'segments',
  'segmentsCount',
  'segmentFilters',
  'segmentFilterSuggestions',
  'segmentValueSuggestions',
  'segmentMigrations',
  'customerSegmentMembers',
  'customerSegmentMembersQuery',
  'customerSegmentMembership',
] as const;
const implementedSegmentQueryRoots = [
  'segment',
  'segments',
  'segmentsCount',
  'segmentFilters',
  'segmentFilterSuggestions',
  'segmentValueSuggestions',
  'segmentMigrations',
] as const;
const scaffoldOnlySegmentQueryRoots = [
  'customerSegmentMembers',
  'customerSegmentMembersQuery',
  'customerSegmentMembership',
] as const;
const segmentMutationRoots = [
  'customerSegmentMembersQueryCreate',
  'segmentCreate',
  'segmentUpdate',
  'segmentDelete',
] as const;
const implementedSegmentMutationRoots = ['segmentCreate', 'segmentUpdate', 'segmentDelete'] as const;
const scaffoldOnlySegmentMutationRoots = ['customerSegmentMembersQueryCreate'] as const;

const marketingRoots = [...marketingQueryRoots, ...marketingMutationRoots] as const;
const segmentRoots = [...segmentQueryRoots, ...segmentMutationRoots] as const;
const scaffoldOnlyMarketingAndSegmentRoots = [
  ...marketingMutationRoots,
  ...scaffoldOnlySegmentQueryRoots,
  ...scaffoldOnlySegmentMutationRoots,
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

describe('Marketing and segment registry scaffold', () => {
  it('tracks every marketing and segment root from Admin GraphQL introspection with accurate runtime support status', () => {
    const registry = readRegistry();
    const entriesByName = new Map(registry.map((entry) => [entry.name, entry]));
    const { queryRoots, mutationRoots } = readIntrospectionRoots();

    for (const root of [...marketingQueryRoots, ...segmentQueryRoots]) {
      expect(queryRoots.has(root), `${root} should exist in the checked-in query-root introspection`).toBe(true);
    }

    for (const root of [...marketingMutationRoots, ...segmentMutationRoots]) {
      expect(mutationRoots.has(root), `${root} should exist in the checked-in mutation-root introspection`).toBe(true);
    }

    for (const root of marketingRoots) {
      const entry = entriesByName.get(root);
      expect(entry, `${root} should be declared in the operation registry`).toBeDefined();
      expect(entry?.domain, `${root} should be grouped under Marketing`).toBe('marketing');
    }

    for (const root of segmentRoots) {
      const entry = entriesByName.get(root);
      expect(entry, `${root} should be declared in the operation registry`).toBeDefined();
      expect(entry?.domain, `${root} should be grouped under Segments`).toBe('segments');
    }

    for (const root of scaffoldOnlyMarketingAndSegmentRoots) {
      const entry = entriesByName.get(root);
      expect(entry?.implemented, `${root} should remain scaffold-only`).toBe(false);
      expect(entry?.runtimeTests, `${root} should not claim runtime coverage`).toEqual([]);
      expect(entry?.supportNotes, `${root} should identify future capture/parity work`).toEqual(
        expect.stringContaining('capture'),
      );
    }

    for (const root of implementedSegmentQueryRoots) {
      const entry = entriesByName.get(root);
      expect(entry?.implemented, `${root} should be enabled by HAR-215 segment read coverage`).toBe(true);
      expect(entry?.runtimeTests, `${root} should claim runtime segment read coverage`).toContain(
        'tests/integration/segment-query-shapes.test.ts',
      );
    }

    for (const root of implementedMarketingQueryRoots) {
      const entry = entriesByName.get(root);
      expect(entry?.implemented, `${root} should be enabled by HAR-212 marketing read coverage`).toBe(true);
      expect(entry?.runtimeTests, `${root} should claim runtime marketing read coverage`).toContain(
        'tests/integration/marketing-query-shapes.test.ts',
      );
    }

    for (const root of implementedSegmentMutationRoots) {
      const entry = entriesByName.get(root);
      expect(entry?.implemented, `${root} should be enabled by HAR-216 segment lifecycle coverage`).toBe(true);
      expect(entry?.runtimeTests, `${root} should claim runtime segment lifecycle coverage`).toContain(
        'tests/integration/segment-lifecycle-flow.test.ts',
      );
    }
  });

  it('records local-staging intent for known future mutations without registering passthrough support', () => {
    const registry = readRegistry();
    const entriesByName = new Map(registry.map((entry) => [entry.name, entry]));

    for (const root of [...marketingQueryRoots, ...segmentQueryRoots]) {
      expect(entriesByName.get(root)?.execution, `${root} should be a planned overlay read`).toBe('overlay-read');
    }

    for (const root of [...marketingMutationRoots, ...segmentMutationRoots]) {
      expect(entriesByName.get(root)?.execution, `${root} should be planned for local staging before support`).toBe(
        'stage-locally',
      );
    }

    const rawRegistry = JSON.parse(readText('config/operation-registry.json')) as Array<{ execution?: string }>;
    expect(rawRegistry.some((entry) => entry.execution === 'passthrough')).toBe(false);
  });

  it('keeps scaffold-only marketing and segment roots out of capability routing', () => {
    expect(
      getOperationCapability({ type: 'query', name: 'MarketingActivities', rootFields: ['marketingActivities'] }),
    ).toEqual({
      domain: 'marketing',
      execution: 'overlay-read',
      operationName: 'MarketingActivities',
      type: 'query',
    });

    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'CreateExternalActivity',
        rootFields: ['marketingActivityCreateExternal'],
      }),
    ).toEqual({
      domain: 'unknown',
      execution: 'passthrough',
      operationName: 'CreateExternalActivity',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'query', name: 'Segments', rootFields: ['segments'] })).toEqual({
      domain: 'segments',
      execution: 'overlay-read',
      operationName: 'Segments',
      type: 'query',
    });

    expect(
      getOperationCapability({
        type: 'mutation',
        name: 'CreateSegmentMembersQuery',
        rootFields: ['customerSegmentMembersQueryCreate'],
      }),
    ).toEqual({
      domain: 'unknown',
      execution: 'passthrough',
      operationName: 'CreateSegmentMembersQuery',
      type: 'mutation',
    });

    expect(getOperationCapability({ type: 'mutation', name: 'SegmentCreate', rootFields: ['segmentCreate'] })).toEqual({
      domain: 'segments',
      execution: 'stage-locally',
      operationName: 'SegmentCreate',
      type: 'mutation',
    });
  });

  it('does not create planned-only parity scenarios for scaffold-only marketing and segment roots', () => {
    const scenarios = loadConformanceScenarios(repoRoot);
    const scenarioOperations = new Set(scenarios.flatMap((scenario) => scenario.operationNames));
    const statusDocument = buildConformanceStatusDocument(repoRoot);

    for (const root of scaffoldOnlyMarketingAndSegmentRoots) {
      expect(scenarioOperations.has(root), `${root} should wait for captured evidence or executable comparison`).toBe(
        false,
      );
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(false);
    }

    for (const root of implementedSegmentQueryRoots) {
      expect(scenarioOperations.has(root), `${root} should have executable segment parity coverage`).toBe(true);
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(true);
    }

    for (const root of implementedMarketingQueryRoots) {
      expect(scenarioOperations.has(root), `${root} should have captured marketing read evidence`).toBe(true);
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(true);
    }

    for (const root of implementedSegmentMutationRoots) {
      expect(scenarioOperations.has(root), `${root} should have executable segment mutation parity coverage`).toBe(
        true,
      );
      expect(statusDocument.implementedOperations.some((entry) => entry.name === root)).toBe(true);
    }
  });
});
