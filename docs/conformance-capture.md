# Conformance Capture Runner Index

Use the capture index before opening individual capture scripts.

```bash
corepack pnpm conformance:capture
```

The index is backed by `scripts/conformance-capture-index.ts` and is validated against the `scripts/capture-*.(ts|mts)` recorder files on disk, so every capture script has a domain, runner ID, script path, auth-scope note, fixture output target, cleanup note, and expected status checks.

Common lookups:

```bash
corepack pnpm conformance:capture -- --domain products
corepack pnpm conformance:capture -- --script product-mutations
corepack pnpm conformance:capture -- --script scripts/capture-product-mutation-conformance.mts
corepack pnpm conformance:capture -- --json
```

Run a capture through the meta runner when you want the index to supply any documented default environment variables:

```bash
corepack pnpm conformance:capture -- --run product-mutations
corepack pnpm conformance:capture -- --run product-relationship-roots
```

You can also run scripts directly without adding package-level shortcuts:

```bash
corepack pnpm exec tsx ./scripts/capture-product-mutation-conformance.mts
SHOPIFY_CONFORMANCE_API_VERSION=2026-04 corepack pnpm exec tsx ./scripts/capture-product-relationship-roots-conformance.ts
```

Before live capture, always confirm the effective store and credential:

```bash
corepack pnpm conformance:probe
```

After a fixture or parity metadata change, run the checks named by the index entry. Most promoted fixtures should pass:

```bash
corepack pnpm conformance:status
corepack pnpm conformance:check
corepack pnpm gleam:test
```

Entries marked with `manual-capture-review` involve merchant topology, customer-visible side effects, publication/channel setup, delivery settings, or another store-specific condition. Treat those notes as a stop sign for the capture setup: verify the disposable target and cleanup path before recording success-path evidence.

## Cassette re-recording

Capture scripts produce the primary captured response (the `cases` /
`lifecycle` blocks under `fixtures/conformance/**/*.json`). The
companion `upstreamCalls` cassette — the deterministic record of every
upstream GraphQL call the proxy makes while serving each captured
request — is produced by the parity recorder, separately:

```bash
corepack pnpm parity:record <scenario-id>
corepack pnpm parity:record --all
```

The parity recorder boots an in-memory Gleam `DraftProxy` in
`LiveHybrid` mode against real Shopify, plays the spec's primary +
target requests through it, intercepts every upstream call the
operation handlers issue, and writes the result back into the
capture file's `upstreamCalls` field. CI never re-records — that's a
human/agent-driven action backed by the same OAuth credentials the
capture scripts use.

See `docs/parity-runner.md` for the cassette schema and parity model.
Never hand-author `seedX` keys in capture files; that pattern is
banned.
