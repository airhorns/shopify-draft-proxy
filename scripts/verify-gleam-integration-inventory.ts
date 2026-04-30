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

function parseInventory(): InventoryDocument {
  const parsed = JSON.parse(readFileSync(inventoryPath, 'utf8')) as unknown;
  if (!isRecord(parsed)) {
    throw new Error('Inventory must be a JSON object.');
  }
  if (parsed['version'] !== 1) {
    throw new Error('Inventory version must be 1.');
  }
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

  return { version: 1, entries };
}

function assertRelativeFile(pathFromRoot: string, label: string): void {
  if (path.isAbsolute(pathFromRoot) || pathFromRoot.includes('..')) {
    throw new Error(`${label} must be a repo-relative path: ${pathFromRoot}`);
  }
  const absolutePath = path.join(repoRoot, pathFromRoot);
  if (!existsSync(absolutePath) || !statSync(absolutePath).isFile()) {
    throw new Error(`${label} does not exist: ${pathFromRoot}`);
  }
}

function sortedDifference(left: Iterable<string>, right: Iterable<string>): string[] {
  const rightSet = new Set(right);
  return [...left].filter((item) => !rightSet.has(item)).sort((a, b) => a.localeCompare(b));
}

function main(): void {
  const inventory = parseInventory();
  const integrationFiles = listFiles('tests/integration', (file) => file.endsWith('.test.ts'));
  const gleamTestFiles = listFiles('gleam/test', (file) => file.endsWith('.gleam'));
  const integrationSet = new Set(integrationFiles);
  const gleamTestSet = new Set(gleamTestFiles);
  const seenSources = new Set<string>();

  for (const entry of inventory.entries) {
    if (seenSources.has(entry.source)) {
      throw new Error(`Duplicate inventory source: ${entry.source}`);
    }
    seenSources.add(entry.source);
    if (!entry.source.startsWith('tests/integration/') || !entry.source.endsWith('.test.ts')) {
      throw new Error(`Inventory source must be a tests/integration/*.test.ts file: ${entry.source}`);
    }
    assertRelativeFile(entry.source, 'Inventory source');

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
      assertRelativeFile(retainedTest, `Retained TypeScript evidence for ${entry.source}`);
      if (!integrationSet.has(retainedTest)) {
        throw new Error(`Retained TypeScript evidence is outside tests/integration inventory: ${retainedTest}`);
      }
    }
  }

  const inventorySources = new Set(inventory.entries.map((entry) => entry.source));
  const missingEntries = sortedDifference(integrationSet, inventorySources);
  const staleEntries = sortedDifference(inventorySources, integrationSet);

  if (missingEntries.length > 0 || staleEntries.length > 0) {
    const lines = [
      ...missingEntries.map((file) => `Missing inventory entry for ${file}`),
      ...staleEntries.map((file) => `Stale inventory entry for ${file}`),
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
      `Verified ${inventory.entries.length} TypeScript integration inventory entries.`,
      `Scanned ${integrationFiles.length} tests/integration/*.test.ts files.`,
      `Scanned ${gleamTestFiles.length} gleam/test/**/*.gleam files.`,
      `Evidence links: ${gleamEvidence} Gleam tests, ${parityEvidence} parity specs, ${retainedEvidence} retained TypeScript boundaries, ${retirements} retirements.`,
    ].join('\n') + '\n',
  );
}

main();
