# Conformance Capture Command Index

Use the capture index before opening individual capture scripts.

```bash
corepack pnpm conformance:capture:index
```

The index is backed by `scripts/conformance-capture-index.ts` and is validated against `package.json` so every packaged `conformance:capture-*` command has a domain, script path, auth-scope note, fixture output target, cleanup note, and expected status checks.

Common lookups:

```bash
corepack pnpm conformance:capture:index -- --domain products
corepack pnpm conformance:capture:index -- --script conformance:capture-product-mutations
corepack pnpm conformance:capture:index -- --json
```

Before live capture, always confirm the effective store and credential:

```bash
corepack pnpm conformance:probe
```

After a fixture or parity metadata change, run the checks named by the index entry. Most promoted fixtures should pass:

```bash
corepack pnpm conformance:status
corepack pnpm conformance:check
corepack pnpm conformance:parity
```

Entries marked with `manual-capture-review` involve merchant topology, customer-visible side effects, publication/channel setup, delivery settings, or another store-specific condition. Treat those notes as a stop sign for the capture setup: verify the disposable target and cleanup path before recording success-path evidence.
