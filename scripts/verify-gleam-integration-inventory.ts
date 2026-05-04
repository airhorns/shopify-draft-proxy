import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

type InventoryEntry = {
  source: string;
  gleamTests?: string[];
  paritySpecs?: string[];
  retainedTypeScriptCoverage?: string[];
  retirementReason?: string;
  notes?: string;
};

type InventoryDocument = {
  version: number;
  legacyTypeScriptIntegrationTests: string[];
  entries: InventoryEntry[];
};

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const inventoryPath = path.join(repoRoot, 'config', 'gleam-integration-inventory.json');

function listFiles(root: string, predicate: (file: string) => boolean): string[] {
  const absoluteRoot = path.join(repoRoot, root);

  function walk(directory: string): string[] {
    return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
      const absolutePath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        return walk(absolutePath);
      }

      if (!entry.isFile()) {
        return [];
      }

      const relativePath = path.relative(repoRoot, absolutePath).split(path.sep).join('/');
      return predicate(relativePath) ? [relativePath] : [];
    });
  }

  return walk(absoluteRoot).sort((left, right) => left.localeCompare(right));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readStringArray(record: Record<string, unknown>, key: string): string[] | undefined {
  const value = record[key];
  if (value === undefined) {
    return undefined;
  }
  if (!Array.isArray(value) || value.some((item) => typeof item !== 'string')) {
    throw new Error(`Inventory field ${key} must be an array of strings when present.`);
  }
  return value;
}

function requireStringArray(record: Record<string, unknown>, key: string): string[] {
  const value = readStringArray(record, key);
  if (value === undefined) {
    throw new Error(`Inventory field ${key} is required.`);
  }
  return value;
}

function parseInventory(): InventoryDocument {
  const parsed = JSON.parse(readFileSync(inventoryPath, 'utf8')) as unknown;
  if (!isRecord(parsed)) {
    throw new Error('Inventory must be a JSON object.');
  }
  if (parsed['version'] !== 1) {
    throw new Error('Inventory version must be 1.');
  }
  const legacyTypeScriptIntegrationTests = requireStringArray(parsed, 'legacyTypeScriptIntegrationTests');

  const entriesValue = parsed['entries'];
  if (!Array.isArray(entriesValue)) {
    throw new Error('Inventory entries must be an array.');
  }

  const entries = entriesValue.map((entryValue): InventoryEntry => {
    if (!isRecord(entryValue)) {
      throw new Error('Every inventory entry must be an object.');
    }
    const source = entryValue['source'];
    if (typeof source !== 'string') {
      throw new Error('Every inventory entry needs a string source.');
    }
    const retirementReason = entryValue['retirementReason'];
    if (retirementReason !== undefined && typeof retirementReason !== 'string') {
      throw new Error(`retirementReason for ${source} must be a string.`);
    }
    const notes = entryValue['notes'];
    if (notes !== undefined && typeof notes !== 'string') {
      throw new Error(`notes for ${source} must be a string.`);
    }

    const gleamTests = readStringArray(entryValue, 'gleamTests');
    const paritySpecs = readStringArray(entryValue, 'paritySpecs');
    const retainedTypeScriptCoverage = readStringArray(entryValue, 'retainedTypeScriptCoverage');

    return {
      source,
      ...(gleamTests !== undefined ? { gleamTests } : {}),
      ...(paritySpecs !== undefined ? { paritySpecs } : {}),
      ...(retainedTypeScriptCoverage !== undefined ? { retainedTypeScriptCoverage } : {}),
      ...(retirementReason !== undefined ? { retirementReason } : {}),
      ...(notes !== undefined ? { notes } : {}),
    };
  });

  return { version: 1, legacyTypeScriptIntegrationTests, entries };
}

function assertRepoRelativePath(pathFromRoot: string, label: string): void {
  if (path.isAbsolute(pathFromRoot) || pathFromRoot.includes('..')) {
    throw new Error(`${label} must be a repo-relative path: ${pathFromRoot}`);
  }
}

function assertRelativeFile(pathFromRoot: string, label: string): void {
  assertRepoRelativePath(pathFromRoot, label);
  const absolutePath = path.join(repoRoot, pathFromRoot);
  if (!existsSync(absolutePath) || !statSync(absolutePath).isFile()) {
    throw new Error(`${label} does not exist: ${pathFromRoot}`);
  }
}

function sortedDifference(left: Iterable<string>, right: Iterable<string>): string[] {
  const rightSet = new Set(right);
  return [...left].filter((item) => !rightSet.has(item)).sort((a, b) => a.localeCompare(b));
}

function assertSortedUnique(values: string[], label: string): void {
  const seen = new Set<string>();
  let previous: string | undefined;

  for (const value of values) {
    if (seen.has(value)) {
      throw new Error(`${label} contains duplicate value: ${value}`);
    }
    seen.add(value);

    if (previous !== undefined && previous.localeCompare(value) > 0) {
      throw new Error(`${label} must be sorted alphabetically: ${previous} before ${value}`);
    }
    previous = value;
  }
}

function main(): void {
  const inventory = parseInventory();
  const currentTypeScriptIntegrationTests = listFiles('tests/integration', (file) => file.endsWith('.test.ts'));
  const gleamTestFiles = listFiles('gleam/test', (file) => file.endsWith('.gleam'));
  const legacyIntegrationSet = new Set(inventory.legacyTypeScriptIntegrationTests);
  const gleamTestSet = new Set(gleamTestFiles);
  const seenSources = new Set<string>();

  assertSortedUnique(inventory.legacyTypeScriptIntegrationTests, 'legacyTypeScriptIntegrationTests');
  for (const source of inventory.legacyTypeScriptIntegrationTests) {
    assertRepoRelativePath(source, 'Recorded legacy TypeScript integration test');
    if (!source.startsWith('tests/integration/') || !source.endsWith('.test.ts')) {
      throw new Error(
        `Recorded legacy TypeScript integration test must be a tests/integration/*.test.ts file: ${source}`,
      );
    }
  }

  for (const entry of inventory.entries) {
    if (seenSources.has(entry.source)) {
      throw new Error(`Duplicate inventory source: ${entry.source}`);
    }
    seenSources.add(entry.source);
    if (!legacyIntegrationSet.has(entry.source)) {
      throw new Error(`Inventory source is not in the recorded legacy TypeScript baseline: ${entry.source}`);
    }
    if (!entry.source.startsWith('tests/integration/') || !entry.source.endsWith('.test.ts')) {
      throw new Error(`Inventory source must be a tests/integration/*.test.ts file: ${entry.source}`);
    }
    assertRepoRelativePath(entry.source, 'Inventory source');

    const evidenceCount =
      (entry.gleamTests?.length ?? 0) +
      (entry.paritySpecs?.length ?? 0) +
      (entry.retainedTypeScriptCoverage?.length ?? 0) +
      (entry.retirementReason === undefined ? 0 : 1);

    if (evidenceCount === 0) {
      throw new Error(`Inventory entry has no coverage evidence: ${entry.source}`);
    }

    for (const gleamTest of entry.gleamTests ?? []) {
      assertRelativeFile(gleamTest, `Gleam test evidence for ${entry.source}`);
      if (!gleamTestSet.has(gleamTest)) {
        throw new Error(`Gleam test evidence is outside gleam/test inventory: ${gleamTest}`);
      }
    }
    for (const paritySpec of entry.paritySpecs ?? []) {
      assertRelativeFile(paritySpec, `Parity spec evidence for ${entry.source}`);
      if (!paritySpec.startsWith('config/parity-specs/') || !paritySpec.endsWith('.json')) {
        throw new Error(`Parity evidence must be a config/parity-specs/*.json file: ${paritySpec}`);
      }
    }
    for (const retainedTest of entry.retainedTypeScriptCoverage ?? []) {
      assertRepoRelativePath(retainedTest, `Retained TypeScript evidence for ${entry.source}`);
      if (!legacyIntegrationSet.has(retainedTest)) {
        throw new Error(`Retained TypeScript evidence is outside recorded legacy baseline: ${retainedTest}`);
      }
    }
  }

  const inventorySources = new Set(inventory.entries.map((entry) => entry.source));
  const missingEntries = sortedDifference(legacyIntegrationSet, inventorySources);
  const staleEntries = sortedDifference(inventorySources, legacyIntegrationSet);
  const unrecordedCurrentTests = sortedDifference(currentTypeScriptIntegrationTests, legacyIntegrationSet);

  if (missingEntries.length > 0 || staleEntries.length > 0 || unrecordedCurrentTests.length > 0) {
    const lines = [
      ...missingEntries.map((file) => `Missing inventory entry for recorded legacy test ${file}`),
      ...staleEntries.map((file) => `Inventory entry is outside recorded legacy baseline ${file}`),
      ...unrecordedCurrentTests.map(
        (file) => `Current TypeScript integration test is not in the recorded legacy baseline ${file}`,
      ),
    ];
    throw new Error(lines.join('\n'));
  }

  const gleamEvidence = inventory.entries.reduce((count, entry) => count + (entry.gleamTests?.length ?? 0), 0);
  const parityEvidence = inventory.entries.reduce((count, entry) => count + (entry.paritySpecs?.length ?? 0), 0);
  const retainedEvidence = inventory.entries.reduce(
    (count, entry) => count + (entry.retainedTypeScriptCoverage?.length ?? 0),
    0,
  );
  const retirements = inventory.entries.filter((entry) => entry.retirementReason !== undefined).length;

  process.stdout.write(
    [
      `Verified ${inventory.entries.length} TypeScript integration inventory entries from the recorded legacy baseline.`,
      `Read ${inventory.legacyTypeScriptIntegrationTests.length} recorded legacy TypeScript integration test names.`,
      `Scanned ${currentTypeScriptIntegrationTests.length} current tests/integration/*.test.ts files for unrecorded additions.`,
      `Scanned ${gleamTestFiles.length} gleam/test/**/*.gleam files.`,
      `Evidence links: ${gleamEvidence} Gleam tests, ${parityEvidence} parity specs, ${retainedEvidence} retained TypeScript boundaries, ${retirements} retirements.`,
    ].join('\n') + '\n',
  );
}

main();
