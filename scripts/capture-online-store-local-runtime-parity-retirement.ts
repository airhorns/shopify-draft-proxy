/* oxlint-disable no-console -- CLI scripts intentionally write status to stdout. */

console.log(
  [
    'The synthetic/local-runtime online-store parity specs and fixtures are retired.',
    'This registration exists only so protected-evidence deletion checks can audit the removal.',
    'Runtime coverage for the retired local-only branches lives in Rust integration tests.',
  ].join('\n'),
);
