import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { parseOperation } from './support/graphql/parse-operation.js';

type ParsedSummary = {
  type: 'query' | 'mutation';
  name: string | null;
  rootFields: string[];
};

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const parityRequestRoot = path.join(repoRoot, 'config', 'parity-requests');
const gleamParserPath = path.join(
  repoRoot,
  'build',
  'dev',
  'javascript',
  'shopify_draft_proxy',
  'shopify_draft_proxy',
  'graphql',
  'parse_operation.mjs',
);

async function listGraphqlFiles(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const fullPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        return listGraphqlFiles(fullPath);
      }

      return entry.isFile() && entry.name.endsWith('.graphql') ? [fullPath] : [];
    }),
  );
  return nested.flat().sort();
}

function gleamListToArray<T>(list: Iterable<T>): T[] {
  return [...list];
}

function gleamOptionToNullable<T>(option: unknown): T | null {
  if (option && option.constructor?.name === 'Some') {
    return (option as { 0: T })[0];
  }

  return null;
}

function parseGleamResult(gleam: Record<string, unknown>, document: string): ParsedSummary {
  const result = (gleam['parse_operation'] as (document: string) => unknown)(document);
  if (!result || result.constructor?.name !== 'Ok') {
    throw new Error('Gleam parser returned Error');
  }

  const parsed = (result as { 0: { type_: unknown; name: unknown; root_fields: Iterable<string> } })[0];
  const isQuery = (gleam['GraphQLOperationType$isQueryOperation'] as (value: unknown) => boolean)(parsed.type_);
  const isMutation = (gleam['GraphQLOperationType$isMutationOperation'] as (value: unknown) => boolean)(parsed.type_);
  if (!isQuery && !isMutation) {
    throw new Error('Gleam parser returned unsupported operation type');
  }

  return {
    type: isQuery ? 'query' : 'mutation',
    name: gleamOptionToNullable<string>(parsed.name),
    rootFields: gleamListToArray(parsed.root_fields),
  };
}

function normalizeTypeScript(document: string): ParsedSummary {
  const parsed = parseOperation(document);
  return {
    type: parsed.type,
    name: parsed.name,
    rootFields: parsed.rootFields,
  };
}

function summariesEqual(left: ParsedSummary, right: ParsedSummary): boolean {
  return (
    left.type === right.type &&
    left.name === right.name &&
    JSON.stringify(left.rootFields) === JSON.stringify(right.rootFields)
  );
}

const gleam = (await import(pathToFileURL(gleamParserPath).href)) as Record<string, unknown>;
const files = await listGraphqlFiles(parityRequestRoot);
const failures: string[] = [];

for (const file of files) {
  const document = await readFile(file, 'utf8');
  try {
    const typescriptSummary = normalizeTypeScript(document);
    const gleamSummary = parseGleamResult(gleam, document);
    if (!summariesEqual(typescriptSummary, gleamSummary)) {
      failures.push(
        [
          path.relative(repoRoot, file),
          `  TypeScript: ${JSON.stringify(typescriptSummary)}`,
          `  Gleam:      ${JSON.stringify(gleamSummary)}`,
        ].join('\n'),
      );
    }
  } catch (error) {
    failures.push(`${path.relative(repoRoot, file)}\n  ${(error as Error).message}`);
  }
}

if (failures.length > 0) {
  process.stderr.write(`Gleam GraphQL parser parity failed for ${failures.length} request(s):\n`);
  process.stderr.write(`${failures.slice(0, 20).join('\n\n')}\n`);
  process.exit(1);
}

process.stdout.write(
  `Gleam GraphQL parser parity matched TypeScript for ${files.length} parity request document(s).\n`,
);
