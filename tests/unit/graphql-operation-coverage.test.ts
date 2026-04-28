import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { listOperationRegistryEntries } from '../../src/proxy/operation-registry.js';

const repoRoot = resolve(import.meta.dirname, '../..');
const rootOperationIntrospectionPath = resolve(
  repoRoot,
  'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json',
);

const introspectedFieldSchema = z.strictObject({
  name: z.string().min(1),
});

const rootTypeSchema = z.strictObject({
  name: z.string().min(1),
  fields: z.array(introspectedFieldSchema),
});

const rootOperationIntrospectionFixtureSchema = z.strictObject({
  capturedAt: z.string().min(1),
  storeDomain: z.string().min(1),
  apiVersion: z.string().min(1),
  introspection: z.strictObject({
    data: z.strictObject({
      __schema: z.strictObject({
        queryType: z.strictObject({
          name: z.string().min(1),
        }),
        mutationType: z.strictObject({
          name: z.string().min(1),
        }),
      }),
      queryRoot: rootTypeSchema,
      mutationRoot: rootTypeSchema,
    }),
    extensions: z.unknown().optional(),
  }),
});

type OperationType = 'query' | 'mutation';

function readRootOperationIntrospectionFixture() {
  return rootOperationIntrospectionFixtureSchema.parse(
    JSON.parse(readFileSync(rootOperationIntrospectionPath, 'utf8')) as unknown,
  );
}

function sortedUniqueFieldNames(fields: { name: string }[]): string[] {
  const names = fields.map((field) => field.name).sort((left, right) => left.localeCompare(right));
  expect(new Set(names).size).toBe(names.length);

  return names;
}

function supportedRootOperations(type: OperationType, rootFieldNames: string[]): string[] {
  const implementedRegistryNames = new Set(
    listOperationRegistryEntries()
      .filter((entry) => entry.type === type && entry.implemented)
      .map((entry) => entry.name),
  );

  return rootFieldNames.filter((fieldName) => implementedRegistryNames.has(fieldName));
}

function classifyRootCoverage(type: OperationType, rootFieldNames: string[]) {
  const registryEntries = new Map(
    listOperationRegistryEntries()
      .filter((entry) => entry.type === type)
      .map((entry) => [entry.name, entry]),
  );

  return {
    declaredGaps: rootFieldNames.filter((fieldName) => registryEntries.get(fieldName)?.implemented === false),
    unregistered: rootFieldNames.filter((fieldName) => !registryEntries.has(fieldName)),
  };
}

describe('GraphQL operation coverage', () => {
  it('identifies formally supported root operations from Admin GraphQL introspection', () => {
    const fixture = readRootOperationIntrospectionFixture();
    expect(fixture.introspection.data.__schema.queryType.name).toBe(fixture.introspection.data.queryRoot.name);
    expect(fixture.introspection.data.__schema.mutationType.name).toBe(fixture.introspection.data.mutationRoot.name);

    const queryRootFields = sortedUniqueFieldNames(fixture.introspection.data.queryRoot.fields);
    const mutationRootFields = sortedUniqueFieldNames(fixture.introspection.data.mutationRoot.fields);

    const supportedQueries = supportedRootOperations('query', queryRootFields);
    const supportedMutations = supportedRootOperations('mutation', mutationRootFields);

    expect(queryRootFields.length).toBeGreaterThan(supportedQueries.length);
    expect(mutationRootFields.length).toBeGreaterThan(supportedMutations.length);
    expect(supportedQueries).toEqual(expect.arrayContaining(['product', 'products', 'productsCount']));
    expect(supportedMutations).toEqual(expect.arrayContaining(['productCreate', 'productUpdate', 'productDelete']));
  });

  it('snapshots introspected Admin GraphQL mutations that are not formally supported yet', () => {
    const fixture = readRootOperationIntrospectionFixture();
    const mutationRootFields = sortedUniqueFieldNames(fixture.introspection.data.mutationRoot.fields);
    const supportedMutations = new Set(supportedRootOperations('mutation', mutationRootFields));
    const unsupportedMutations = mutationRootFields.filter((fieldName) => !supportedMutations.has(fieldName));

    expect(unsupportedMutations).toMatchSnapshot();
  });

  it('separates declared mutation gaps from unregistered introspected mutation roots', () => {
    const fixture = readRootOperationIntrospectionFixture();
    const mutationRootFields = sortedUniqueFieldNames(fixture.introspection.data.mutationRoot.fields);

    expect(classifyRootCoverage('mutation', mutationRootFields)).toMatchSnapshot();
  });
});
