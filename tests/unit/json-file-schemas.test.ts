import { readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import {
  graphqlVariablesSchema,
  jsonValueSchema,
  operationRegistrySchema,
  parseJsonFileWithSchema,
  paritySpecSchema,
} from '../../src/json-schemas.js';
import { loadNormalizedStateSnapshot } from '../../src/state/snapshot-loader.js';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function listFiles(directory: string, predicate: (filePath: string) => boolean): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      return listFiles(entryPath, predicate);
    }

    return predicate(entryPath) ? [entryPath] : [];
  });
}

describe('schemaful JSON files', () => {
  it('validates the operation registry through its Zod schema', () => {
    const registry = parseJsonFileWithSchema(
      path.join(repoRoot, 'config/operation-registry.json'),
      operationRegistrySchema,
    );

    expect(registry.length).toBeGreaterThan(0);
    expect(registry.every((entry) => entry.matchNames.length > 0)).toBe(true);
  });

  it('validates every parity spec through the ParitySpec Zod schema', () => {
    const specPaths = listFiles(path.join(repoRoot, 'config/parity-specs'), (filePath) => filePath.endsWith('.json'));

    expect(specPaths.length).toBeGreaterThan(0);
    for (const specPath of specPaths) {
      expect(() => parseJsonFileWithSchema(specPath, paritySpecSchema), specPath).not.toThrow();
    }
  });

  it('rejects parity specs with comparison modes outside the enforced-mode schema', () => {
    expect(
      paritySpecSchema.safeParse({
        scenarioId: 'passive-recording-example',
        comparisonMode: 'passive-recording',
      }).success,
    ).toBe(false);
  });

  it('requires captured parity specs to declare a non-planned comparison mode', () => {
    expect(
      paritySpecSchema.safeParse({
        scenarioId: 'captured-without-mode',
        scenarioStatus: 'captured',
      }).success,
    ).toBe(false);
    expect(
      paritySpecSchema.safeParse({
        scenarioId: 'captured-planned-mode',
        scenarioStatus: 'captured',
        comparisonMode: 'planned',
      }).success,
    ).toBe(false);
  });

  it('rejects parity specs that declare blocker metadata', () => {
    expect(
      paritySpecSchema.safeParse({
        scenarioId: 'blocked-placeholder',
        scenarioStatus: 'planned',
        comparisonMode: 'planned',
        blocker: {
          kind: 'missing-live-scopes',
          blockerPath: null,
          details: {
            linearIssue: 'HAR-204',
          },
        },
      }).success,
    ).toBe(false);
  });

  it('validates every checked-in parity request variables file as GraphQL variables', () => {
    const variablesPaths = listFiles(path.join(repoRoot, 'config/parity-requests'), (filePath) =>
      filePath.endsWith('.variables.json'),
    );

    expect(variablesPaths.length).toBeGreaterThan(0);
    for (const variablesPath of variablesPaths) {
      expect(() => parseJsonFileWithSchema(variablesPath, graphqlVariablesSchema), variablesPath).not.toThrow();
    }
  });

  it('validates every checked-in normalized snapshot through the snapshot loader', () => {
    const snapshotPaths = listFiles(path.join(repoRoot, 'fixtures/snapshots'), (filePath) =>
      filePath.endsWith('.json'),
    );

    expect(snapshotPaths.length).toBeGreaterThan(0);
    for (const snapshotPath of snapshotPaths) {
      expect(() => loadNormalizedStateSnapshot(snapshotPath), snapshotPath).not.toThrow();
    }
  });

  it('validates every checked-in conformance fixture as a JSON document before parity execution reads it', () => {
    const fixturePaths = listFiles(path.join(repoRoot, 'fixtures/conformance'), (filePath) =>
      filePath.endsWith('.json'),
    );

    expect(fixturePaths.length).toBeGreaterThan(0);
    for (const fixturePath of fixturePaths) {
      expect(() => parseJsonFileWithSchema(fixturePath, jsonValueSchema), fixturePath).not.toThrow();
    }
  });

  it('rejects malformed normalized snapshot records at the read boundary', () => {
    const invalidSnapshotPath = path.join(repoRoot, 'tests/fixtures/invalid-normalized-snapshot.fixture');
    expect(() => loadNormalizedStateSnapshot(invalidSnapshotPath)).toThrow(/Invalid normalized snapshot file/);
  });
});
