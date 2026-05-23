import { readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

const ignoredDirectories = new Set(['.git', 'node_modules', 'build', 'target']);
const gleamFiles: string[] = [];

function walk(directory: string): void {
  for (const entry of readdirSync(directory)) {
    if (ignoredDirectories.has(entry)) continue;
    const path = join(directory, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      walk(path);
    } else if (path.endsWith('.gleam')) {
      gleamFiles.push(path.startsWith('./') ? path.slice(2) : path);
    }
  }
}

walk('.');

if (gleamFiles.length === 0) {
  process.stdout.write('No Gleam files remain.\n');
  process.exit(0);
}

process.stdout.write(`Found ${gleamFiles.length} Gleam files.\n`);
for (const path of gleamFiles.slice(0, 50)) process.stdout.write(`- ${path}\n`);
if (gleamFiles.length > 50) process.stdout.write(`... ${gleamFiles.length - 50} more\n`);

if (process.env['RUST_PORT_FINAL'] === '1') {
  process.stderr.write('RUST_PORT_FINAL=1 requires deleting all Gleam files.\n');
  process.exit(1);
}
