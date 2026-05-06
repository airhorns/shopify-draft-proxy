import { spawnSync } from 'node:child_process';

const conformanceTestFiles = [
  'tests/unit/conformance-scenario-discovery.test.ts',
  'tests/unit/operation-registry.test.ts',
];

function parseArgs(argv: string[]): { filter: string | null; passthroughArgs: string[] } {
  const passthroughArgs: string[] = [];
  let filter: string | null = null;

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index] ?? '';

    if (arg === '--filter') {
      const next = argv[index + 1];
      if (!next || next.startsWith('-')) {
        throw new Error('Expected a scenario id after --filter.');
      }
      filter = String(next);
      index += 1;
    } else {
      passthroughArgs.push(arg);
    }
  }

  return { filter, passthroughArgs };
}

const { filter, passthroughArgs } = parseArgs(process.argv.slice(2));
const vitestArgs = ['pnpm', 'exec', 'vitest', 'run', ...conformanceTestFiles];

if (filter) {
  vitestArgs.push('-t', filter);
}

vitestArgs.push(...passthroughArgs);

const result = spawnSync('corepack', vitestArgs, {
  stdio: 'inherit',
});

process.exit(result.status ?? 1);
