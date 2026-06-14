import { spawnSync } from 'node:child_process';

const scenarioId = 'inventory-quantity-updated-at-and-after-change-local-runtime';

const result = spawnSync('corepack', ['pnpm', 'parity', scenarioId], {
  shell: process.platform === 'win32',
  stdio: 'inherit',
});

process.exit(typeof result.status === 'number' ? result.status : 1);
