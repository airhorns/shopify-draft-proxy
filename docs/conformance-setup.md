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
- `SHOPIFY_CONFORMANCE_API_VERSION`
- `SHOPIFY_CONFORMANCE_APP_HANDLE` (optional but useful)
- `SHOPIFY_CONFORMANCE_APP_ID` (optional; lets local inventory-adjust replay mirror `inventoryAdjustmentGroup.app.id`)
- `SHOPIFY_CONFORMANCE_APP_API_KEY` (optional; lets local inventory-adjust replay mirror `inventoryAdjustmentGroup.app.apiKey` when `SHOPIFY_API_KEY` is not set)

See `.env.example` for the canonical variable names.

The server now loads `.env` automatically via `dotenv`, so a local `.env` file is enough for normal `pnpm dev` / `pnpm start` workflows.

The live conformance access token is no longer stored in repo `.env`. The canonical credential lives at:

```text
~/.shopify-draft-proxy/conformance-admin-auth.json
```

Generate a fresh grant link with:

```bash
corepack pnpm conformance:auth-link
```

Then exchange the browser callback URL with:

```bash
corepack pnpm conformance:exchange-auth -- '<full callback url>'
```

### 4. Validate structural conformance coverage before live probing

Before probing the live target, run:

```bash
pnpm conformance:check
pnpm conformance:parity
```

`conformance:check` runs the normal Vitest structural checks for the canonical operation registry, convention-discovered parity specs, captured fixture references, and proxy request files. Any implemented operation must declare runtime-test coverage and be named by at least one discovered parity spec.

There is no central conformance scenario manifest. Standard scenarios are discovered from:

```text
config/parity-specs/*.json
```

Each parity spec is the scenario metadata source and must include:

- `scenarioId`
- `operationNames`
- `scenarioStatus` (`planned` or `captured`)
- `assertionKinds`
- `liveCaptureFiles`
- `proxyRequest.documentPath` / `proxyRequest.variablesPath` when a proxy parity request exists
- `comparison` for captured scenarios that are ready to execute as strict JSON comparisons

Every operation named by a discovered parity spec must exist in `config/operation-registry.json`, and every implemented operation in the registry must be named by at least one discovered parity spec. The scenario-to-operation mapping lives in each parity spec's `operationNames` field; the registry stays focused on runtime capability classification and runtime-test files.

`conformance:parity` executes captured scenarios only after they declare both a proxy request and a strict JSON comparison contract. Valid high-assurance scenarios must compare explicit targets and list every allowed difference as a path-scoped rule with a reason. Use matchers for legitimate nondeterminism such as Shopify IDs, timestamps, and throttle metadata. An `ignore: true` rule means the proxy has not reached parity for that path; it must also set `regrettable: true` and should only be used for hard temporary gaps that will be fixed later.

### 5. Probe the live target before writing parity fixtures

Once the vars are present, run:

```bash
pnpm conformance:probe
```

This performs a minimal Admin GraphQL `shop` query against the configured store and fails fast if the domain/origin/token combination is wrong. Internally it now resolves the token via `getValidConformanceAccessToken(...)`, which probes the stored token, refreshes it when possible, and reports a clear error when the home-folder credential is missing or dead.

### 6. Refresh expiring conformance auth before it strands the repo

The current host now uses a **refreshable expiring store-auth token** persisted in:

- `.manual-store-auth-token.json`
- `.env` (`SHOPIFY_CONFORMANCE_ADMIN_ACCESS_TOKEN`)

When the access token expires, the repo should be repaired with the refresh path first, not by guessing or generating a brand-new auth link unnecessarily.

Run:

```bash
corepack pnpm conformance:refresh-auth
corepack pnpm conformance:probe
```

What `conformance:refresh-auth` does:

- reads the current `refresh_token` + `client_id` from `.manual-store-auth-token.json`
- reads `SHOPIFY_API_SECRET` from the linked app `.env`
  - default candidate: `/tmp/shopify-conformance-app/<SHOPIFY_CONFORMANCE_APP_HANDLE>/.env`
  - override with `SHOPIFY_CONFORMANCE_APP_ENV_PATH=/path/to/app/.env`
- calls:
  - `POST https://<store>/admin/oauth/access_token`
  - form-encoded body with `client_id`, `client_secret`, `grant_type=refresh_token`, and `refresh_token`
- persists the returned rotated token pair back into both:
  - `.manual-store-auth-token.json`
  - `.env`
- verifies the refreshed token immediately with a live `shop` probe

Important current-host findings:

- Shopify's newer refreshable store-auth flow may still return a token shaped like `shpca_...`, not only `shpat_...`
- treat Shopify token families with the broader `^shp[a-z]+_` rule
- those tokens must still be sent as raw `X-Shopify-Access-Token: <token>`
- the refresh response can rotate **both** the access token and refresh token, so persisting them atomically matters

If `conformance:refresh-auth` fails, inspect the returned JSON before retrying. On this host, a repo-local refresh can now fail with `invalid_request` / `This request requires an active refresh_token`, which means the saved manual store-auth grant is no longer refreshable and you should stop retrying it. In that branch, generate a new store-auth link, complete the browser approval flow, exchange the callback in one step, and only then probe again.

### 7. Capture product-domain fixtures from the live store

Run:

```bash
pnpm conformance:capture-products
pnpm conformance:capture-product-mutations
pnpm conformance:capture-product-state-mutations
pnpm conformance:capture-product-option-mutations
pnpm conformance:capture-collections
pnpm conformance:capture-collection-mutations
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

1. Pick the product-first Shopify behavior to model and add/update runtime tests for the proxy behavior.
2. Add or update a parity spec in `config/parity-specs/`; do not edit a central scenario manifest.
3. Add a proxy request pair under `config/parity-requests/` for scenarios ready to replay through the proxy.
4. Add captured live fixtures under `fixtures/conformance/<store-domain>/<api-version>/` when credentials and store safety allow it.
5. Ensure the parity spec's `operationNames` list matches existing root operations in `config/operation-registry.json`.
6. Run `pnpm conformance:check` and `pnpm conformance:parity`.

## Important safety note

Unsupported mutations in the proxy may still passthrough today. Conformance targets must therefore be treated as **safe-to-mutate** stores until passthrough behavior is made safer and more explicit.

## Principle

If a behavior is surprising or underspecified, do not guess forever. Add a conformance scenario against real Shopify and record what actually happens.
