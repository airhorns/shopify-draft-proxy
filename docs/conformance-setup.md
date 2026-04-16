# Real Shopify Conformance Setup

This project needs a real Shopify development target to measure fidelity against actual Admin GraphQL behavior.

## Why this exists

The proxy can be implemented without a live Shopify target, but **high-fidelity behavior cannot be validated from guesswork alone**. We need a real dev store plus Admin API credentials to:

- capture true query/mutation response shapes
- verify `userErrors` semantics
- compare null/empty behavior
- study list ordering, handles, timestamps, and derived fields
- build replayable parity fixtures

## What we need

### 1. A Shopify dev/test store
A store we can safely mutate during conformance runs.

### 2. A Shopify app installation with Admin API access
For early conformance work, a single installed app or custom app with a valid Admin API access token is enough.

### 3. Stable configuration
Set these variables in a local `.env` or shell session:

- `SHOPIFY_CONFORMANCE_STORE_DOMAIN`
- `SHOPIFY_CONFORMANCE_ADMIN_ORIGIN`
- `SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN`
- `SHOPIFY_CONFORMANCE_API_VERSION`
- `SHOPIFY_CONFORMANCE_APP_HANDLE` (optional but useful)

See `.env.example` for the canonical variable names.

The server now loads `.env` automatically via `dotenv`, so a local `.env` file is enough for normal `pnpm dev` / `pnpm start` workflows.

### 4. Validate structural conformance coverage before live probing
Before probing the live target, run:

```bash
corepack pnpm conformance:check
corepack pnpm conformance:parity
```

`conformance:check` verifies the canonical operation registry, conformance scenario registry, parity-spec files, worklist sync, and captured fixture references. Any implemented operation must declare runtime-test coverage plus one or more conformance scenario manifests.

`conformance:parity` is the parity-runner scaffold. Today it reports whether each scenario is still planned, captured-but-missing a proxy request spec, or ready for actual proxy-vs-Shopify comparison once the request/comparator details are filled in.

### 5. Probe the live target before writing parity fixtures
Once the vars are present, run:

```bash
corepack pnpm conformance:probe
```

This performs a minimal Admin GraphQL `shop` query against the configured store and fails fast if the domain/origin/token combination is wrong.

### 5. Capture product-domain fixtures from the live store
Run:

```bash
corepack pnpm conformance:capture-products
corepack pnpm conformance:capture-product-mutations
corepack pnpm conformance:capture-product-state-mutations
corepack pnpm conformance:capture-product-option-mutations
corepack pnpm conformance:capture-collections
corepack pnpm conformance:capture-collection-mutations
```

This writes live Admin GraphQL captures under:

```text
fixtures/conformance/<store-domain>/<api-version>/
```

The current capture set records:
- catalog page / cursor sample
- detailed product shape
- variant matrix shape
- search / count behavior
- variant-backed product search samples (`sku:` and, when available in the dev store, `barcode:`)
- top-level `collection(id:)` detail with nested `products` connection fields
- top-level `collections` catalog pagination with nested `products` slices

## Near-term workflow

1. Install Shopify CLI on the host.
2. Authenticate via Shopify CLI device flow.
3. Identify a suitable test app and test store.
4. Record the chosen store domain, API version, and access token source.
5. Add recorder scripts that can issue real Admin GraphQL requests and store fixtures under `fixtures/`.
6. Compare proxy responses against recorded Shopify responses.

## Important safety note

Unsupported mutations in the proxy may still passthrough today. Conformance targets must therefore be treated as **safe-to-mutate** stores until passthrough behavior is made safer and more explicit.

## Principle

If a behavior is surprising or underspecified, do not guess forever. Add a conformance scenario against real Shopify and record what actually happens.
