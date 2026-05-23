# Benchmarks

The Rust runtime benchmark suite exercises common local proxy paths through the
same `DraftProxy::process_request` boundary used by tests and the HTTP adapter.

Run it locally with:

```bash
corepack pnpm rust:bench
```

The suite currently covers:

- meta route dispatch for `/__meta/health`
- supported `productCreate` mutation staging
- product read-after-write from staged local state
- product catalog and count reads from seeded local state

CI runs the same command on every pull request and push to `main`, so benchmark
results are visible in the `Rust proxy path benchmarks` log step. The historical
repository baseline does not contain a recoverable benchmark script or checked-in
benchmark result, so compare future changes against recent CI output from this
suite.
