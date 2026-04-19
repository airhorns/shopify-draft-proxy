import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const scannedRoots = ['.github', 'scripts', 'src', 'tests'];
const scannedExtensions = new Set(['.js', '.mjs', '.cjs', '.ts', '.tsx', '.yml', '.yaml']);
const skippedDirectoryNames = new Set(['.git', 'dist', 'node_modules']);
const restrictedObject = 'console';
const restrictedMethod = 'log';
const restrictedCallPattern = new RegExp(
  `(^|[^A-Za-z0-9_$])${restrictedObject}\\s*\\.\\s*${restrictedMethod}\\s*\\(`,
  'g',
);

function* walk(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (!skippedDirectoryNames.has(entry.name)) {
        yield* walk(fullPath);
      }
      continue;
    }

    if (entry.isFile() && scannedExtensions.has(path.extname(entry.name))) {
      yield fullPath;
    }
  }
}

const violations = [];

for (const root of scannedRoots) {
  const absoluteRoot = path.join(repoRoot, root);
  for (const filePath of walk(absoluteRoot)) {
    const content = readFileSync(filePath, 'utf8');
    for (const match of content.matchAll(restrictedCallPattern)) {
      const line = content.slice(0, match.index).split('\n').length;
      violations.push(`${path.relative(repoRoot, filePath)}:${line}`);
    }
  }
}

if (violations.length > 0) {
  console.error(`Do not call ${restrictedObject}.${restrictedMethod}; use structured logging or explicit stdout helpers.`);
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  process.exit(1);
}
