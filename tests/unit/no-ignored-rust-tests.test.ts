import { describe, expect, it } from 'vitest';

import { findIgnoredRustTestAttributes } from '../../scripts/check-no-ignored-rust-tests.js';

describe('ignored Rust test guardrail', () => {
  it('reports plain and reasoned ignore attributes', () => {
    const violations = findIgnoredRustTestAttributes([
      {
        path: 'tests/example.rs',
        content: [
          '#[test]',
          '#[ignore]',
          'fn skipped() {}',
          '#[test]',
          '#[ignore = "needs a store"]',
          'fn skipped_with_reason() {}',
        ].join('\n'),
      },
    ]);

    expect(violations).toEqual([
      { path: 'tests/example.rs', line: 2, attribute: '#[ignore]' },
      { path: 'tests/example.rs', line: 5, attribute: '#[ignore = "needs a store"]' },
    ]);
  });

  it('reports cfg_attr ignore attributes', () => {
    const violations = findIgnoredRustTestAttributes([
      {
        path: 'tests/cfg.rs',
        content: '#[cfg_attr(feature = "slow", ignore)]\nfn conditionally_skipped() {}',
      },
    ]);

    expect(violations).toEqual([
      {
        path: 'tests/cfg.rs',
        line: 1,
        attribute: '#[cfg_attr(feature = "slow", ignore)]',
      },
    ]);
  });

  it('does not report comments or strings mentioning ignore attributes', () => {
    const violations = findIgnoredRustTestAttributes([
      {
        path: 'src/example.rs',
        content: ['// #[ignore]', 'let text = "#[ignore = \\"example\\"]";', '#[test]', 'fn enforced() {}'].join('\n'),
      },
    ]);

    expect(violations).toEqual([]);
  });
});
