import { execFileSync } from 'node:child_process';

import { describe, expect, it } from 'vitest';

describe('repository script policy', () => {
  it('does not contain tracked or pending .mjs files', () => {
    const files = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard'], {
      encoding: 'utf8',
    })
      .split('\n')
      .filter(Boolean);

    const mjsFiles = files.filter((file) => file.endsWith('.mjs'));

    expect(mjsFiles).toEqual([]);
  });
});
