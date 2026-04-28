import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import {
  listAdminPlatformNodeResolverEntries,
  listSupportedAdminPlatformNodeTypes,
} from '../../src/proxy/admin-platform.js';
import { listOperationRegistryEntries } from '../../src/proxy/operation-registry.js';

const repoRoot = resolve(import.meta.dirname, '../..');
const adminPlatformFixturePath = resolve(
  repoRoot,
  'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/admin-platform-utility-roots.json',
);

const nodeTypeSchema = z.strictObject({
  name: z.string().min(1),
});

const nodeCandidateRootFieldSchema = z.strictObject({
  name: z.string().min(1),
  typeName: z.string().min(1),
});

const adminPlatformFixtureSchema = z
  .object({
    introspection: z.object({
      nodeInterface: z.object({
        possibleTypes: z.array(nodeTypeSchema),
      }),
      nodeCandidateRootFields: z.array(nodeCandidateRootFieldSchema),
    }),
  })
  .passthrough();

function readFixture() {
  return adminPlatformFixtureSchema.parse(JSON.parse(readFileSync(adminPlatformFixturePath, 'utf8')) as unknown);
}

function sortedUnique(values: Iterable<string>): string[] {
  return [...new Set(values)].sort((left, right) => left.localeCompare(right));
}

describe('admin platform Node coverage', () => {
  it('classifies every Shopify Node implementor from live introspection', () => {
    const fixture = readFixture();
    const possibleNodeTypes = sortedUnique(fixture.introspection.nodeInterface.possibleTypes.map((type) => type.name));
    const supportedNodeTypes = listSupportedAdminPlatformNodeTypes();
    const unsupportedNodeTypes = possibleNodeTypes.filter((type) => !supportedNodeTypes.includes(type));

    expect(possibleNodeTypes).toEqual(expect.arrayContaining(supportedNodeTypes));
    expect(unsupportedNodeTypes).toMatchSnapshot();
  });

  it('keeps implemented singular Node roots wired into generic node dispatch', () => {
    const fixture = readFixture();
    const supportedNodeTypes = new Set(listSupportedAdminPlatformNodeTypes());
    const implementedQueryRoots = new Set(
      listOperationRegistryEntries()
        .filter((entry) => entry.type === 'query' && entry.implemented)
        .flatMap((entry) => [entry.name, ...entry.matchNames]),
    );
    const resolverRootFields = new Map(
      listAdminPlatformNodeResolverEntries()
        .filter((entry) => entry.rootField !== null)
        .map((entry) => [entry.nodeType, entry.rootField]),
    );

    const implementedRootGaps = fixture.introspection.nodeCandidateRootFields
      .filter((field) => implementedQueryRoots.has(field.name))
      .filter((field) => !supportedNodeTypes.has(field.typeName))
      .map((field) => `${field.name}:${field.typeName}`);

    const resolverRootMismatches = [...resolverRootFields.entries()]
      .filter(([nodeType, rootField]) =>
        fixture.introspection.nodeCandidateRootFields.some(
          (field) => field.typeName === nodeType && field.name !== rootField,
        ),
      )
      .map(([nodeType, rootField]) => `${nodeType}:${rootField}`);

    expect(implementedRootGaps).toMatchSnapshot();
    expect(resolverRootMismatches).toEqual([]);
  });
});
