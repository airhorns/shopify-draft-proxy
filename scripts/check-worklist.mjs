import { readFileSync } from 'node:fs';
import path from 'node:path';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const worklistPath = path.join(repoRoot, 'docs', 'shopify-admin-worklist.md');
const registryPath = path.join(repoRoot, 'config', 'operation-registry.json');

const content = readFileSync(worklistPath, 'utf8');
const registry = JSON.parse(readFileSync(registryPath, 'utf8'));

if (!content.includes('## Product domain')) {
  throw new Error('Worklist must include the product domain section.');
}

const implementedWorklistOperations = new Set(
  content
    .split('\n')
    .filter((line) => line.includes('[x]'))
    .flatMap((line) => Array.from(line.matchAll(/`([^`]+)`/g), (match) => match[1])),
);

for (const entry of registry) {
  if (!entry.implemented) {
    continue;
  }
  if (!implementedWorklistOperations.has(entry.name)) {
    throw new Error(`Implemented registry operation ${entry.name} must appear as [x] in docs/shopify-admin-worklist.md.`);
  }
}

console.log('worklist ok');
