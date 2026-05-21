import { execFileSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';

describe('Rust parity runner CLI', () => {
  it('discovers the same full parity corpus as main before executing scenarios', () => {
    const output = execFileSync('corepack', ['pnpm', 'parity', '--', '--all', '--dry-run'], {
      cwd: process.cwd(),
      encoding: 'utf8',
    });

    expect(output).toContain('[parity] 910 spec(s) selected');
  });
});
